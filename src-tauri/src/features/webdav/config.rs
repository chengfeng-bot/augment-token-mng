use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebDavVendor {
    Jianguoyun,
    Infinicloud,
    Koofr,
    Nextcloud,
    SelfHosted,
    Custom,
}

impl Default for WebDavVendor {
    fn default() -> Self {
        Self::Jianguoyun
    }
}

impl WebDavVendor {
    pub fn default_url(self) -> &'static str {
        match self {
            Self::Jianguoyun => "https://dav.jianguoyun.com/dav/",
            Self::Infinicloud => "",
            Self::Koofr => "https://app.koofr.net/dav/Koofr",
            Self::Nextcloud => "https://example.com/remote.php/dav/files/USERNAME/",
            Self::SelfHosted | Self::Custom => "",
        }
    }

    pub fn password_hint_key(self) -> &'static str {
        match self {
            Self::Jianguoyun => "jianguoyun",
            Self::Infinicloud => "infinicloud",
            Self::Koofr => "koofr",
            Self::Nextcloud => "nextcloud",
            Self::SelfHosted => "selfHosted",
            Self::Custom => "custom",
        }
    }

    pub fn propfind_depth(self) -> u8 {
        // 坚果云明确要求不要使用 Depth: infinity；本功能只列单层目录，统一使用 1。
        1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavVendorInfo {
    pub default_url: String,
    pub password_hint_key: String,
    pub propfind_depth: u8,
}

impl From<WebDavVendor> for WebDavVendorInfo {
    fn from(vendor: WebDavVendor) -> Self {
        Self {
            default_url: vendor.default_url().to_string(),
            password_hint_key: vendor.password_hint_key().to_string(),
            propfind_depth: vendor.propfind_depth(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavConfig {
    #[serde(default)]
    pub vendor: WebDavVendor,
    pub url: String,
    pub username: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub password: String,
    #[serde(default)]
    pub password_encrypted: String,
    #[serde(default = "default_remote_dir")]
    pub remote_dir: String,
    #[serde(default = "default_retention_count")]
    pub retention_count: usize,
    #[serde(default)]
    pub enabled: bool,
}

fn default_remote_dir() -> String {
    "ATM".to_string()
}

fn default_retention_count() -> usize {
    1
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            vendor: WebDavVendor::default(),
            url: WebDavVendor::Jianguoyun.default_url().to_string(),
            username: String::new(),
            password: String::new(),
            password_encrypted: String::new(),
            remote_dir: default_remote_dir(),
            retention_count: default_retention_count(),
            enabled: false,
        }
    }
}

impl WebDavConfig {
    pub fn new(
        vendor: WebDavVendor,
        url: String,
        username: String,
        password: String,
        remote_dir: String,
        retention_count: usize,
        enabled: bool,
    ) -> Self {
        let mut config = Self {
            vendor,
            url: normalize_base_url(&url),
            username,
            password: password.clone(),
            password_encrypted: String::new(),
            remote_dir: normalize_remote_dir(&remote_dir),
            retention_count: retention_count.max(1),
            enabled,
        };
        if !password.is_empty() {
            if let Ok(encrypted) = encrypt_secret(&password) {
                config.password_encrypted = encrypted;
            }
        }
        config
    }

    pub fn decrypt_password(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.password_encrypted.is_empty() {
            self.password = decrypt_secret(&self.password_encrypted)?;
        }
        Ok(())
    }

    pub fn to_view(&self) -> WebDavConfigView {
        WebDavConfigView {
            vendor: self.vendor,
            url: self.url.clone(),
            username: self.username.clone(),
            remote_dir: self.remote_dir.clone(),
            retention_count: self.retention_count,
            enabled: self.enabled,
            has_password: !self.password_encrypted.is_empty(),
            vendor_info: self.vendor.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavConfigView {
    pub vendor: WebDavVendor,
    pub url: String,
    pub username: String,
    pub remote_dir: String,
    pub retention_count: usize,
    pub enabled: bool,
    pub has_password: bool,
    pub vendor_info: WebDavVendorInfo,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebDavConfigRequest {
    pub vendor: WebDavVendor,
    pub url: String,
    pub username: String,
    pub password: Option<String>,
    pub remote_dir: String,
    #[serde(default = "default_retention_count")]
    pub retention_count: usize,
    pub enabled: bool,
}

pub struct WebDavConfigManager {
    config_path: PathBuf,
}

impl WebDavConfigManager {
    pub fn new(
        app_handle: &tauri::AppHandle,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let app_data_dir = app_handle.path().app_data_dir()?;
        fs::create_dir_all(&app_data_dir)?;
        Ok(Self {
            config_path: app_data_dir.join("webdav_config.json"),
        })
    }

    pub fn load_config(&self) -> Result<WebDavConfig, Box<dyn std::error::Error + Send + Sync>> {
        if !self.config_path.exists() {
            return Ok(WebDavConfig::default());
        }

        let content = fs::read_to_string(&self.config_path)?;
        let mut config: WebDavConfig = serde_json::from_str(&content)?;
        config.url = normalize_base_url(&config.url);
        config.remote_dir = normalize_remote_dir(&config.remote_dir);
        config.decrypt_password()?;
        Ok(config)
    }

    pub fn save_config(
        &self,
        request: SaveWebDavConfigRequest,
    ) -> Result<WebDavConfig, Box<dyn std::error::Error + Send + Sync>> {
        let existing = self.load_config().ok();
        let password = request
            .password
            .filter(|value| !value.is_empty())
            .or_else(|| existing.as_ref().map(|config| config.password.clone()))
            .unwrap_or_default();

        let config = WebDavConfig::new(
            request.vendor,
            request.url,
            request.username,
            password,
            request.remote_dir,
            request.retention_count,
            request.enabled,
        );
        let json = serde_json::to_string_pretty(&config)?;
        fs::write(&self.config_path, json)?;
        Ok(config)
    }

    pub fn delete_config(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.config_path.exists() {
            fs::remove_file(&self.config_path)?;
        }
        Ok(())
    }
}

pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub fn normalize_remote_dir(remote_dir: &str) -> String {
    remote_dir.trim().trim_matches('/').replace('\\', "/")
}

fn get_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    key[..16].copy_from_slice(b"atm_webdav_conf_");
    key[16..].copy_from_slice(b"encryption_key!!");
    key
}

fn encrypt_secret(secret: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let key = Aes256Gcm::new_from_slice(&get_encryption_key())
        .map_err(|e| format!("Failed to create encryption key: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = key
        .encrypt(nonce, secret.as_bytes())
        .map_err(|e| format!("Failed to encrypt WebDAV password: {}", e))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(hex::encode(result))
}

fn decrypt_secret(encrypted_hex: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let encrypted_data = hex::decode(encrypted_hex)?;
    if encrypted_data.len() < 12 {
        return Err("Invalid encrypted WebDAV password".into());
    }

    let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let key = Aes256Gcm::new_from_slice(&get_encryption_key())
        .map_err(|e| format!("Failed to create decryption key: {}", e))?;
    let plaintext = key
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Failed to decrypt WebDAV password: {}", e))?;
    Ok(String::from_utf8(plaintext)?)
}
