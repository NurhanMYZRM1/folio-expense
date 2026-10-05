use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub theme: String,
    pub default_currency: String,
    pub online_enabled: bool,
    pub provider: String,
    pub api_base_url: String,
    pub ai_model: String,
    pub offline_ocr_enabled: bool,
    pub export_directory: Option<String>,
    pub include_receipts: bool,
    pub company: String,
    pub employee: String,
    /// Convert foreign-currency receipts to `default_currency` using published
    /// exchange rates (only currency codes and a date are sent online).
    #[serde(default = "enabled")]
    pub currency_conversion_enabled: bool,
}
fn enabled() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            default_currency: "MYR".into(),
            online_enabled: false,
            provider: "openai_compatible".into(),
            api_base_url: "https://api.openai.com/v1".into(),
            ai_model: "gpt-4o-mini".into(),
            offline_ocr_enabled: true,
            export_directory: None,
            include_receipts: true,
            company: String::new(),
            employee: String::new(),
            currency_conversion_enabled: true,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub data_directory: String,
    pub version: String,
    pub credential_configured: bool,
}
