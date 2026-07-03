use super::config::{WebDavConfig, normalize_base_url, normalize_remote_dir};
use chrono::{DateTime, Utc};
use quick_xml::Reader;
use quick_xml::events::Event;
use reqwest::{Method, StatusCode};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavBackupItem {
    pub name: String,
    pub size: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Default)]
struct DavResponse {
    href: String,
    size: Option<u64>,
    modified_at: Option<String>,
    is_collection: bool,
}

pub struct WebDavClient {
    config: WebDavConfig,
    client: reqwest::Client,
}

impl WebDavClient {
    pub fn new(config: WebDavConfig) -> Result<Self, String> {
        if config.url.trim().is_empty() {
            return Err("WebDAV URL 不能为空".to_string());
        }
        if config.username.trim().is_empty() {
            return Err("WebDAV 用户名不能为空".to_string());
        }
        if config.password.is_empty() {
            return Err("WebDAV 应用密码不能为空".to_string());
        }

        let client = crate::core::http_client::create_http_client()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
        Ok(Self { config, client })
    }

    pub async fn test_connection(&self) -> Result<(), String> {
        self.ensure_remote_dir().await?;
        Ok(())
    }

    pub async fn ensure_remote_dir(&self) -> Result<(), String> {
        let remote_dir = normalize_remote_dir(&self.config.remote_dir);
        if remote_dir.is_empty() {
            self.propfind_url(&self.base_url(true), 0).await?;
            return Ok(());
        }

        let mut current = String::new();
        for segment in remote_dir.split('/').filter(|part| !part.is_empty()) {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(segment);
            let url = self.build_url(&current, true);
            match self.propfind_url(&url, 0).await {
                Ok(_) => {}
                Err(e) if e.contains("远程路径不存在") => {
                    self.mkcol_url(&url).await?;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    pub async fn upload_backup(&self, file_name: &str, bytes: Vec<u8>) -> Result<(), String> {
        let url = self.backup_file_url(file_name);
        let response = self
            .with_auth(self.client.put(url))
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("上传 WebDAV 备份失败: {}", e))?;

        if response.status().is_success()
            || response.status() == StatusCode::CREATED
            || response.status() == StatusCode::NO_CONTENT
        {
            Ok(())
        } else {
            Err(status_error(response.status(), "上传 WebDAV 备份失败"))
        }
    }

    pub async fn download_backup(&self, file_name: &str) -> Result<Vec<u8>, String> {
        let url = self.backup_file_url(file_name);
        let response = self
            .with_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| format!("下载 WebDAV 备份失败: {}", e))?;

        if response.status().is_success() {
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|e| format!("读取 WebDAV 备份响应失败: {}", e))
        } else {
            Err(status_error(response.status(), "下载 WebDAV 备份失败"))
        }
    }

    pub async fn delete_backup(&self, file_name: &str) -> Result<(), String> {
        let url = self.backup_file_url(file_name);
        let response = self
            .with_auth(self.client.delete(url))
            .send()
            .await
            .map_err(|e| format!("删除 WebDAV 备份失败: {}", e))?;

        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(status_error(response.status(), "删除 WebDAV 备份失败"))
        }
    }

    pub async fn list_backups(&self) -> Result<Vec<WebDavBackupItem>, String> {
        let xml = self
            .propfind_url(&self.backup_dir_url(), self.config.vendor.propfind_depth())
            .await?;
        let mut items = parse_propfind_backups(&xml)?;
        items.sort_by(|a, b| {
            b.modified_at
                .cmp(&a.modified_at)
                .then_with(|| b.name.cmp(&a.name))
        });
        Ok(items)
    }

    pub async fn prune_old_backups(
        &self,
        keep_file_name: &str,
        retention_count: usize,
    ) -> Result<(), String> {
        let backups = self.list_backups().await?;
        let keep_count = retention_count.max(1);
        let mut kept_names = vec![keep_file_name.to_string()];

        for backup in &backups {
            if kept_names.len() >= keep_count {
                break;
            }
            if !kept_names.contains(&backup.name) {
                kept_names.push(backup.name.clone());
            }
        }

        for backup in backups {
            if !kept_names.contains(&backup.name) {
                self.delete_backup(&backup.name).await?;
            }
        }
        Ok(())
    }

    fn base_url(&self, trailing_slash: bool) -> String {
        let base = normalize_base_url(&self.config.url);
        if trailing_slash {
            format!("{}/", base.trim_end_matches('/'))
        } else {
            base
        }
    }

    fn backup_dir_url(&self) -> String {
        self.build_url(&self.config.remote_dir, true)
    }

    fn backup_file_url(&self, file_name: &str) -> String {
        let remote_dir = normalize_remote_dir(&self.config.remote_dir);
        let path = if remote_dir.is_empty() {
            file_name.to_string()
        } else {
            format!("{}/{}", remote_dir, file_name)
        };
        self.build_url(&path, false)
    }

    fn build_url(&self, path: &str, trailing_slash: bool) -> String {
        let base = normalize_base_url(&self.config.url);
        let normalized_path = normalize_remote_dir(path);
        if normalized_path.is_empty() {
            return self.base_url(trailing_slash);
        }

        let encoded = normalized_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(urlencoding::encode)
            .collect::<Vec<_>>()
            .join("/");
        if trailing_slash {
            format!("{}/{}/", base, encoded)
        } else {
            format!("{}/{}", base, encoded)
        }
    }

    async fn propfind_url(&self, url: &str, depth: u8) -> Result<String, String> {
        let method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:getcontentlength/>
    <D:getlastmodified/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;
        let response = self
            .with_auth(
                self.client
                    .request(method, url)
                    .header("Depth", depth.to_string())
                    .header("Content-Type", "application/xml; charset=utf-8")
                    .body(body.to_string()),
            )
            .send()
            .await
            .map_err(|e| format!("WebDAV PROPFIND 请求失败: {}", e))?;

        if response.status().is_success() || response.status().as_u16() == 207 {
            response
                .text()
                .await
                .map_err(|e| format!("读取 WebDAV PROPFIND 响应失败: {}", e))
        } else {
            Err(status_error(response.status(), "WebDAV PROPFIND 请求失败"))
        }
    }

    async fn mkcol_url(&self, url: &str) -> Result<(), String> {
        let method = Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;
        let response = self
            .with_auth(self.client.request(method, url))
            .send()
            .await
            .map_err(|e| format!("创建 WebDAV 目录失败: {}", e))?;

        if response.status().is_success()
            || response.status() == StatusCode::CREATED
            || response.status() == StatusCode::METHOD_NOT_ALLOWED
        {
            Ok(())
        } else {
            Err(status_error(response.status(), "创建 WebDAV 目录失败"))
        }
    }

    fn with_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.basic_auth(&self.config.username, Some(&self.config.password))
    }
}

fn parse_propfind_backups(xml: &str) -> Result<Vec<WebDavBackupItem>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut current_tag: Option<Vec<u8>> = None;
    let mut current: Option<DavResponse> = None;
    let mut responses = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref()).to_vec();
                if name.as_slice() == b"response" {
                    current = Some(DavResponse::default());
                } else if is_text_tag(name.as_slice()) {
                    current_tag = Some(name);
                } else if name.as_slice() == b"collection" {
                    if let Some(item) = current.as_mut() {
                        item.is_collection = true;
                    }
                }
            }
            Ok(Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == b"collection" {
                    if let Some(item) = current.as_mut() {
                        item.is_collection = true;
                    }
                }
            }
            Ok(Event::Text(text)) => {
                if let (Some(item), Some(tag)) = (current.as_mut(), current_tag.as_deref()) {
                    let value = String::from_utf8_lossy(text.as_ref()).to_string();
                    match tag {
                        b"href" => item.href = value,
                        b"getcontentlength" => item.size = value.parse::<u64>().ok(),
                        b"getlastmodified" => {
                            item.modified_at = parse_http_date(&value).or(Some(value));
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref()).to_vec();
                if name.as_slice() == b"response" {
                    if let Some(item) = current.take() {
                        responses.push(item);
                    }
                }
                if is_text_tag(name.as_slice()) {
                    current_tag = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("解析 WebDAV 响应失败: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(responses
        .into_iter()
        .filter(|item| !item.is_collection)
        .filter_map(|item| {
            let name = href_file_name(&item.href)?;
            if !name.starts_with("atm-backup-") || !name.ends_with(".zip.enc") {
                return None;
            }
            Some(WebDavBackupItem {
                name,
                size: item.size.unwrap_or(0),
                modified_at: item.modified_at,
            })
        })
        .collect())
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn is_text_tag(name: &[u8]) -> bool {
    name == b"href" || name == b"getcontentlength" || name == b"getlastmodified"
}

fn href_file_name(href: &str) -> Option<String> {
    let raw = href.trim_end_matches('/').rsplit('/').next()?;
    urlencoding::decode(raw).map(|value| value.to_string()).ok()
}

fn parse_http_date(value: &str) -> Option<String> {
    DateTime::parse_from_rfc2822(value)
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
        .ok()
}

fn status_error(status: StatusCode, prefix: &str) -> String {
    let message = match status.as_u16() {
        401 => "认证失败，请确认账号和应用专用密码",
        403 => "服务器拒绝访问，请确认账号权限和远程目录",
        404 => "远程路径不存在",
        405 => "服务器不允许该 WebDAV 操作，请确认 WebDAV 已开启",
        409 => "远程父目录不存在或路径冲突",
        423 => "远程文件被锁定，请稍后重试",
        429 => "请求过于频繁，服务商已限流，请稍后重试",
        _ => "服务器返回异常状态",
    };
    format!("{}: {} (HTTP {})", prefix, message, status.as_u16())
}
