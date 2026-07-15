use base64::{Engine as _, engine::general_purpose};
use serde_json::Value;

use crate::http_client::create_proxy_client;
use crate::platforms::openai::models::{
    CodexResetConsumeResponse, CodexResetCreditsResponse, QuotaData, WhamUsageResponse,
};

const CHATGPT_WHAM_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CHATGPT_RESET_CREDITS_URL: &str =
    "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
const CHATGPT_RESET_CREDITS_CONSUME_URL: &str =
    "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";

#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    #[error("{0}")]
    Client(String),
    #[error("Request failed: {0}")]
    Transport(String),
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("{0}")]
    Parse(String),
    #[error("no available reset credit")]
    NoAvailableResetCredit,
}

impl QuotaError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Http { status: 401, .. })
    }
}

/// Fetch OpenAI quota from ChatGPT wham usage API.
pub async fn fetch_quota(
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<QuotaData, QuotaError> {
    let client = create_proxy_client().map_err(QuotaError::Client)?;
    let resolved_account_id = resolve_chatgpt_account_id(chatgpt_account_id, access_token);

    let mut request_builder = client
        .get(CHATGPT_WHAM_USAGE_URL)
        .header("authorization", format!("Bearer {}", access_token))
        .header("accept", "application/json");

    if let Some(account_id) = resolved_account_id.as_deref() {
        request_builder = request_builder.header("chatgpt-account-id", account_id);
    }

    let max_attempts = 2;
    let mut last_error: Option<QuotaError> = None;

    for attempt in 1..=max_attempts {
        let request = request_builder
            .try_clone()
            .ok_or_else(|| QuotaError::Client("failed to clone wham usage request".to_string()))?;

        match request.send().await {
            Ok(response) => {
                let status = response.status();

                if status == reqwest::StatusCode::UNAUTHORIZED {
                    return Err(QuotaError::Http {
                        status: 401,
                        message: "Token expired or invalid".to_string(),
                    });
                }

                if status == reqwest::StatusCode::PAYMENT_REQUIRED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    let mut quota = QuotaData::new();
                    quota.is_forbidden = true;
                    return Ok(quota);
                }

                let body = response.text().await.unwrap_or_default();

                if status.is_success() {
                    let wham =
                        serde_json::from_str::<WhamUsageResponse>(&body).map_err(|error| {
                            QuotaError::Parse(format!(
                                "Failed to parse wham usage response: {}; body: {}",
                                error,
                                truncate_for_error(&body)
                            ))
                        })?;
                    let mut quota = QuotaData::from_wham_usage(&wham);
                    attach_reset_credits(&mut quota, access_token, resolved_account_id.as_deref())
                        .await;
                    return Ok(quota);
                }

                let error = http_error(status, &body);
                if attempt == max_attempts || !is_retryable_status(status) {
                    return Err(error);
                }
                last_error = Some(error);
            }
            Err(e) => {
                last_error = Some(QuotaError::Transport(e.to_string()));
            }
        }

        if attempt < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    Err(last_error.unwrap_or_else(|| QuotaError::Client("Quota fetch failed after retries".into())))
}

/// 拉取限流重置券列表（`GET /wham/rate-limit-reset-credits`）
pub async fn fetch_reset_credits(
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<CodexResetCreditsResponse, QuotaError> {
    let client = create_proxy_client().map_err(QuotaError::Client)?;
    let mut builder = client
        .get(CHATGPT_RESET_CREDITS_URL)
        .header("authorization", format!("Bearer {}", access_token))
        .header("accept", "application/json");
    if let Some(account_id) =
        resolve_chatgpt_account_id(chatgpt_account_id, access_token).as_deref()
    {
        builder = builder.header("chatgpt-account-id", account_id);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| QuotaError::Transport(e.to_string()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(http_error(status, &body));
    }
    serde_json::from_str::<CodexResetCreditsResponse>(&body).map_err(|e| {
        QuotaError::Parse(format!(
            "Failed to parse reset-credits response: {}; body: {}",
            e,
            truncate_for_error(&body)
        ))
    })
}

/// 消费一张可用的限流重置券（`POST /wham/rate-limit-reset-credits/consume`）
pub async fn consume_reset_credit(
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<CodexResetConsumeResponse, QuotaError> {
    let list = fetch_reset_credits(access_token, chatgpt_account_id).await?;
    let credit_id = list
        .credits
        .iter()
        .find(|c| c.status.as_deref() == Some("available"))
        .and_then(|c| c.id.clone())
        .ok_or(QuotaError::NoAvailableResetCredit)?;

    let client = create_proxy_client().map_err(QuotaError::Client)?;
    let mut builder = client
        .post(CHATGPT_RESET_CREDITS_CONSUME_URL)
        .header("authorization", format!("Bearer {}", access_token))
        .header("accept", "application/json")
        .header("content-type", "application/json");
    if let Some(account_id) =
        resolve_chatgpt_account_id(chatgpt_account_id, access_token).as_deref()
    {
        builder = builder.header("chatgpt-account-id", account_id);
    }

    let payload = serde_json::json!({
        "credit_id": credit_id,
        "redeem_request_id": uuid::Uuid::new_v4().to_string(),
    });

    let response = builder
        .json(&payload)
        .send()
        .await
        .map_err(|e| QuotaError::Transport(e.to_string()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(http_error(status, &body));
    }
    serde_json::from_str::<CodexResetConsumeResponse>(&body).map_err(|e| {
        QuotaError::Parse(format!(
            "Failed to parse reset-credits consume response: {}; body: {}",
            e,
            truncate_for_error(&body)
        ))
    })
}

/// 尽力而为地把重置券可用/总数写入 quota（失败静默）
async fn attach_reset_credits(
    quota: &mut QuotaData,
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) {
    match fetch_reset_credits(access_token, chatgpt_account_id).await {
        Ok(resp) => {
            let total = resp.credits.len() as i64;
            let available = resp.available_count.unwrap_or_else(|| {
                resp.credits
                    .iter()
                    .filter(|c| c.status.as_deref() == Some("available"))
                    .count() as i64
            });
            quota.reset_credits_total = Some(total);
            quota.reset_credits_available = Some(available);
        }
        Err(_) => {}
    }
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn resolve_chatgpt_account_id(
    explicit_account_id: Option<&str>,
    access_token: &str,
) -> Option<String> {
    let explicit = explicit_account_id
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    if explicit.is_some() {
        return explicit;
    }

    extract_chatgpt_account_id_from_access_token(access_token)
}

fn extract_chatgpt_account_id_from_access_token(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let decoded_payload = decode_base64_url(payload)?;
    let claims: Value = serde_json::from_slice(&decoded_payload).ok()?;

    claims
        .pointer("/https://api.openai.com/auth/chatgpt_account_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn decode_base64_url(value: &str) -> Option<Vec<u8>> {
    if let Ok(decoded) = general_purpose::URL_SAFE_NO_PAD.decode(value.as_bytes()) {
        return Some(decoded);
    }

    let mut padded = value.to_string();
    let remainder = padded.len() % 4;
    if remainder != 0 {
        padded.push_str(&"=".repeat(4 - remainder));
    }

    general_purpose::URL_SAFE.decode(padded.as_bytes()).ok()
}

fn http_error(status: reqwest::StatusCode, body: &str) -> QuotaError {
    let (code, message) = parse_error_code_and_message(body);

    let message = match (code, message) {
        (Some(code), Some(message)) => format!("[{}]: {}", code, message),
        (Some(code), None) => format!("[{}]: {}", code, truncate_for_error(body)),
        (None, Some(message)) => message,
        (None, None) => truncate_for_error(body),
    };
    QuotaError::Http {
        status: status.as_u16(),
        message,
    }
}

fn parse_error_code_and_message(body: &str) -> (Option<String>, Option<String>) {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return (None, None);
    };

    let code = value
        .pointer("/detail/code")
        .and_then(json_value_to_string)
        .or_else(|| value.get("code").and_then(json_value_to_string));

    let message = value
        .pointer("/detail/message")
        .and_then(json_value_to_string)
        .or_else(|| value.get("message").and_then(json_value_to_string))
        .or_else(|| value.get("detail").and_then(json_value_to_string));

    (code, message)
}

fn json_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(v) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        _ => None,
    }
}

fn truncate_for_error(text: &str) -> String {
    const MAX_LEN: usize = 400;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_LEN {
        return trimmed.to_string();
    }

    let truncated: String = trimmed.chars().take(MAX_LEN).collect();
    format!("{}...", truncated)
}
