use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    OnlineAi,
    LocalOcr,
    Manual,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldMeta {
    pub confidence: f64,
    pub source: FieldSource,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Extraction {
    pub merchant_name: Option<String>,
    pub date: Option<String>,
    #[ts(type = "number | null")]
    pub total_amount_minor: Option<i64>,
    #[ts(type = "number | null")]
    pub tax_amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub suggested_category: Option<String>,
    pub confidence: BTreeMap<String, f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub job_type: String,
    pub entity_id: String,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobLease {
    pub job: Job,
    pub token: String,
    pub receipt_id: Option<String>,
}
