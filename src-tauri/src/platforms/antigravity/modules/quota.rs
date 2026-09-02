use crate::antigravity::models::quota::{
    LoadProjectResponse, ModelQuota, QuotaBucket, QuotaData, QuotaGroup, QuotaResponse,
    QuotaSummaryResponse,
};
use crate::http_client::create_proxy_client;
use serde_json::{json, Value};
use std::sync::OnceLock;

const QUOTA_API_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:fetchAvailableModels",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
    "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
];

const QUOTA_SUMMARY_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
];

const LOAD_PROJECT_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:loadCodeAssist",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
    "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
];

fn quota_user_agent() -> &'static str {
    static UA: OnceLock<String> = OnceLock::new();
    UA.get_or_init(|| {
        let version = crate::antigravity::modules::version::get_antigravity_version()
            .map(|v| v.short_version)
            .unwrap_or_else(|_| "1.16.5".to_string());
        format!("vscode/1.X.X (Antigravity/{version})")
    })
    .as_str()
}

fn is_relevant_model(name: &str) -> bool {
    name.starts_with("gemini")
        || name.starts_with("claude")
        || name.starts_with("gpt")
        || name.starts_with("image")
        || name.starts_with("imagen")
}

fn extract_subscription_tier(data: &LoadProjectResponse) -> Option<String> {
    let mut subscription_tier = data
        .paid_tier
        .as_ref()
        .and_then(|t| t.name.clone().or_else(|| t.id.clone()));

    let is_ineligible = data
        .ineligible_tiers
        .as_ref()
        .is_some_and(|tiers| !tiers.is_empty());

    if subscription_tier.is_none() {
        if !is_ineligible {
            subscription_tier = data
                .current_tier
                .as_ref()
                .and_then(|t| t.name.clone().or_else(|| t.id.clone()));
        } else if let Some(allowed) = &data.allowed_tiers {
            if let Some(default_tier) = allowed.iter().find(|t| t.is_default == Some(true)) {
                if let Some(name) = &default_tier.name {
                    subscription_tier = Some(format!("{name} (Restricted)"));
                } else if let Some(id) = &default_tier.id {
                    subscription_tier = Some(format!("{id} (Restricted)"));
                }
            }
        }
    }

    subscription_tier
}

/// 获取 Project ID 与订阅档位。Sandbox → Daily → Prod 依次回退。
async fn fetch_project_id(access_token: &str) -> (Option<String>, Option<String>) {
    let client = match create_proxy_client() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create proxy client: {e}");
            return (None, None);
        }
    };

    let body = json!({
        "metadata": {
            "ideType": "ANTIGRAVITY"
        }
    });

    for url in LOAD_PROJECT_ENDPOINTS {
        match client
            .post(url)
            .bearer_auth(access_token)
            .header("User-Agent", quota_user_agent())
            .json(&body)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                if let Ok(data) = res.json::<LoadProjectResponse>().await {
                    return (data.project_id.clone(), extract_subscription_tier(&data));
                }
            }
            Ok(res) => {
                let status = res.status();
                if status == reqwest::StatusCode::UNAUTHORIZED {
                    return (None, None);
                }
                eprintln!("loadCodeAssist {url} failed: {status}");
            }
            Err(e) => {
                eprintln!("loadCodeAssist {url} network error: {e}");
            }
        }
    }

    (None, None)
}

fn quota_payload(project_id: Option<&str>) -> Value {
    match project_id {
        Some(pid) if !pid.trim().is_empty() => json!({ "project": pid }),
        _ => json!({}),
    }
}

/// 查询账号配额
pub async fn fetch_quota(
    access_token: &str,
    cached_project_id: Option<&str>,
) -> Result<(QuotaData, Option<String>), String> {
    let client = create_proxy_client()?;

    let (fetched_project_id, subscription_tier) = fetch_project_id(access_token).await;
    let project_id = fetched_project_id.or_else(|| {
        cached_project_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
    });

    let payload = quota_payload(project_id.as_deref());
    let mut last_error: Option<String> = None;

    for (ep_idx, ep_url) in QUOTA_API_ENDPOINTS.iter().enumerate() {
        let has_next = ep_idx + 1 < QUOTA_API_ENDPOINTS.len();
        let mut current_payload = payload.clone();
        let mut retry_without_project = false;

        loop {
            match client
                .post(*ep_url)
                .bearer_auth(access_token)
                .header("User-Agent", quota_user_agent())
                .json(&current_payload)
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();

                    if status == reqwest::StatusCode::FORBIDDEN {
                        if current_payload.get("project").is_some() && !retry_without_project {
                            current_payload = json!({});
                            retry_without_project = true;
                            continue;
                        }

                        let mut q = QuotaData::new();
                        q.is_forbidden = true;
                        q.subscription_tier = subscription_tier.clone();
                        return Ok((q, project_id.clone()));
                    }

                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err("HTTP 401: Token expired or invalid".to_string());
                    }

                    if !status.is_success() {
                        let text = response.text().await.unwrap_or_default();
                        if has_next
                            && (status == reqwest::StatusCode::TOO_MANY_REQUESTS
                                || status.is_server_error())
                        {
                            last_error = Some(format!("HTTP {status} - {text}"));
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            break;
                        }
                        return Err(format!("HTTP {status}: {text}"));
                    }

                    let quota_response: QuotaResponse = response
                        .json()
                        .await
                        .map_err(|e| format!("Failed to parse quota response: {e}"))?;

                    let mut quota_data = QuotaData::new();
                    quota_data.subscription_tier = subscription_tier.clone();

                    for (name, info) in quota_response.models {
                        if !is_relevant_model(&name) {
                            continue;
                        }
                        if let Some(quota_info) = info.quota_info {
                            let percentage = quota_info
                                .remaining_fraction
                                .map(|f| (f * 100.0) as i32)
                                .unwrap_or(0);
                            let reset_time = quota_info.reset_time.unwrap_or_default();
                            quota_data.add_model_quota(ModelQuota {
                                name,
                                percentage,
                                reset_time,
                                display_name: info.display_name,
                            });
                        }
                    }

                    if let Some(deprecated) = quota_response.deprecated_model_ids {
                        for (old_id, info) in deprecated {
                            quota_data
                                .model_forwarding_rules
                                .insert(old_id, info.new_model_id);
                        }
                    }

                    quota_data.quota_groups =
                        fetch_quota_summary(access_token, project_id.as_deref()).await;

                    return Ok((quota_data, project_id.clone()));
                }
                Err(e) => {
                    last_error = Some(format!("Request failed: {e}"));
                    if has_next {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                    break;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "Quota fetch failed: all endpoints exhausted".to_string()))
}

async fn fetch_quota_summary(
    access_token: &str,
    project_id: Option<&str>,
) -> Option<Vec<QuotaGroup>> {
    let client = create_proxy_client().ok()?;
    let payload = quota_payload(project_id);

    for ep_url in QUOTA_SUMMARY_ENDPOINTS {
        let res = client
            .post(ep_url)
            .bearer_auth(access_token)
            .header("User-Agent", quota_user_agent())
            .json(&payload)
            .send()
            .await;

        match res {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS
                    {
                        return None;
                    }
                    continue;
                }

                let summary: QuotaSummaryResponse = match response.json().await {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("QuotaSummary JSON parse failed: {e}");
                        return None;
                    }
                };

                let groups: Vec<QuotaGroup> = summary
                    .groups
                    .into_iter()
                    .map(|g| QuotaGroup {
                        display_name: g.display_name.unwrap_or_default(),
                        description: g.description,
                        buckets: g
                            .buckets
                            .into_iter()
                            .map(|b| QuotaBucket {
                                bucket_id: b.bucket_id.unwrap_or_default(),
                                window: b.window.unwrap_or_default(),
                                remaining_fraction: b.remaining_fraction.unwrap_or(0.0),
                                reset_time: b.reset_time.unwrap_or_default(),
                                display_name: b.display_name,
                                description: b.description,
                            })
                            .collect(),
                    })
                    .collect();

                return Some(groups);
            }
            Err(e) => {
                eprintln!("QuotaSummary request failed at {ep_url}: {e}");
                continue;
            }
        }
    }

    None
}
