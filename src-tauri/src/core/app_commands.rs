use arboard::Clipboard;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::http_client;

// Update check structures
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    has_update: bool,
    download_url: String,
    release_notes: Option<String>,
}

// RSS Feed 解析结构
#[derive(Debug, Deserialize)]
struct Feed {
    entry: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Entry {
    id: String,
    title: String,
    link: Link,
    content: Option<Content>,
}

#[derive(Debug, Deserialize)]
struct Link {
    #[serde(rename = "@href")]
    href: String,
}

#[derive(Debug, Deserialize)]
struct Content {
    #[serde(rename = "$value")]
    value: String,
}

// Version comparison helper
fn compare_versions(current: &str, latest: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };

    let current_parts = parse_version(current);
    let latest_parts = parse_version(latest);

    for i in 0..std::cmp::max(current_parts.len(), latest_parts.len()) {
        let current_part = current_parts.get(i).unwrap_or(&0);
        let latest_part = latest_parts.get(i).unwrap_or(&0);

        if latest_part > current_part {
            return true;
        } else if latest_part < current_part {
            return false;
        }
    }

    false
}

#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");

    // 使用 GitHub RSS Feed，避免 API 速率限制
    let client = http_client::create_proxy_client()?;
    let response = client
        .get("https://github.com/cubezhao/ai-tools-mng/releases.atom")
        .header("User-Agent", "Mozilla/5.0 (compatible; ATM-App/1.0)")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch RSS feed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub RSS feed returned status: {}",
            response.status()
        ));
    }

    let xml_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read RSS feed: {}", e))?;

    // 解析 XML
    let feed: Feed = quick_xml::de::from_str(&xml_text)
        .map_err(|e| format!("Failed to parse RSS feed: {}", e))?;

    // 获取第一个 entry (最新版本)
    let latest_entry = feed.entry.first().ok_or("No releases found in RSS feed")?;

    // 从 id 中提取版本号 (格式: tag:github.com,2008:Repository/.../v1.2.3)
    let latest_version = latest_entry
        .id
        .split('/')
        .last()
        .ok_or("Invalid release ID format")?
        .trim_start_matches('v');

    let has_update = compare_versions(current_version, latest_version);

    // 构建 GitHub Release 页面 URL
    let download_url = latest_entry.link.href.clone();

    // 从 content 中提取 release notes (如果有)
    let release_notes = latest_entry.content.as_ref().map(|c| c.value.clone());

    Ok(UpdateInfo {
        current_version: current_version.to_string(),
        latest_version: latest_version.to_string(),
        has_update,
        download_url,
        release_notes,
    })
}

#[tauri::command]
pub async fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard =
        Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

#[tauri::command]
pub async fn close_window(app: AppHandle, window_label: String) -> Result<(), String> {
    if let Some(window) = app.get_window(&window_label) {
        window
            .close()
            .map_err(|e| format!("Failed to close window: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    // Open folder using system default file manager
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&app_data_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&app_data_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&app_data_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn create_jetbrains_token_file(
    editor_type: String,
    token_data: String,
) -> Result<String, String> {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    // 获取用户主目录
    let home_dir = env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .map_err(|_| "Failed to get home directory".to_string())?;

    let augment_dir = PathBuf::from(&home_dir).join(".augment");

    // 确保 .augment 目录存在
    fs::create_dir_all(&augment_dir)
        .map_err(|e| format!("Failed to create .augment directory: {}", e))?;

    // 创建文件路径
    let file_name = format!("{}_token.json", editor_type);
    let file_path = augment_dir.join(&file_name);

    // 写入文件
    fs::write(&file_path, token_data).map_err(|e| format!("Failed to write token file: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn configure_vim_augment(
    access_token: String,
    tenant_url: String,
) -> Result<String, String> {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    // 获取用户主目录
    let home_dir = env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .map_err(|_| "Failed to get home directory".to_string())?;

    // 配置文件路径 (所有系统统一使用 .local/share/vim-augment)
    let vim_augment_dir = PathBuf::from(&home_dir)
        .join(".local")
        .join("share")
        .join("vim-augment");

    // 确保目录存在
    fs::create_dir_all(&vim_augment_dir)
        .map_err(|e| format!("Failed to create vim-augment directory: {}", e))?;

    let secrets_path = vim_augment_dir.join("secrets.json");

    // 如果文件已存在，先删除
    if secrets_path.exists() {
        fs::remove_file(&secrets_path)
            .map_err(|e| format!("Failed to remove existing secrets.json: {}", e))?;
    }

    // 构建内层 JSON 对象
    let inner_json = serde_json::json!({
        "accessToken": access_token,
        "tenantURL": tenant_url,
        "scopes": ["email"]
    });

    // 将内层 JSON 转换为字符串（这会自动转义引号）
    let inner_json_str = serde_json::to_string(&inner_json)
        .map_err(|e| format!("Failed to serialize inner JSON: {}", e))?;

    // 构建外层 JSON 对象
    let outer_json = serde_json::json!({
        "augment.sessions": inner_json_str
    });

    // 将外层 JSON 转换为格式化的字符串
    let json_content = serde_json::to_string_pretty(&outer_json)
        .map_err(|e| format!("Failed to serialize outer JSON: {}", e))?;

    // 写入文件
    fs::write(&secrets_path, json_content)
        .map_err(|e| format!("Failed to write secrets.json: {}", e))?;

    // 在 Unix 系统上设置文件权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&secrets_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .permissions();
        perms.set_mode(0o600); // 仅所有者可读写
        fs::set_permissions(&secrets_path, perms)
            .map_err(|e| format!("Failed to set file permissions: {}", e))?;
    }

    Ok(secrets_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn configure_auggie(access_token: String, tenant_url: String) -> Result<String, String> {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    // 获取用户主目录
    let home_dir = env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .map_err(|_| "Failed to get home directory".to_string())?;

    // 确定 .augment 目录路径
    let augment_dir = PathBuf::from(&home_dir).join(".augment");

    // 确保目录存在
    fs::create_dir_all(&augment_dir)
        .map_err(|e| format!("Failed to create .augment directory: {}", e))?;

    let session_path = augment_dir.join("session.json");

    // 如果文件已存在，先删除
    if session_path.exists() {
        fs::remove_file(&session_path)
            .map_err(|e| format!("Failed to remove existing session.json: {}", e))?;
    }

    // 构建 JSON 对象
    let session_json = serde_json::json!({
        "accessToken": access_token,
        "tenantURL": tenant_url,
        "scopes": ["read", "write"]
    });

    // 将 JSON 转换为格式化的字符串
    let json_content = serde_json::to_string_pretty(&session_json)
        .map_err(|e| format!("Failed to serialize session JSON: {}", e))?;

    // 写入文件
    fs::write(&session_path, json_content)
        .map_err(|e| format!("Failed to write session.json: {}", e))?;

    // 在 Unix 系统上设置文件权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&session_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .permissions();
        perms.set_mode(0o600); // 仅所有者可读写
        fs::set_permissions(&session_path, perms)
            .map_err(|e| format!("Failed to set file permissions: {}", e))?;
    }

    Ok(session_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn open_editor_with_protocol(app: AppHandle, protocol_url: String) -> Result<(), String> {
    println!("Opening editor with protocol URL: {}", protocol_url);

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(protocol_url, None::<&str>)
        .map_err(|e| format!("Failed to open editor with protocol: {}", e))
}
