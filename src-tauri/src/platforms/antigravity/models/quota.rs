use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 单个配额桶（对应 retrieveUserQuotaSummary 里的一个 bucket）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaBucket {
    pub bucket_id: String,
    pub window: String,
    pub remaining_fraction: f64,
    pub reset_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 一个模型组（如 Gemini Models / Claude and GPT models）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaGroup {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub buckets: Vec<QuotaBucket>,
}

/// 配额数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaData {
    pub models: Vec<ModelQuota>,
    pub last_updated: i64,
    pub is_forbidden: bool,
    #[serde(default)]
    pub subscription_tier: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model_forwarding_rules: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_groups: Option<Vec<QuotaGroup>>,
}

/// 单个模型的配额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelQuota {
    pub name: String,
    pub percentage: i32,
    pub reset_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl QuotaData {
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            last_updated: chrono::Utc::now().timestamp(),
            is_forbidden: false,
            subscription_tier: None,
            model_forwarding_rules: HashMap::new(),
            quota_groups: None,
        }
    }

    pub fn add_model(&mut self, name: String, percentage: i32, reset_time: String) {
        self.add_model_quota(ModelQuota {
            name,
            percentage,
            reset_time,
            display_name: None,
        });
    }

    pub fn add_model_quota(&mut self, model: ModelQuota) {
        self.models.push(model);
    }
}

impl Default for QuotaData {
    fn default() -> Self {
        Self::new()
    }
}

/// API 响应结构
#[derive(Debug, Deserialize)]
pub struct QuotaResponse {
    #[serde(default)]
    pub models: HashMap<String, ModelInfo>,
    #[serde(rename = "deprecatedModelIds", default)]
    pub deprecated_model_ids: Option<HashMap<String, DeprecatedModelInfo>>,
}

#[derive(Debug, Deserialize)]
pub struct DeprecatedModelInfo {
    #[serde(rename = "newModelId")]
    pub new_model_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    #[serde(rename = "quotaInfo")]
    pub quota_info: Option<QuotaInfo>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QuotaInfo {
    #[serde(rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    pub reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QuotaSummaryResponse {
    #[serde(default)]
    pub groups: Vec<QuotaSummaryGroup>,
}

#[derive(Debug, Deserialize)]
pub struct QuotaSummaryGroup {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub buckets: Vec<QuotaSummaryBucket>,
}

#[derive(Debug, Deserialize)]
pub struct QuotaSummaryBucket {
    #[serde(rename = "bucketId")]
    pub bucket_id: Option<String>,
    pub window: Option<String>,
    #[serde(rename = "remainingFraction")]
    pub remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    pub reset_time: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoadProjectResponse {
    #[serde(rename = "cloudaicompanionProject", alias = "projectId")]
    pub project_id: Option<String>,
    #[serde(rename = "currentTier")]
    pub current_tier: Option<Tier>,
    #[serde(rename = "paidTier")]
    pub paid_tier: Option<Tier>,
    #[serde(rename = "allowedTiers", default)]
    pub allowed_tiers: Option<Vec<Tier>>,
    #[serde(rename = "ineligibleTiers", default)]
    pub ineligible_tiers: Option<Vec<IneligibleTier>>,
}

#[derive(Debug, Deserialize)]
pub struct IneligibleTier {
    #[allow(dead_code)]
    #[serde(rename = "reasonCode")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Tier {
    #[serde(rename = "isDefault", alias = "is_default")]
    pub is_default: Option<bool>,
    pub id: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "quotaTier")]
    pub quota_tier: Option<String>,
    pub name: Option<String>,
    #[allow(dead_code)]
    pub slug: Option<String>,
}
