use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};
use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, OwnedRwLockWriteGuard, RwLock};

use crate::AppState;
use crate::platforms::openai::models::{Account, AccountType, TokenData};
use crate::platforms::openai::modules::{account as account_module, oauth, storage};

#[derive(Debug, Error)]
pub enum OAuthTokenError {
    #[error("{0}")]
    Storage(String),
    #[error("Account is not an OAuth account")]
    NotOAuthAccount,
    #[error("OAuth account missing token")]
    MissingToken,
    #[error("No refresh token available")]
    MissingRefreshToken,
    #[error("OAuth refresh token is invalid: {0}")]
    InvalidRefreshToken(String),
    #[error("OAuth refresh task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Refresh(#[from] oauth::OAuthRefreshError),
}

#[derive(Debug, Clone)]
pub struct TokenResolution {
    pub account: Account,
    pub refreshed: bool,
}

#[derive(Default)]
struct EntryState {
    pending_account: Option<Account>,
}

#[derive(Clone, Copy)]
enum RefreshPolicy<'a> {
    EnsureFresh { min_ttl_secs: i64 },
    Rejected { access_token: &'a str },
    Force,
}

pub struct OAuthTokenCoordinator {
    entries: Mutex<HashMap<String, Arc<AsyncMutex<EntryState>>>>,
    quota_entries: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    storage_gate: Arc<RwLock<()>>,
    app: tauri::AppHandle,
}

impl OAuthTokenCoordinator {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            quota_entries: Mutex::new(HashMap::new()),
            storage_gate: Arc::new(RwLock::new(())),
            app,
        }
    }

    pub async fn lock_storage_sync(&self) -> OwnedRwLockWriteGuard<()> {
        self.storage_gate.clone().write_owned().await
    }

    pub async fn lock_quota_operation(&self, account_id: &str) -> OwnedMutexGuard<()> {
        let entry = {
            let mut entries = self
                .quota_entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };
        entry.lock_owned().await
    }

    pub async fn ensure_fresh(
        &self,
        account_id: &str,
        min_ttl_secs: i64,
    ) -> Result<TokenResolution, OAuthTokenError> {
        self.resolve(
            account_id,
            RefreshPolicy::EnsureFresh {
                min_ttl_secs: min_ttl_secs.max(0),
            },
            "ensure-fresh",
        )
        .await
    }

    pub async fn refresh_after_unauthorized(
        &self,
        account_id: &str,
        rejected_access_token: &str,
    ) -> Result<TokenResolution, OAuthTokenError> {
        self.resolve(
            account_id,
            RefreshPolicy::Rejected {
                access_token: rejected_access_token,
            },
            "unauthorized",
        )
        .await
    }

    pub async fn force_refresh(
        &self,
        account_id: &str,
    ) -> Result<TokenResolution, OAuthTokenError> {
        self.resolve(account_id, RefreshPolicy::Force, "manual-force")
            .await
    }

    pub async fn update_account<F>(
        &self,
        account_id: &str,
        source: &str,
        update: F,
    ) -> Result<Account, OAuthTokenError>
    where
        F: FnOnce(&mut Account),
    {
        let _storage_guard = self.storage_gate.read().await;
        let entry = self.entry(account_id);
        let mut state = entry.lock().await;
        let mut account = self.load_latest(account_id, &mut state).await?;
        update(&mut account);
        account.updated_at = chrono::Utc::now().timestamp();
        self.persist(&account, &mut state).await?;
        self.publish_account_update(&account, source).await;
        Ok(account)
    }

    /// 保存来自 UI/导入路径的账号元数据，同时保留 Coordinator 中更新的凭据与配额。
    pub async fn save_account(
        &self,
        mut incoming: Account,
        source: &str,
    ) -> Result<Account, OAuthTokenError> {
        let _storage_guard = self.storage_gate.read().await;
        let account_id = incoming.id.clone();
        let entry = self.entry(&account_id);
        let mut state = entry.lock().await;
        let latest = if let Some(pending) = state.pending_account.take() {
            Some(pending)
        } else {
            storage::load_optional_account(&self.app, &account_id)
                .await
                .map_err(OAuthTokenError::Storage)?
        };

        if let Some(latest) = latest {
            if latest.account_type == AccountType::OAuth
                && incoming.account_type == AccountType::OAuth
            {
                incoming.token = latest.token;
                incoming.rt_invalid = latest.rt_invalid;
                incoming.rt_invalid_reason = latest.rt_invalid_reason;
                incoming.quota = latest.quota;
                incoming.quota_refresh = latest.quota_refresh;
                incoming.openai_auth_json = latest.openai_auth_json;
                incoming.last_used = incoming.last_used.max(latest.last_used);
            }
            incoming.version = latest.version;
        }
        incoming.updated_at = chrono::Utc::now().timestamp();
        self.persist(&incoming, &mut state).await?;
        self.publish_account_update(&incoming, source).await;
        Ok(incoming)
    }

    pub async fn delete_account(
        &self,
        account_id: &str,
        source: &str,
    ) -> Result<bool, OAuthTokenError> {
        let _storage_guard = self.storage_gate.read().await;
        let entry = self.entry(account_id);
        let mut state = entry.lock().await;
        let deleted = storage::delete_account(&self.app, account_id)
            .await
            .map_err(OAuthTokenError::Storage)?;
        state.pending_account = None;
        self.publish_account_deleted(account_id, source).await;
        Ok(deleted)
    }

    async fn resolve(
        &self,
        account_id: &str,
        policy: RefreshPolicy<'_>,
        source: &str,
    ) -> Result<TokenResolution, OAuthTokenError> {
        let storage_guard = self.storage_gate.clone().read_owned().await;
        let entry = self.entry(account_id);
        let mut state = entry.lock_owned().await;
        let mut account = self.load_latest(account_id, &mut state).await?;

        if account.account_type != AccountType::OAuth {
            return Err(OAuthTokenError::NotOAuthAccount);
        }
        if account.rt_invalid && !matches!(policy, RefreshPolicy::Force) {
            return Err(OAuthTokenError::InvalidRefreshToken(
                account
                    .rt_invalid_reason
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            ));
        }

        let current_token = account
            .token
            .as_ref()
            .ok_or(OAuthTokenError::MissingToken)?;
        let should_refresh = match &policy {
            RefreshPolicy::EnsureFresh { min_ttl_secs } => {
                oauth::token_needs_refresh(current_token, *min_ttl_secs)
            }
            RefreshPolicy::Rejected { access_token } => current_token.access_token == *access_token,
            RefreshPolicy::Force => true,
        };

        if !should_refresh {
            return Ok(TokenResolution {
                account,
                refreshed: false,
            });
        }

        let refresh_token = current_token
            .refresh_token
            .clone()
            .ok_or(OAuthTokenError::MissingRefreshToken)?;
        let current_refresh_token = current_token.refresh_token.clone();
        let current_id_token = current_token.id_token.clone();
        let app = self.app.clone();
        let source = source.to_string();

        tokio::spawn(async move {
            let _storage_guard = storage_guard;
            let response = match oauth::refresh_token(&refresh_token).await {
                Ok(response) => response,
                Err(error) => {
                    if let Some(reason) = error.invalid_reason() {
                        account.rt_invalid = true;
                        account.rt_invalid_reason = Some(reason.as_str().to_string());
                        account.updated_at = chrono::Utc::now().timestamp();
                        persist_account(&app, &account, &mut state).await?;
                        publish_account_update(&app, &account, &source).await;
                    }
                    return Err(OAuthTokenError::Refresh(error));
                }
            };

            let now = chrono::Utc::now().timestamp();
            account.token = Some(TokenData::new(
                response.access_token,
                response.refresh_token.or(current_refresh_token),
                response.id_token.or(current_id_token),
                response.expires_in,
                now + response.expires_in,
                response.token_type,
            ));
            account.rt_invalid = false;
            account.rt_invalid_reason = None;
            account.updated_at = now;
            account_module::backfill_openai_auth_json_if_missing(&mut account);
            persist_account(&app, &account, &mut state).await?;
            publish_account_update(&app, &account, &source).await;

            Ok(TokenResolution {
                account,
                refreshed: true,
            })
        })
        .await
        .map_err(|error| OAuthTokenError::Task(error.to_string()))?
    }

    async fn publish_account_update(&self, account: &Account, source: &str) {
        publish_account_update(&self.app, account, source).await;
    }

    async fn publish_account_deleted(&self, account_id: &str, source: &str) {
        publish_account_deleted(&self.app, account_id, source).await;
    }

    fn entry(&self, account_id: &str) -> Arc<AsyncMutex<EntryState>> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries
            .entry(account_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(EntryState::default())))
            .clone()
    }

    async fn load_latest(
        &self,
        account_id: &str,
        state: &mut EntryState,
    ) -> Result<Account, OAuthTokenError> {
        if let Some(pending) = state.pending_account.take() {
            self.persist(&pending, state).await?;
            return Ok(pending);
        }

        storage::load_account(&self.app, account_id)
            .await
            .map_err(OAuthTokenError::Storage)
    }

    async fn persist(
        &self,
        account: &Account,
        state: &mut EntryState,
    ) -> Result<(), OAuthTokenError> {
        persist_account(&self.app, account, state).await
    }
}

async fn persist_account(
    app: &tauri::AppHandle,
    account: &Account,
    state: &mut EntryState,
) -> Result<(), OAuthTokenError> {
    let mut to_save = account.clone();
    to_save.version = 0;
    match storage::save_account(app, &to_save).await {
        Ok(()) => {
            state.pending_account = None;
            Ok(())
        }
        Err(error) => {
            state.pending_account = Some(account.clone());
            Err(OAuthTokenError::Storage(error))
        }
    }
}

async fn publish_account_update(app: &tauri::AppHandle, account: &Account, source: &str) {
    let pool = {
        let state = app.state::<AppState>();
        state.codex_pool.lock().ok().and_then(|guard| guard.clone())
    };
    if let Some(pool) = pool {
        pool.sync_account(account).await;
    }

    let _ = app.emit(
        "openai-accounts-updated",
        serde_json::json!({
            "source": source,
            "account_ids": [account.id.clone()],
            "timestamp": chrono::Utc::now().timestamp(),
        }),
    );
}

async fn publish_account_deleted(app: &tauri::AppHandle, account_id: &str, source: &str) {
    let pool = {
        let state = app.state::<AppState>();
        state.codex_pool.lock().ok().and_then(|guard| guard.clone())
    };
    if let Some(pool) = pool {
        pool.remove_account(account_id).await;
    }

    let _ = app.emit(
        "openai-accounts-updated",
        serde_json::json!({
            "source": source,
            "account_ids": [account_id],
            "timestamp": chrono::Utc::now().timestamp(),
        }),
    );
}
