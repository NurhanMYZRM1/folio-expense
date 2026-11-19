//! Versioned, limited tax estimation and local document preparation. No filing.
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaxReliefRule {
    pub code: String,
    pub label: String,
    #[ts(type = "number")]
    pub cap_minor: i64,
    pub eligibility: String,
    pub source_url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaxBand {
    #[ts(type = "number | null")]
    pub upper_minor: Option<i64>,
    pub rate_basis_points: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaxRules {
    pub assessment_year: i32,
    pub currency: String,
    pub bands: Vec<TaxBand>,
    pub reliefs: Vec<TaxReliefRule>,
    pub sources: Vec<String>,
    pub supported_scope: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MaritalStatus {
    Single,
    MarriedSeparate,
    MarriedSpouseNoIncome,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaxReliefClaim {
    pub code: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    /// Eligibility is checked by the user against the linked source.
    pub eligibility_confirmed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IncomeTaxInput {
    pub assessment_year: i32,
    pub resident: bool,
    pub marital_status: MaritalStatus,
    /// Total income after permitted deductions, before personal reliefs.
    /// This is not gross revenue or gross salary.
    #[ts(type = "number")]
    pub total_income_minor: i64,
    pub reliefs: Vec<TaxReliefClaim>,
    #[ts(type = "number")]
    pub zakat_minor: i64,
    /// PCB/MTD already withheld for this assessment year, not payroll forecast.
    #[ts(type = "number")]
    pub pcb_paid_minor: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IncomeTaxEstimate {
    pub assessment_year: i32,
    pub currency: String,
    #[ts(type = "number")]
    pub relief_minor: i64,
    #[ts(type = "number")]
    pub chargeable_income_minor: i64,
    #[ts(type = "number")]
    pub tax_before_rebates_minor: i64,
    #[ts(type = "number")]
    pub rebate_minor: i64,
    #[ts(type = "number")]
    pub tax_payable_minor: i64,
    /// Negative means estimated overpayment; no refund is requested.
    #[ts(type = "number")]
    pub balance_after_pcb_minor: i64,
    pub warnings: Vec<String>,
    pub sources: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SstInput {
    #[ts(type = "number")]
    pub amount_minor: i64,
    /// Confirmed rate chosen by the user; no automatic tax classification.
    pub rate_basis_points: i32,
    pub tax_inclusive: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SstCalculation {
    #[ts(type = "number")]
    pub net_minor: i64,
    #[ts(type = "number")]
    pub tax_minor: i64,
    #[ts(type = "number")]
    pub total_minor: i64,
    pub rate_basis_points: i32,
    pub notice: String,
    pub source_url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EinvoiceValidation {
    pub invoice_id: String,
    pub preparation_status: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub source_url: String,
}
