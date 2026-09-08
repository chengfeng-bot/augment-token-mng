//! WorkBuddy 自定义渠道与模型配置。
//!
//! WorkBuddy 使用 `%USERPROFILE%\\.workbuddy\\models.json` 保存一个扁平模型数组。
//! 本模块不把 API key 当作账户凭据，而是按模型原样维护自定义渠道配置。

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

const CONFIG_DIR: &str = ".workbuddy";
const CONFIG_FILE: &str = "models.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkBuddyConfig {
    pub path: String,
    pub models: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkBuddyDetectedModel {
    pub id: String,
    pub name: String,
    pub vendor: String,
}

fn config_path() -> Result<PathBuf, String> {
    let profile = dirs::home_dir().ok_or_else(|| "无法确定当前用户目录".to_string())?;
    Ok(profile.join(CONFIG_DIR).join(CONFIG_FILE))
}

fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn models_endpoint(url: &str) -> String {
    let normalized = normalize_url(url);
    if normalized.ends_with("/models") {
        return normalized;
    }
    for suffix in ["/chat/completions", "/responses", "/completions"] {
        if let Some(base) = normalized.strip_suffix(suffix) {
            return format!("{}/models", base);
        }
    }
    format!("{}/models", normalized)
}

#[tauri::command]
pub async fn workbuddy_get_config() -> Result<WorkBuddyConfig, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(WorkBuddyConfig {
            path: path.display().to_string(),
            models: Vec::new(),
        });
    }
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取 WorkBuddy 配置失败：{}", e))?;
    let models: Vec<Value> = serde_json::from_str(&text)
        .map_err(|e| format!("解析 WorkBuddy models.json 失败：{}", e))?;
    Ok(WorkBuddyConfig {
        path: path.display().to_string(),
        models,
    })
}

#[tauri::command]
pub async fn workbuddy_save_config(models: Vec<Value>) -> Result<WorkBuddyConfig, String> {
    let path = config_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "WorkBuddy 配置目录无效".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| format!("创建 WorkBuddy 配置目录失败：{}", e))?;
    let json = serde_json::to_string_pretty(&models)
        .map_err(|e| format!("序列化 WorkBuddy 配置失败：{}", e))?;
    let temp = parent.join(format!(".models.json.tmp-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&temp, format!("{}\n", json))
        .await
        .map_err(|e| format!("写入临时配置失败：{}", e))?;
    if path.exists() {
        let backup = path.with_extension(format!(
            "json.backup-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ));
        tokio::fs::rename(&path, &backup)
            .await
            .map_err(|e| format!("备份 WorkBuddy 配置失败：{}", e))?;
        if let Err(error) = tokio::fs::rename(&temp, &path).await {
            let _ = tokio::fs::rename(&backup, &path).await;
            return Err(format!("替换 WorkBuddy 配置失败：{}", error));
        }
    } else {
        tokio::fs::rename(&temp, &path)
            .await
            .map_err(|e| format!("写入 WorkBuddy 配置失败：{}", e))?;
    }
    Ok(WorkBuddyConfig {
        path: path.display().to_string(),
        models,
    })
}

#[tauri::command]
pub async fn workbuddy_fetch_models(
    url: String,
    api_key: String,
) -> Result<Vec<WorkBuddyDetectedModel>, String> {
    let normalized = normalize_url(&url);
    if normalized.is_empty() {
        return Err("请先填写模型探测地址".to_string());
    }
    let endpoint = models_endpoint(&normalized);
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    if !api_key.trim().is_empty() {
        let value = format!("Bearer {}", api_key.trim());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::try_from(value).map_err(|_| "API Key 格式无效".to_string())?,
        );
    }
    let response = reqwest::Client::new()
        .get(endpoint)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("探测模型失败：{}", e))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|e| format!("模型接口返回不是有效 JSON：{}", e))?;
    if !status.is_success() {
        return Err(format!("模型接口返回 HTTP {}", status));
    }
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.as_array())
        .ok_or_else(|| "模型接口缺少 data 数组".to_string())?;
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let id = item
            .as_str()
            .map(str::to_string)
            .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_string));
        let Some(id) = id.filter(|v| !v.trim().is_empty()) else {
            continue;
        };
        let key = id.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let vendor = item
            .get("owned_by")
            .or_else(|| item.get("vendor"))
            .and_then(Value::as_str)
            .unwrap_or("Custom")
            .to_string();
        result.push(WorkBuddyDetectedModel { id, name, vendor });
    }
    Ok(result)
}
