//! Sources and exact supported boundaries are recorded in docs/tax-sources.md.
use super::AppService;
use crate::{
    domain::{expense::MAX_MONEY, tax::*},
    error::{AppError, Result},
};
use std::collections::BTreeSet;
const RATES: &str = "https://www.hasil.gov.my/en/individu/kadar-cukai/";
const RELIEFS: &str = "https://www.hasil.gov.my/en/individu/pelepasan-cukai/";
const NOTES: &str = "https://ef.hasil.gov.my/eBE2026/Pdf/Nota_BE_e.pdf";
const SST: &str = "https://mysst.customs.gov.my/background/";
const EINVOICE: &str = "https://sdk.myinvois.hasil.gov.my/documents/invoice-v1-1/";
fn amount(value: i64) -> Result<()> {
    if !(0..=MAX_MONEY).contains(&value) {
        return Err(AppError::invalid(
            "Enter a nonnegative amount within the supported range.",
        ));
    }
    Ok(())
}
fn checked(value: i128) -> Result<i64> {
    i64::try_from(value)
        .ok()
        .filter(|v| (0..=MAX_MONEY).contains(v))
        .ok_or_else(|| AppError::invalid("The calculated total exceeds the supported amount."))
}
fn rules(year: i32) -> Result<TaxRules> {
    if year != 2025 {
        return Err(AppError::new("UnsupportedTaxYear","Only YA 2025 resident individual estimates are verified. Other years, including YA 2026, are not calculated."));
    }
    let bands = [
        (Some(5000), 0),
        (Some(20000), 100),
        (Some(35000), 300),
        (Some(50000), 600),
        (Some(70000), 1100),
        (Some(100000), 1900),
        (Some(400000), 2500),
        (Some(600000), 2600),
        (Some(2000000), 2800),
        (None, 3000),
    ]
    .into_iter()
    .map(|(upper, rate)| TaxBand {
        upper_minor: upper.map(|n| n * 100),
        rate_basis_points: rate,
    })
    .collect();
    let reliefs=[
        ("individual","Individual and dependent relatives",9000,"Automatic individual relief; do not enter another claim for this code."),
        ("spouse","Spouse with no income",4000,"Only the married spouse-with-no-income assessment option is supported; joint assessment and alimony are outside this calculator."),
        ("disabled_individual","Disabled individual",7000,"Qualifying disabled individual; confirm certification and eligibility against the source."),
        ("lifestyle","Lifestyle",2500,"Qualifying books, personal devices, own-name internet or permitted personal development courses; exclude business purchases."),
        ("epf","EPF or approved scheme",4000,"Qualifying mandatory or voluntary contributions, excluding PRS; do not allocate the same contribution to another relief."),
        ("life_insurance","Life insurance or family takaful",3000,"Qualifying premiums, family takaful or additional voluntary EPF; no double-counting with EPF."),
        ("prs","Private retirement scheme / deferred annuity",3000,"Qualifying PRS or deferred annuity contributions."),
        ("education_medical_insurance","Education and medical insurance",4000,"Qualifying premiums for self, spouse or child."),
        ("socso","SOCSO / EIS",350,"Qualifying SOCSO and EIS contributions.")
    ].into_iter().map(|(code,label,cap,eligibility)|TaxReliefRule{code:code.into(),label:label.into(),cap_minor:cap*100,eligibility:eligibility.into(),source_url:RELIEFS.into()}).collect();
    Ok(TaxRules{assessment_year:year,currency:"MYR".into(),bands,reliefs,sources:vec![RATES.into(),RELIEFS.into(),NOTES.into()],supported_scope:"Resident individual, YA 2025, single or separate assessment with ordinary progressive-rate income. Enter total income after permitted deductions. Excludes joint assessment, business-income computation, foreign tax credits, dividend-specific tax, special rates and reliefs not listed. Eligibility is user-confirmed. PCB input is tax already withheld; payroll PCB calculation is not provided.".into()})
}
impl AppService {
    pub fn get_tax_rules(&self, assessment_year: i32) -> Result<TaxRules> {
        rules(assessment_year)
    }
    pub fn estimate_income_tax(&self, input: IncomeTaxInput) -> Result<IncomeTaxEstimate> {
        let rules = rules(input.assessment_year)?;
        if !input.resident {
            return Err(AppError::new(
                "UnsupportedTaxInputs",
                "This estimate supports Malaysian resident individuals only.",
            ));
        }
        amount(input.total_income_minor)?;
        amount(input.zakat_minor)?;
        amount(input.pcb_paid_minor)?;
        if input.reliefs.len() > rules.reliefs.len() {
            return Err(AppError::invalid("Check the relief claims."));
        }
        let mut relief: i128 = 900_000;
        let mut seen = BTreeSet::new();
        let spouse = input.marital_status == MaritalStatus::MarriedSpouseNoIncome;
        // Spouse allowance is explicit and user-confirmed in the assessment choice.
        if spouse {
            relief += 400_000;
        }
        for claim in input.reliefs {
            let rule = rules
                .reliefs
                .iter()
                .find(|r| r.code == claim.code)
                .ok_or_else(|| {
                    AppError::new(
                        "UnsupportedTaxInputs",
                        format!("Relief {} is not supported by this estimate.", claim.code),
                    )
                })?;
            amount(claim.amount_minor)?;
            if claim.code == "individual" || claim.code == "spouse" {
                return Err(AppError::invalid("Individual and eligible spouse relief are included automatically; remove duplicate relief entries."));
            }
            if !seen.insert(claim.code) {
                return Err(AppError::invalid("A relief can only be entered once."));
            }
            if claim.amount_minor > 0 && !claim.eligibility_confirmed {
                return Err(AppError::invalid(
                    "Confirm each claimed relief's eligibility against its source.",
                ));
            }
            relief += i128::from(claim.amount_minor.min(rule.cap_minor));
        }
        let relief = checked(relief)?;
        // Folio estimates with exact integer sen and half-up tax rounding.
        // This is an estimation convention, not a filing-rounding rule.
        let chargeable = (input.total_income_minor - relief).max(0);
        let mut lower = 0;
        let mut tax_numerator: i128 = 0;
        for band in &rules.bands {
            let upper = band.upper_minor.unwrap_or(chargeable);
            let taxable = (chargeable.min(upper) - lower).max(0);
            tax_numerator += i128::from(taxable) * i128::from(band.rate_basis_points);
            lower = upper;
            if chargeable <= upper {
                break;
            }
        }
        let tax = checked((tax_numerator + 5000) / 10000)?;
        let personal_rebate = if chargeable <= 3_500_000 {
            40_000 + if spouse { 40_000 } else { 0 }
        } else {
            0
        };
        let rebate = checked(
            (i128::from(personal_rebate) + i128::from(input.zakat_minor)).min(i128::from(tax)),
        )?;
        let payable = tax - rebate;
        Ok(IncomeTaxEstimate{assessment_year:input.assessment_year,currency:"MYR".into(),relief_minor:relief,chargeable_income_minor:chargeable,tax_before_rebates_minor:tax,rebate_minor:rebate,tax_payable_minor:payable,balance_after_pcb_minor:payable-input.pcb_paid_minor,warnings:vec!["Estimate only. Confirm total income and relief eligibility against LHDN's assessment-year guidance. Nothing is filed or submitted.".into(),rules.supported_scope],sources:rules.sources})
    }
    pub fn calculate_sst(&self, input: SstInput) -> Result<SstCalculation> {
        amount(input.amount_minor)?;
        // An arithmetic helper for a rate confirmed by the user. Registration,
        // exemptions, goods classification and transition rules are not inferred.
        if ![0, 500, 600, 800, 1000].contains(&input.rate_basis_points) {
            return Err(AppError::new("UnsupportedTaxInputs","Select a confirmed rate: 0%, 5%, 6%, 8% or 10%. This helper does not determine whether SST applies."));
        }
        let rate = i128::from(input.rate_basis_points);
        let (net, tax, total) = if input.tax_inclusive {
            let denominator = 10000 + rate;
            let tax =
                checked((i128::from(input.amount_minor) * rate + denominator / 2) / denominator)?;
            (input.amount_minor - tax, tax, input.amount_minor)
        } else {
            let tax = checked((i128::from(input.amount_minor) * rate + 5000) / 10000)?;
            (
                input.amount_minor,
                tax,
                checked(i128::from(input.amount_minor) + i128::from(tax))?,
            )
        };
        Ok(SstCalculation{net_minor:net,tax_minor:tax,total_minor:total,rate_basis_points:input.rate_basis_points,notice:"Arithmetic estimate using your selected rate. Confirm registration, taxable service/goods, exemptions and effective-date rules with Customs; no automatic classification or return submission.".into(),source_url:SST.into()})
    }
    pub fn validate_einvoice(
        &self,
        workspace_id: &str,
        invoice_id: &str,
    ) -> Result<EinvoiceValidation> {
        let invoice = self.get_invoice(workspace_id, invoice_id)?;
        let mut errors = Vec::new();
        let supplier = invoice
            .supplier
            .or(self.get_business_profile(workspace_id)?);
        match supplier {
            None => errors.push("Add the supplier's business profile.".into()),
            Some(p) => {
                if p.tin.trim().is_empty() {
                    errors.push("Supplier TIN is missing.".into());
                }
                if p.registration_number.trim().is_empty() {
                    errors.push("Supplier registration / identification number is missing.".into());
                }
                if p.address.trim().is_empty() {
                    errors.push("Supplier address is missing.".into());
                }
                if p.phone.trim().is_empty() {
                    errors.push("Supplier contact number is missing.".into());
                }
            }
        }
        if invoice.customer.tin.trim().is_empty() {
            errors.push("Buyer TIN is missing; special consolidated or general-public cases require separate official guidance.".into());
        }
        if invoice.customer.registration_number.trim().is_empty() {
            errors.push("Buyer registration / identification number is missing.".into());
        }
        if invoice.customer.address.trim().is_empty() {
            errors.push("Buyer address is missing.".into());
        }
        if invoice.number.is_none() {
            errors.push("Issue the invoice to assign its document number.".into());
        }
        for (index, line) in invoice.lines.iter().enumerate() {
            if line.input.classification_code.is_empty() {
                errors.push(format!(
                    "Line {} needs a classification code checked against the MyInvois code list.",
                    index + 1
                ));
            }
        }
        Ok(EinvoiceValidation{invoice_id:invoice_id.into(),preparation_status:"local_preflight_only".into(),errors,warnings:vec!["Local checklist only. This is not a MyInvois validated or submitted document. Full UBL generation, address/country/state codes, tax-type mappings, TIN lookup, digital signature and government API validation remain unavailable.".into()],source_url:EINVOICE.into()})
    }
}
