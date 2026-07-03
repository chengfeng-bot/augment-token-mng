use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use chrono::Utc;
use rand::{RngCore, rngs::OsRng};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tauri::Manager;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const BACKUP_MAGIC: &[u8; 8] = b"ATMWDAV1";
const KDF_ITERATIONS: u32 = 200_000;
const LOCAL_BACKUP_DIR: &str = "webdav_backups";
const LOCAL_SNAPSHOT_DIR: &str = "local-snapshots";

const KNOWN_ROOT_FILES: &[&str] = &[
    "tokens.json",
    "antigravity_accounts.json",
    "windsurf_accounts.json",
    "cursor_accounts.json",
    "openai_accounts.json",
    "claude_accounts.json",
    "subscription_accounts.json",
    "database_config.json",
    "proxy_config.json",
    "telegram_config.json",
    "raindrop_config.json",
    "notification_records.json",
    "gateway_config.json",
    "gateway_models.json",
    "gateway_custom_models.json",
    "openai_codex_config.json",
    "codex_unsupported_params.json",
    "windsurf_config.json",
    "antigravity_config.json",
    "cursor_config.json",
    "webdav_config.json",
    "bookmarks.db",
    "gateway_usage.db",
    "gptmail_emails.db",
    "hme_emails.db",
    "outlook_credentials.db",
    "antigravity_storage.db",
    "windsurf_storage.db",
    "cursor_storage.db",
    "openai_storage.db",
    "claude_storage.db",
    "subscription_storage.db",
    "bookmark_storage.db",
];

const KNOWN_NESTED_FILES: &[&str] = &["logs/codex_logs.db"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupArchive {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSnapshot {
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub restored_files: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    version: u32,
    app_version: String,
    created_at: String,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    path: String,
    size: u64,
}

#[derive(Debug, Clone)]
struct BackupFile {
    path: PathBuf,
    relative_path: String,
    size: u64,
}

pub fn create_encrypted_backup(
    app: &tauri::AppHandle,
    passphrase: &str,
) -> Result<BackupArchive, String> {
    if passphrase.trim().is_empty() {
        return Err("备份口令不能为空".to_string());
    }

    let created_at = Utc::now().to_rfc3339();
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let zip_bytes = create_plain_zip(app, &created_at)?;
    let encrypted = encrypt_bytes(&zip_bytes, passphrase)?;
    let size = encrypted.len() as u64;

    Ok(BackupArchive {
        file_name: format!("atm-backup-{}.zip.enc", timestamp),
        bytes: encrypted,
        size,
        created_at,
    })
}

pub fn create_local_snapshot(app: &tauri::AppHandle) -> Result<LocalSnapshot, String> {
    let created_at = Utc::now().to_rfc3339();
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    let snapshot_dir = app_data_dir.join(LOCAL_BACKUP_DIR).join(LOCAL_SNAPSHOT_DIR);
    fs::create_dir_all(&snapshot_dir).map_err(|e| format!("创建本地快照目录失败: {}", e))?;

    let zip_bytes = create_plain_zip(app, &created_at)?;
    let snapshot_path = snapshot_dir.join(format!("atm-local-snapshot-{}.zip", timestamp));
    fs::write(&snapshot_path, zip_bytes).map_err(|e| format!("写入本地快照失败: {}", e))?;

    Ok(LocalSnapshot {
        path: snapshot_path.to_string_lossy().to_string(),
        created_at,
    })
}

pub fn restore_encrypted_backup(
    app: &tauri::AppHandle,
    encrypted: &[u8],
    passphrase: &str,
) -> Result<RestoreSummary, String> {
    if passphrase.trim().is_empty() {
        return Err("备份口令不能为空".to_string());
    }
    let zip_bytes = decrypt_bytes(encrypted, passphrase)?;
    restore_zip_bytes(app, &zip_bytes)
}

pub fn restore_snapshot(
    app: &tauri::AppHandle,
    snapshot_path: &str,
) -> Result<RestoreSummary, String> {
    let bytes = fs::read(snapshot_path).map_err(|e| format!("读取本地快照失败: {}", e))?;
    restore_zip_bytes(app, &bytes)
}

fn create_plain_zip(app: &tauri::AppHandle, created_at: &str) -> Result<Vec<u8>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let files = collect_backup_files(&app_data_dir)?;
    checkpoint_sqlite_files(&files);

    let manifest = BackupManifest {
        version: 1,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: created_at.to_string(),
        files: files
            .iter()
            .map(|file| ManifestFile {
                path: file.relative_path.clone(),
                size: file.size,
            })
            .collect(),
    };

    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(|e| format!("序列化备份清单失败: {}", e))?;
    writer
        .start_file("manifest.json", options)
        .map_err(|e| format!("写入备份清单失败: {}", e))?;
    writer
        .write_all(&manifest_json)
        .map_err(|e| format!("写入备份清单失败: {}", e))?;

    for file in files {
        writer
            .start_file(file.relative_path, options)
            .map_err(|e| format!("写入备份文件失败: {}", e))?;
        let mut handle = fs::File::open(&file.path)
            .map_err(|e| format!("读取备份文件失败 {}: {}", file.path.display(), e))?;
        std::io::copy(&mut handle, &mut writer)
            .map_err(|e| format!("写入备份文件失败 {}: {}", file.path.display(), e))?;
    }

    let cursor = writer
        .finish()
        .map_err(|e| format!("完成 zip 归档失败: {}", e))?;
    Ok(cursor.into_inner())
}

fn collect_backup_files(app_data_dir: &Path) -> Result<Vec<BackupFile>, String> {
    let mut files = Vec::new();
    for relative_path in KNOWN_ROOT_FILES.iter().chain(KNOWN_NESTED_FILES.iter()) {
        collect_known_file(app_data_dir, relative_path, &mut files)?;
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn collect_known_file(
    root: &Path,
    relative_path: &str,
    files: &mut Vec<BackupFile>,
) -> Result<(), String> {
    let path = root.join(relative_path);
    push_backup_file(root, path, files)?;

    if is_sqlite_main_file(Path::new(relative_path)) {
        push_backup_file(root, root.join(format!("{}-wal", relative_path)), files)?;
        push_backup_file(root, root.join(format!("{}-shm", relative_path)), files)?;
    }

    Ok(())
}

fn push_backup_file(root: &Path, path: PathBuf, files: &mut Vec<BackupFile>) -> Result<(), String> {
    if !path.exists() || !path.is_file() {
        return Ok(());
    }

    let metadata =
        fs::metadata(&path).map_err(|e| format!("读取文件元数据失败 {}: {}", path.display(), e))?;
    let relative_path = relative_zip_path(root, &path)?;
    files.push(BackupFile {
        path,
        relative_path,
        size: metadata.len(),
    });
    Ok(())
}

fn checkpoint_sqlite_files(files: &[BackupFile]) {
    for file in files {
        if !is_sqlite_main_file(&file.path) {
            continue;
        }
        if let Ok(conn) = Connection::open(&file.path) {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
    }
}

fn is_sqlite_main_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .as_deref(),
        Some("db" | "sqlite" | "sqlite3")
    )
}

fn relative_zip_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|e| format!("计算相对路径失败: {}", e))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn restore_zip_bytes(app: &tauri::AppHandle, zip_bytes: &[u8]) -> Result<RestoreSummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| format!("读取 zip 归档失败: {}", e))?;
    validate_manifest(&mut archive)?;

    let current_files = collect_backup_files(&app_data_dir)?;
    for file in current_files {
        if file.path.exists() {
            fs::remove_file(&file.path)
                .map_err(|e| format!("删除旧数据文件失败 {}: {}", file.path.display(), e))?;
        }
    }

    let mut restored_files = 0usize;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("读取 zip 文件项失败: {}", e))?;
        if file.name() == "manifest.json" || file.is_dir() {
            continue;
        }
        let Some(enclosed_name) = file.enclosed_name() else {
            return Err("备份归档包含不安全路径，已中止恢复".to_string());
        };
        let target_path = app_data_dir.join(enclosed_name);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建恢复目录失败 {}: {}", parent.display(), e))?;
        }
        let mut output = fs::File::create(&target_path)
            .map_err(|e| format!("创建恢复文件失败 {}: {}", target_path.display(), e))?;
        std::io::copy(&mut file, &mut output)
            .map_err(|e| format!("写入恢复文件失败 {}: {}", target_path.display(), e))?;
        restored_files += 1;
    }

    Ok(RestoreSummary { restored_files })
}

fn validate_manifest<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>) -> Result<(), String> {
    let mut manifest_file = archive
        .by_name("manifest.json")
        .map_err(|_| "备份归档缺少 manifest.json".to_string())?;
    let mut manifest_json = String::new();
    manifest_file
        .read_to_string(&mut manifest_json)
        .map_err(|e| format!("读取备份清单失败: {}", e))?;
    let manifest: BackupManifest =
        serde_json::from_str(&manifest_json).map_err(|e| format!("解析备份清单失败: {}", e))?;
    if manifest.version != 1 {
        return Err(format!("不支持的备份版本: {}", manifest.version));
    }
    Ok(())
}

fn encrypt_bytes(bytes: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    let key = derive_key(passphrase, &salt);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("创建加密器失败: {}", e))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, bytes)
        .map_err(|_| "加密备份归档失败".to_string())?;

    let mut output = Vec::with_capacity(BACKUP_MAGIC.len() + 16 + 12 + 4 + ciphertext.len());
    output.extend_from_slice(BACKUP_MAGIC);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&KDF_ITERATIONS.to_be_bytes());
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn decrypt_bytes(bytes: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let header_len = BACKUP_MAGIC.len() + 16 + 12 + 4;
    if bytes.len() <= header_len || &bytes[..BACKUP_MAGIC.len()] != BACKUP_MAGIC {
        return Err("备份文件格式无效".to_string());
    }

    let salt_start = BACKUP_MAGIC.len();
    let nonce_start = salt_start + 16;
    let iterations_start = nonce_start + 12;
    let ciphertext_start = iterations_start + 4;

    let salt = &bytes[salt_start..nonce_start];
    let nonce_bytes = &bytes[nonce_start..iterations_start];
    let iterations = u32::from_be_bytes(
        bytes[iterations_start..ciphertext_start]
            .try_into()
            .map_err(|_| "备份文件 KDF 参数无效".to_string())?,
    );
    if iterations != KDF_ITERATIONS {
        return Err("备份文件 KDF 参数不兼容".to_string());
    }

    let key = derive_key(passphrase, salt);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("创建解密器失败: {}", e))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, &bytes[ciphertext_start..])
        .map_err(|_| "备份口令错误或备份文件已损坏".to_string())
}

fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, KDF_ITERATIONS, &mut key);
    key
}
