use super::traits::{AccountStorage, StorageError, SyncableAccount};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, Weak};
use tauri::Manager;

const SCHEMA_VERSION: i32 = 2;
static STORAGE_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn default_schema_version() -> i32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeletedRecord {
    id: String,
    version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountStore<T> {
    #[serde(default = "default_schema_version")]
    schema_version: i32,
    #[serde(default)]
    version: i64,
    current_account_id: Option<String>,
    accounts: Vec<T>,
    #[serde(default)]
    deletions: Vec<DeletedRecord>,
}

impl<T> Default for AccountStore<T> {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            version: 0,
            current_account_id: None,
            accounts: Vec::new(),
            deletions: Vec::new(),
        }
    }
}

impl<T> AccountStore<T> {
    fn next_version(&mut self) -> i64 {
        self.version += 1;
        self.version
    }
}

/// 通用本地文件存储
pub struct GenericLocalStorage<T: SyncableAccount> {
    storage_path: PathBuf,
    lock: Arc<Mutex<()>>,
    _phantom: PhantomData<T>,
}

impl<T: SyncableAccount> GenericLocalStorage<T> {
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, StorageError> {
        let app_data_dir = app_handle.path().app_data_dir()?;
        fs::create_dir_all(&app_data_dir)?;

        let storage_path = app_data_dir.join(T::storage_file_name());
        Ok(Self {
            lock: storage_lock(&storage_path),
            storage_path,
            _phantom: PhantomData,
        })
    }

    pub fn new_with_path(storage_path: PathBuf) -> Self {
        Self {
            lock: storage_lock(&storage_path),
            storage_path,
            _phantom: PhantomData,
        }
    }

    fn read_store(&self) -> Result<AccountStore<T>, StorageError> {
        let _guard = self.lock.lock().unwrap();
        self.read_store_unlocked()
    }

    fn read_store_unlocked(&self) -> Result<AccountStore<T>, StorageError> {
        if !self.storage_path.exists() {
            return Ok(AccountStore::default());
        }

        let content = fs::read_to_string(&self.storage_path)?;
        if content.trim().is_empty() {
            return Ok(AccountStore::default());
        }

        let store: AccountStore<T> = serde_json::from_str(&content)?;
        if store.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "Unsupported {} account schema version: {}",
                T::platform_name(),
                store.schema_version
            )
            .into());
        }

        Ok(store)
    }

    fn write_store(&self, store: &AccountStore<T>) -> Result<(), StorageError> {
        let _guard = self.lock.lock().unwrap();
        self.write_store_unlocked(store)
    }

    fn write_store_unlocked(&self, store: &AccountStore<T>) -> Result<(), StorageError> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = self.storage_path.with_extension("tmp");
        let content = serde_json::to_string_pretty(store)?;

        fs::write(&temp_path, content)?;

        match fs::rename(&temp_path, &self.storage_path) {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&temp_path);
                Err(format!("Failed to rename temp file: {}", e).into())
            }
        }
    }

    pub async fn get_current_account_id(&self) -> Result<Option<String>, StorageError> {
        let store = self.read_store()?;
        Ok(store.current_account_id)
    }

    pub async fn set_current_account_id(&self, id: Option<String>) -> Result<(), StorageError> {
        let _guard = self.lock.lock().unwrap();
        let mut store = self.read_store_unlocked()?;
        store.current_account_id = id;
        self.write_store_unlocked(&store)
    }

    pub async fn replace_all(
        &self,
        mut accounts: Vec<T>,
        deletions: Vec<String>,
        version: i64,
        current_account_id: Option<String>,
    ) -> Result<(), StorageError> {
        for account in &mut accounts {
            account.set_deleted(false);
        }

        let store = AccountStore {
            schema_version: SCHEMA_VERSION,
            version,
            current_account_id,
            accounts,
            deletions: deletions
                .into_iter()
                .map(|id| DeletedRecord { id, version: 0 })
                .collect(),
        };

        self.write_store(&store)
    }

    pub fn get_local_version(&self) -> Result<i64, StorageError> {
        let store = self.read_store()?;
        Ok(store.version)
    }

    pub fn get_deletions(&self) -> Result<Vec<String>, StorageError> {
        let store = self.read_store()?;
        Ok(store.deletions.iter().map(|d| d.id.clone()).collect())
    }
}

fn storage_lock(path: &PathBuf) -> Arc<Mutex<()>> {
    let mut locks = STORAGE_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.clone(), Arc::downgrade(&lock));
    lock
}

#[async_trait::async_trait]
impl<T: SyncableAccount> AccountStorage<T> for GenericLocalStorage<T> {
    async fn save_account(&self, account: &T) -> Result<(), StorageError> {
        let _guard = self.lock.lock().unwrap();
        let mut store = self.read_store_unlocked()?;
        let mut account = account.clone();
        account.set_deleted(false);

        if account.version() <= 0 {
            account.set_version(store.next_version());
        } else if account.version() > store.version {
            store.version = account.version();
        }

        let account_id = account.id().to_string();
        store.accounts.retain(|a| a.id() != account_id);
        store.accounts.push(account);
        store.deletions.retain(|d| d.id != account_id);

        self.write_store_unlocked(&store)
    }

    async fn load_accounts(&self) -> Result<Vec<T>, StorageError> {
        let store = self.read_store()?;
        Ok(store
            .accounts
            .into_iter()
            .filter(|a| !a.is_deleted())
            .collect())
    }

    async fn get_account(&self, id: &str) -> Result<Option<T>, StorageError> {
        let store = self.read_store()?;
        Ok(store
            .accounts
            .into_iter()
            .find(|a| a.id() == id && !a.is_deleted()))
    }

    async fn update_account(&self, account: &T) -> Result<(), StorageError> {
        self.save_account(account).await
    }

    async fn delete_account(&self, id: &str) -> Result<bool, StorageError> {
        let _guard = self.lock.lock().unwrap();
        let mut store = self.read_store_unlocked()?;
        let initial_len = store.accounts.len();

        store.accounts.retain(|a| a.id() != id);
        store.deletions.retain(|d| d.id != id);

        let version = store.next_version();
        store.deletions.push(DeletedRecord {
            id: id.to_string(),
            version,
        });

        if store.current_account_id.as_deref() == Some(id) {
            store.current_account_id = store.accounts.first().map(|a| a.id().to_string());
        }

        self.write_store_unlocked(&store)?;
        Ok(store.accounts.len() < initial_len)
    }

    async fn clear_all_accounts(&self) -> Result<(), StorageError> {
        self.write_store(&AccountStore::default())
    }

    fn storage_type(&self) -> &'static str {
        match T::platform_name() {
            "antigravity" => "antigravity_local_file",
            "windsurf" => "windsurf_local_file",
            _ => "generic_local_file",
        }
    }

    async fn is_available(&self) -> bool {
        self.storage_path
            .parent()
            .map(|p| p.exists() || fs::create_dir_all(p).is_ok())
            .unwrap_or(false)
    }
}
