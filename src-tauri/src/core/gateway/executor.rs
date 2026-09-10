//! 网关渠道执行器
//!
//! 按 `ChannelKind` 选取凭证并向对应上游发起单次请求：CodexOauth 复用 OpenAI OAuth
//! 账号与 `codex::upstream` 工具函数（不经 `CodexPool`），OpenaiCompat / Anthropic 走
//! Base URL + Key。失败切换（failover）由路由层依据候选优先级编排，本层只负责单渠道执行。

use bytes::Bytes;
use reqwest::Method;
use serde_json::{Value, json};
use tauri::Manager;
use warp::http::HeaderMap;

use super::canonical::CanonicalRequest;
use super::config::{ChannelKind, GatewayChannel};
use super::translate::stream_bridge::outbound_for;
use crate::AppState;
use crate::platforms::openai::codex::upstream::{
    CODEX_UPSTREAM_ORIGIN, apply_forward_headers, build_upstream_url, format_transport_error,
};
use crate::platforms::openai::models::Account;
use crate::proxy_helper::ProxyClient;

/// 允许从网关入站请求传递到 Codex 上游的会话/客户端元数据头。
const CODEX_FORWARDED_HEADERS: [&str; 8] = [
    "version",
    "x-codex-beta-features",
    "x-codex-turn-metadata",
    "x-client-request-id",
    "x-codex-window-id",
    "thread-id",
    "session-id",
    "x-openai-internal-codex-responses-lite",
];

/// ChatGPT Codex Responses 上游不接受或不适用于 OAuth 后端的顶层字段。
const CODEX_UNSUPPORTED_FIELDS: [&str; 13] = [
    "max_output_tokens",
    "max_completion_tokens",
    "temperature",
    "top_p",
    "truncation",
    "prompt_cache_options",
    "prompt_cache_retention",
    "context_management",
    "user",
    "safety_identifier",
    "previous_response_id",
    "generate",
    "stream_options",
];

/// 渠道执行错误
#[derive(Debug)]
pub enum GatewayError {
    /// 凭证缺失/解析失败（账号不存在、缺少 Base URL/Key 等）
    Credential(String),
    /// 传输层错误（连接/超时等）
    Transport(String),
}

impl GatewayError {
    pub fn message(&self) -> String {
        match self {
            GatewayError::Credential(m) => m.clone(),
            GatewayError::Transport(m) => m.clone(),
        }
    }
}

/// 网关渠道执行器
pub struct GatewayExecutor {
    app_handle: tauri::AppHandle,
    client: ProxyClient,
}

impl GatewayExecutor {
    /// 复用调用方传入的客户端（由 `AppState` 缓存），避免每请求重建客户端与重复握手
    pub fn new(app_handle: tauri::AppHandle, client: ProxyClient) -> Self {
        Self { app_handle, client }
    }

    /// 经渠道出站转换器把 canonical 请求构造为渠道请求体并发送（标准跨协议路径）
    pub async fn send_canonical(
        &self,
        channel: &GatewayChannel,
        req: &CanonicalRequest,
        headers: &HeaderMap,
    ) -> Result<reqwest::Response, GatewayError> {
        let body = outbound_for(channel.wire()).build_request(req);
        let bytes = Bytes::from(
            serde_json::to_vec(&body)
                .map_err(|e| GatewayError::Credential(format!("渠道请求体序列化失败: {}", e)))?,
        );
        self.send(channel, bytes, headers).await
    }

    /// 将已序列化的渠道请求体发往指定渠道，返回上游响应（含非成功状态码，交由路由层判定）
    pub async fn send(
        &self,
        channel: &GatewayChannel,
        body: Bytes,
        headers: &HeaderMap,
    ) -> Result<reqwest::Response, GatewayError> {
        match channel.kind {
            ChannelKind::CodexOauth => self.send_codex(channel, body, headers).await,
            ChannelKind::OpenaiCompat => self.send_openai_compat(channel, body).await,
            ChannelKind::Anthropic => self.send_anthropic(channel, body).await,
        }
    }

    /// CodexOauth：复用 OpenAI OAuth 账号与 codex 上游构造工具，转发前校验/刷新 token
    async fn send_codex(
        &self,
        channel: &GatewayChannel,
        body: Bytes,
        headers: &HeaderMap,
    ) -> Result<reqwest::Response, GatewayError> {
        let account_id = channel
            .account_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| GatewayError::Credential("Codex 渠道未绑定账号".into()))?;

        let coordinator = self
            .app_handle
            .state::<AppState>()
            .openai_token_coordinator
            .clone();
        let resolved = coordinator
            .ensure_fresh(account_id, 300)
            .await
            .map_err(|error| GatewayError::Credential(format!("Token 刷新失败: {}", error)))?;
        let rejected_access_token = resolved
            .account
            .token
            .as_ref()
            .map(|token| token.access_token.clone())
            .ok_or_else(|| GatewayError::Credential("OAuth 账号缺少 token".into()))?;

        let url = build_upstream_url(CODEX_UPSTREAM_ORIGIN, "/backend-api/codex/responses", None);
        // 统一规整为 ChatGPT Codex OAuth 后端可接受的 Responses 请求体。
        let body = normalize_codex_body(body);
        let headers = select_codex_headers(headers);
        let response = self
            .send_codex_once(&url, body.clone(), &headers, &resolved.account)
            .await?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            self.persist_codex_forbidden(&coordinator, account_id, response.status())
                .await;
            return Ok(response);
        }

        let refreshed = match coordinator
            .refresh_after_unauthorized(account_id, &rejected_access_token)
            .await
        {
            Ok(resolution) => resolution,
            Err(_) => return Ok(response),
        };
        let response = self
            .send_codex_once(&url, body, &headers, &refreshed.account)
            .await?;
        self.persist_codex_forbidden(&coordinator, account_id, response.status())
            .await;
        Ok(response)
    }

    async fn send_codex_once(
        &self,
        url: &str,
        body: Bytes,
        headers: &HeaderMap,
        account: &Account,
    ) -> Result<reqwest::Response, GatewayError> {
        let access_token = account
            .token
            .as_ref()
            .map(|token| token.access_token.as_str())
            .ok_or_else(|| GatewayError::Credential("OAuth 账号缺少 token".into()))?;
        let chatgpt_account_id = account
            .chatgpt_account_id
            .as_deref()
            .unwrap_or(account.email.as_str());
        let builder = self.client.request(Method::POST, url);
        let builder = apply_forward_headers(builder, headers, access_token, chatgpt_account_id)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .body(body);
        send_builder(builder).await
    }

    async fn persist_codex_forbidden(
        &self,
        coordinator: &crate::platforms::openai::modules::token_coordinator::OAuthTokenCoordinator,
        account_id: &str,
        status: reqwest::StatusCode,
    ) {
        if !matches!(
            status,
            reqwest::StatusCode::PAYMENT_REQUIRED | reqwest::StatusCode::FORBIDDEN
        ) {
            return;
        }

        let _ = coordinator
            .update_account(account_id, "gateway-upstream-forbidden", |account| {
                account
                    .quota
                    .get_or_insert_with(Default::default)
                    .is_forbidden = true;
            })
            .await;
    }

    /// OpenaiCompat：Base URL + Bearer Key，按线型选 /chat/completions 或 /responses
    async fn send_openai_compat(
        &self,
        channel: &GatewayChannel,
        body: Bytes,
    ) -> Result<reqwest::Response, GatewayError> {
        let base = require_base(channel)?;
        let key = require_key(channel)?;
        let path = if channel.wire.as_deref() == Some("responses") {
            "/responses"
        } else {
            "/chat/completions"
        };
        let url = if base.to_ascii_lowercase().ends_with(path) {
            base.clone()
        } else {
            format!("{}{}", base, path)
        };
        let body = normalize_openai_compat_body(channel, body);
        let builder = self
            .client
            .post(&url)
            .bearer_auth(key)
            .header("Content-Type", "application/json")
            .body(body);
        send_builder(builder).await
    }

    /// Anthropic：Base URL + x-api-key，POST /v1/messages
    async fn send_anthropic(
        &self,
        channel: &GatewayChannel,
        body: Bytes,
    ) -> Result<reqwest::Response, GatewayError> {
        let base = require_base(channel)?;
        let key = require_key(channel)?;
        let url = format!("{}/v1/messages", base);
        let builder = self
            .client
            .post(&url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .body(body);
        send_builder(builder).await
    }
}

/// 发送请求并将传输层错误映射为 `GatewayError`
async fn send_builder(builder: reqwest::RequestBuilder) -> Result<reqwest::Response, GatewayError> {
    builder
        .send()
        .await
        .map_err(|e| GatewayError::Transport(format_transport_error(&e)))
}

/// 规整 Base URL（去尾斜杠）并校验非空
fn require_base(channel: &GatewayChannel) -> Result<String, GatewayError> {
    let base = channel
        .base_url
        .as_deref()
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GatewayError::Credential("渠道缺少 Base URL".into()))?;
    Ok(base.to_string())
}

/// 校验 API Key 非空
fn require_key(channel: &GatewayChannel) -> Result<String, GatewayError> {
    channel
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| GatewayError::Credential("渠道缺少 API Key".into()))
}

/// OpenAI 兼容渠道的轻量兼容层。
fn normalize_openai_compat_body(channel: &GatewayChannel, body: Bytes) -> Bytes {
    let Ok(mut root) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(obj) = root.as_object_mut() else {
        return body;
    };

    let base = channel
        .base_url
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let model = obj
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let zhipu_like = base.contains("bigmodel")
        || base.contains("zhipu")
        || base.contains("zai")
        || model.starts_with("glm-");
    if !zhipu_like {
        return body;
    }

    if let Some(choice) = obj.get("tool_choice") {
        let unsupported_choice = match choice {
            Value::String(s) => s == "required",
            Value::Object(_) => true,
            _ => false,
        };
        if unsupported_choice {
            obj.insert("tool_choice".to_string(), json!("auto"));
        }
    }
    if let Some(effort) = obj.get("reasoning_effort").and_then(|v| v.as_str()) {
        let kind = if effort == "none" {
            "disabled"
        } else {
            "enabled"
        };
        obj.insert("thinking".to_string(), json!({ "type": kind }));
    }

    serde_json::to_vec(&root).map(Bytes::from).unwrap_or(body)
}

/// 仅保留允许透传给 Codex 上游的会话/客户端元数据头。
fn select_codex_headers(headers: &HeaderMap) -> HeaderMap {
    let mut selected = HeaderMap::new();
    for name in CODEX_FORWARDED_HEADERS {
        if let Some(value) = headers.get(name) {
            selected.insert(name, value.clone());
        }
    }
    selected
}

/// 规整 Responses 请求体以兼容 ChatGPT Codex OAuth 后端。
fn normalize_codex_body(body: Bytes) -> Bytes {
    let Ok(mut root) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(obj) = root.as_object_mut() else {
        return body;
    };
    if let Some(Value::String(text)) = obj.get("input").cloned() {
        obj.insert(
            "input".to_string(),
            json!([{
                "role": "user",
                "content": [{"type": "input_text", "text": text}]
            }]),
        );
    }
    // 剔除历史 reasoning 项：其 encrypted_content 与产出它的渠道/账号强绑定。
    // 同一会话跨渠道切换（如先走 openai 兼容渠道、后切 codex）或多账号 failover 时，
    // 把别处的 encrypted_content 透传给 codex 上游会解密校验失败（encrypted content could not be verified）
    if let Some(Value::Array(items)) = obj.get_mut("input") {
        items.retain(|item| item.get("type").and_then(|t| t.as_str()) != Some("reasoning"));
        for item in items {
            let Some(item_obj) = item.as_object_mut() else {
                continue;
            };
            if item_obj.get("role").and_then(|v| v.as_str()) == Some("system") {
                item_obj.insert("role".to_string(), json!("developer"));
            }
            if let Some(Value::Array(parts)) = item_obj.get_mut("content") {
                for part in parts {
                    if let Some(part_obj) = part.as_object_mut() {
                        part_obj.remove("prompt_cache_breakpoint");
                    }
                }
            }
        }
    }
    for field in CODEX_UNSUPPORTED_FIELDS {
        obj.remove(field);
    }
    if obj.get("instructions").is_none_or(Value::is_null) {
        obj.insert(
            "instructions".to_string(),
            json!("You are a helpful assistant."),
        );
    }
    obj.insert("stream".to_string(), json!(true));
    obj.insert("store".to_string(), json!(false));
    obj.insert(
        "include".to_string(),
        json!(["reasoning.encrypted_content"]),
    );
    let has_tools = obj
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    if has_tools {
        obj.insert("parallel_tool_calls".to_string(), json!(true));
    } else {
        obj.remove("parallel_tool_calls");
    }
    serde_json::to_vec(&root).map(Bytes::from).unwrap_or(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::http::HeaderValue;

    #[test]
    fn normalize_codex_body_applies_oauth_compatibility() {
        let body = Bytes::from_static(
            br#"{
                "model":"gpt-5.6-sol",
                "stream":false,
                "store":true,
                "instructions":null,
                "max_output_tokens":4096,
                "max_completion_tokens":4096,
                "temperature":0.2,
                "top_p":0.9,
                "truncation":"auto",
                "prompt_cache_options":{"mode":"implicit"},
                "prompt_cache_retention":"24h",
                "context_management":[{"type":"compaction"}],
                "user":"owner",
                "safety_identifier":"safe",
                "previous_response_id":"resp_old",
                "generate":true,
                "stream_options":{"include_usage":true},
                "parallel_tool_calls":false,
                "tools":[{"type":"function","name":"search","parameters":{"type":"object"}}],
                "input":[
                    {"type":"reasoning","encrypted_content":"bound"},
                    {"type":"message","role":"system","content":[
                        {"type":"input_text","text":"rules","prompt_cache_breakpoint":{"type":"ephemeral"}}
                    ]},
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}
                ]
            }"#,
        );

        let normalized = normalize_codex_body(body);
        let value: Value = serde_json::from_slice(&normalized).unwrap();

        assert_eq!(value["stream"], json!(true));
        assert_eq!(value["store"], json!(false));
        assert_eq!(value["parallel_tool_calls"], json!(true));
        assert_eq!(value["include"], json!(["reasoning.encrypted_content"]));
        assert_eq!(value["instructions"], json!("You are a helpful assistant."));
        assert_eq!(value["input"].as_array().unwrap().len(), 2);
        assert_eq!(value["input"][0]["role"], json!("developer"));
        assert!(
            value["input"][0]["content"][0]
                .get("prompt_cache_breakpoint")
                .is_none()
        );
        for field in CODEX_UNSUPPORTED_FIELDS {
            assert!(value.get(field).is_none(), "{field} should be removed");
        }
    }

    #[test]
    fn normalize_codex_body_arrayizes_input_and_drops_parallel_without_tools() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-5.6-sol","input":"hello","parallel_tool_calls":true}"#,
        );

        let normalized = normalize_codex_body(body);
        let value: Value = serde_json::from_slice(&normalized).unwrap();

        assert_eq!(value["input"][0]["role"], json!("user"));
        assert_eq!(value["input"][0]["content"][0]["text"], json!("hello"));
        assert!(value.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn select_codex_headers_uses_allowlist() {
        let mut headers = HeaderMap::new();
        headers.insert("session-id", HeaderValue::from_static("session-1"));
        headers.insert("x-codex-turn-metadata", HeaderValue::from_static("turn-1"));
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer gateway-key"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("gateway-key"));
        headers.insert("x-unrelated", HeaderValue::from_static("drop-me"));

        let selected = select_codex_headers(&headers);

        assert_eq!(selected.get("session-id").unwrap(), "session-1");
        assert_eq!(selected.get("x-codex-turn-metadata").unwrap(), "turn-1");
        assert!(selected.get("authorization").is_none());
        assert!(selected.get("x-api-key").is_none());
        assert!(selected.get("x-unrelated").is_none());
    }
}
