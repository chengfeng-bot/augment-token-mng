use tauri::Manager;

use crate::AppState;
use crate::platforms::openai::models::{Account, AccountType, QuotaData};
use crate::platforms::openai::modules::token_coordinator::OAuthTokenCoordinator;
use crate::platforms::openai::modules::{oauth, quota};

const TOKEN_REFRESH_WINDOW_SECS: i64 = 300;

struct QuotaFetchResult {
    quota: QuotaData,
    account: Account,
}

/// 使用统一 Token Coordinator 查询配额；仅在首次 401 后刷新并重试一次。
async fn fetch_quota_with_retry(
    app: &tauri::AppHandle,
    account_id: &str,
) -> Result<QuotaFetchResult, String> {
    let coordinator = app.state::<AppState>().openai_token_coordinator.clone();
    let resolved = coordinator
        .ensure_fresh(account_id, TOKEN_REFRESH_WINDOW_SECS)
        .await
        .map_err(|error| error.to_string())?;
    if resolved.account.account_type == AccountType::API {
        return Err("API accounts do not support quota fetching".to_string());
    }
    let access_token = resolved
        .account
        .token
        .as_ref()
        .map(|t| t.access_token.clone())
        .ok_or_else(|| "OAuth account missing token".to_string())?;
    match quota::fetch_quota(
        &access_token,
        resolved.account.chatgpt_account_id.as_deref(),
    )
    .await
    {
        Ok(quota) => Ok(QuotaFetchResult {
            quota,
            account: resolved.account,
        }),
        Err(error) if error.is_unauthorized() => {
            let refreshed = coordinator
                .refresh_after_unauthorized(account_id, &access_token)
                .await
                .map_err(|refresh_error| refresh_error.to_string())?;
            let new_access_token = refreshed
                .account
                .token
                .as_ref()
                .map(|token| token.access_token.clone())
                .ok_or_else(|| "OAuth account missing token".to_string())?;
            let quota = quota::fetch_quota(
                &new_access_token,
                refreshed.account.chatgpt_account_id.as_deref(),
            )
            .await
            .map_err(|retry_error| retry_error.to_string())?;
            Ok(QuotaFetchResult {
                quota,
                account: refreshed.account,
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

/// 刷新配额并以 patch 方式写回最新账号，避免覆盖并发轮换后的 Token。
pub async fn refresh_quota_and_backfill(
    app: &tauri::AppHandle,
    account_id: &str,
) -> Result<Account, String> {
    let coordinator = app.state::<AppState>().openai_token_coordinator.clone();
    let _quota_guard = coordinator.lock_quota_operation(account_id).await;
    refresh_quota_and_backfill_locked(app, account_id, &coordinator).await
}

async fn refresh_quota_and_backfill_locked(
    app: &tauri::AppHandle,
    account_id: &str,
    coordinator: &OAuthTokenCoordinator,
) -> Result<Account, String> {
    let base_interval = configured_quota_refresh_interval(app);
    let fetched = match fetch_quota_with_retry(app, account_id).await {
        Ok(fetched) => fetched,
        Err(error) => {
            let recorded_error = error.clone();
            let _ = coordinator
                .update_account(account_id, "quota-refresh-failed", move |account| {
                    account.quota_refresh.record_failure(
                        chrono::Utc::now().timestamp(),
                        base_interval,
                        recorded_error,
                    );
                })
                .await;
            return Err(error);
        }
    };
    let mut enriched = fetched.account.clone();
    backfill_openai_auth_json_if_missing(&mut enriched);

    if missing_subscription_expiry(&enriched) {
        if let Some(access_token) = enriched
            .token
            .as_ref()
            .map(|token| token.access_token.clone())
        {
            oauth::enrich_openai_auth_json_with_account_check(
                &access_token,
                enriched.organization_id.as_deref(),
                enriched.chatgpt_account_id.as_deref(),
                &mut enriched.openai_auth_json,
            )
            .await;
        }
    }

    let quota = fetched.quota;
    let openai_auth_json = enriched.openai_auth_json;
    coordinator
        .update_account(account_id, "quota-refresh", move |account| {
            let now = chrono::Utc::now().timestamp();
            let exhausted = quota.is_exhausted();
            let reset_after = [
                quota.codex_5h_reset_after_seconds,
                quota.codex_7d_reset_after_seconds,
            ]
            .into_iter()
            .flatten()
            .filter(|seconds| *seconds > 0)
            .min();
            account.update_quota(quota);
            account.quota_refresh.record_success(now, base_interval);
            if exhausted {
                if let Some(reset_after) = reset_after {
                    let reset_at = now.saturating_add(reset_after.max(60)).saturating_add(5);
                    let normal_next = now.saturating_add(base_interval.min(i64::MAX as u64) as i64);
                    account.quota_refresh.next_check_at = Some(reset_at.min(normal_next));
                    account.quota_refresh.stale_after = Some(reset_at);
                }
            }
            if openai_auth_json.is_some() {
                account.openai_auth_json = openai_auth_json;
            }
        })
        .await
        .map_err(|error| error.to_string())
}

fn configured_quota_refresh_interval(app: &tauri::AppHandle) -> u64 {
    let state = app.state::<AppState>();
    state
        .codex_server_config
        .lock()
        .ok()
        .and_then(|config| {
            config
                .as_ref()
                .map(|config| config.quota_refresh_interval_seconds)
        })
        .unwrap_or(30 * 60)
        .max(60)
}

pub(crate) fn missing_subscription_expiry(account: &Account) -> bool {
    let expiry_str = account
        .openai_auth_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .and_then(|value| {
            value
                .get("chatgpt_subscription_active_until")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
        });

    match expiry_str {
        // 无订阅到期数据，需要补充
        None => true,
        Some(expiry) => {
            // JWT id_token 中的订阅到期时间是 token 签发时的快照。
            // 如果存储的到期日已过，可能已经自动续费，需要重新通过 API 获取。
            chrono::DateTime::parse_from_rfc3339(&expiry)
                .map(|dt| dt < chrono::Utc::now())
                .unwrap_or(false)
        }
    }
}

/// 消费一张限流重置券；使用 ensureFresh，首次 401 后刷新并重试一次。
pub async fn consume_reset_credit(
    app: &tauri::AppHandle,
    account_id: &str,
) -> Result<Account, String> {
    let coordinator = app.state::<AppState>().openai_token_coordinator.clone();
    let _quota_guard = coordinator.lock_quota_operation(account_id).await;
    let resolved = coordinator
        .ensure_fresh(account_id, TOKEN_REFRESH_WINDOW_SECS)
        .await
        .map_err(|error| error.to_string())?;
    if resolved.account.account_type == AccountType::API {
        return Err("API accounts do not support reset credits".to_string());
    }

    let access_token = resolved
        .account
        .token
        .as_ref()
        .map(|t| t.access_token.clone())
        .ok_or_else(|| "OAuth account missing token".to_string())?;
    let consume = quota::consume_reset_credit(
        &access_token,
        resolved.account.chatgpt_account_id.as_deref(),
    )
    .await;
    if let Err(error) = consume {
        if !error.is_unauthorized() {
            return Err(error.to_string());
        }
        let refreshed = coordinator
            .refresh_after_unauthorized(account_id, &access_token)
            .await
            .map_err(|refresh_error| refresh_error.to_string())?;
        let new_access_token = refreshed
            .account
            .token
            .as_ref()
            .map(|token| token.access_token.clone())
            .ok_or_else(|| "OAuth account missing token".to_string())?;
        quota::consume_reset_credit(
            &new_access_token,
            refreshed.account.chatgpt_account_id.as_deref(),
        )
        .await
        .map_err(|retry_error| retry_error.to_string())?;
    };

    refresh_quota_and_backfill_locked(app, account_id, &coordinator).await
}

fn has_empty_openai_auth_json(account: &Account) -> bool {
    account
        .openai_auth_json
        .as_ref()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
}

/// 用当前 token 中的 id_token 解析并更新 account.openai_auth_json（订阅到期、套餐等字段）。
/// 解析逻辑在 oauth::extract_openai_auth_json：解码 JWT payload，取出 "https://api.openai.com/auth" 并序列化为 JSON。
/// 可在刷新配额/刷新 token 后调用，保证订阅信息与 id_token 一致。
pub fn backfill_openai_auth_json_if_missing(account: &mut Account) -> bool {
    let id_token = account.token.as_ref().and_then(|t| t.id_token.as_deref());
    let Some(id_token) = id_token else {
        return false;
    };
    let Some(auth_json) = oauth::extract_openai_auth_json(id_token) else {
        return false;
    };
    let was_empty = has_empty_openai_auth_json(account);
    account.openai_auth_json = Some(auth_json);
    was_empty
}
