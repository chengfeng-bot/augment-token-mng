use super::archive::{
    BackupArchive, LocalSnapshot, RestoreSummary, create_encrypted_backup, create_local_snapshot,
    restore_encrypted_backup, restore_snapshot,
};
use super::client::{WebDavBackupItem, WebDavClient};
use super::config::{SaveWebDavConfigRequest, WebDavConfig, WebDavConfigManager, WebDavConfigView};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavBackupResult {
    pub file_name: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavRestoreResult {
    pub restored_file: String,
    pub restored_files: usize,
    pub local_snapshot_path: String,
}

#[tauri::command]
pub async fn webdav_load_config(app: tauri::AppHandle) -> Result<WebDavConfigView, String> {
    let manager =
        WebDavConfigManager::new(&app).map_err(|e| format!("创建 WebDAV 配置管理器失败: {}", e))?;
    let config = manager
        .load_config()
        .map_err(|e| format!("读取 WebDAV 配置失败: {}", e))?;
    Ok(config.to_view())
}

#[tauri::command]
pub async fn webdav_save_config(
    request: SaveWebDavConfigRequest,
    app: tauri::AppHandle,
) -> Result<WebDavConfigView, String> {
    let manager =
        WebDavConfigManager::new(&app).map_err(|e| format!("创建 WebDAV 配置管理器失败: {}", e))?;
    let config = manager
        .save_config(request)
        .map_err(|e| format!("保存 WebDAV 配置失败: {}", e))?;
    Ok(config.to_view())
}

#[tauri::command]
pub async fn webdav_delete_config(app: tauri::AppHandle) -> Result<(), String> {
    let manager =
        WebDavConfigManager::new(&app).map_err(|e| format!("创建 WebDAV 配置管理器失败: {}", e))?;
    manager
        .delete_config()
        .map_err(|e| format!("删除 WebDAV 配置失败: {}", e))
}

#[tauri::command]
pub async fn webdav_test_connection(
    request: SaveWebDavConfigRequest,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let config = config_from_request_or_existing(&app, request)?;
    let client = WebDavClient::new(config)?;
    client.test_connection().await
}

#[tauri::command]
pub async fn webdav_backup_now(
    passphrase: String,
    app: tauri::AppHandle,
) -> Result<WebDavBackupResult, String> {
    let config = load_enabled_config(&app)?;
    let retention_count = config.retention_count;
    let client = WebDavClient::new(config)?;
    client.ensure_remote_dir().await?;

    let backup: BackupArchive = create_encrypted_backup(&app, &passphrase)?;
    let file_name = backup.file_name.clone();
    let result = WebDavBackupResult {
        file_name: backup.file_name.clone(),
        size: backup.size,
        created_at: backup.created_at.clone(),
    };

    client
        .upload_backup(&backup.file_name, backup.bytes)
        .await?;
    client
        .prune_old_backups(&file_name, retention_count)
        .await?;
    Ok(result)
}

#[tauri::command]
pub async fn webdav_list_backups(app: tauri::AppHandle) -> Result<Vec<WebDavBackupItem>, String> {
    let config = load_enabled_config(&app)?;
    let client = WebDavClient::new(config)?;
    client.list_backups().await
}

#[tauri::command]
pub async fn webdav_restore(
    file_name: String,
    passphrase: String,
    app: tauri::AppHandle,
) -> Result<WebDavRestoreResult, String> {
    if file_name.trim().is_empty() {
        return Err("请选择要恢复的备份".to_string());
    }

    let config = load_enabled_config(&app)?;
    let client = WebDavClient::new(config)?;
    let encrypted = client.download_backup(&file_name).await?;

    // 先验证口令和归档格式，确认可读后再创建本地快照并覆盖。
    let snapshot: LocalSnapshot = create_local_snapshot(&app)?;
    let restored = match restore_encrypted_backup(&app, &encrypted, &passphrase) {
        Ok(summary) => summary,
        Err(error) => {
            let _rollback: Result<RestoreSummary, String> = restore_snapshot(&app, &snapshot.path);
            return Err(format!("恢复失败，已尝试使用本地快照回滚: {}", error));
        }
    };

    Ok(WebDavRestoreResult {
        restored_file: file_name,
        restored_files: restored.restored_files,
        local_snapshot_path: snapshot.path,
    })
}

#[tauri::command]
pub async fn webdav_delete_backup(file_name: String, app: tauri::AppHandle) -> Result<(), String> {
    if file_name.trim().is_empty() {
        return Err("请选择要删除的备份".to_string());
    }
    let config = load_enabled_config(&app)?;
    let client = WebDavClient::new(config)?;
    client.delete_backup(&file_name).await
}

fn config_from_request_or_existing(
    app: &tauri::AppHandle,
    request: SaveWebDavConfigRequest,
) -> Result<WebDavConfig, String> {
    let manager =
        WebDavConfigManager::new(app).map_err(|e| format!("创建 WebDAV 配置管理器失败: {}", e))?;
    let existing = manager.load_config().ok();
    let password = request
        .password
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| existing.as_ref().map(|config| config.password.clone()))
        .unwrap_or_default();

    Ok(WebDavConfig::new(
        request.vendor,
        request.url,
        request.username,
        password,
        request.remote_dir,
        request.retention_count,
        request.enabled,
    ))
}

fn load_enabled_config(app: &tauri::AppHandle) -> Result<WebDavConfig, String> {
    let manager =
        WebDavConfigManager::new(app).map_err(|e| format!("创建 WebDAV 配置管理器失败: {}", e))?;
    let config = manager
        .load_config()
        .map_err(|e| format!("读取 WebDAV 配置失败: {}", e))?;
    if !config.enabled {
        return Err("WebDAV 备份尚未启用".to_string());
    }
    Ok(config)
}
