use super::{sanitize, ExtractionInput, ReceiptExtractor};
use crate::{
    domain::{
        expense::{currency_exponent, parse_money},
        extraction::Extraction,
    },
    error::{AppError, Result},
};
use regex::Regex;
use std::collections::BTreeMap;

fn re(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).map_err(|_| AppError::invalid("OCR pattern failed."))
}

/// Currency tokens tried in priority order (most specific first) against a
/// piece of text; the first match wins. `¥` is special-cased below to prefer
/// `CNY` when a China cue (`RMB`/`CNY`/`CN¥`) appears anywhere in the text.
const CURRENCY_SPECS: &[(&str, &str)] = &[
    (r"(?i)\bUS\$", "US$"),
    (r"(?i)\bS\$", "S$"),
    (r"(?i)\bRM\s*\d", "RM"),
    (r"(?i)\bRM\b", "RM"),
    (r"(?i)\bRp\s*\d", "Rp"),
    (r"(?i)\bRp\b", "Rp"),
    (r"¥", "¥"),
    (r"€", "€"),
    (r"£", "£"),
    (r"₹", "₹"),
    (r"฿", "฿"),
    (r"₩", "₩"),
    (r"\bMYR\b", "MYR"),
    (r"\bUSD\b", "USD"),
    (r"\bSGD\b", "SGD"),
    (r"\bEUR\b", "EUR"),
    (r"\bGBP\b", "GBP"),
    (r"\bAUD\b", "AUD"),
    (r"\bCAD\b", "CAD"),
    (r"\bCHF\b", "CHF"),
    (r"\bCNY\b", "CNY"),
    (r"\bHKD\b", "HKD"),
    (r"\bINR\b", "INR"),
    (r"\bTHB\b", "THB"),
    (r"\bIDR\b", "IDR"),
    (r"\bJPY\b", "JPY"),
    (r"\bKRW\b", "KRW"),
    (r"\bBHD\b", "BHD"),
    (r"\bKWD\b", "KWD"),
    (r"\bOMR\b", "OMR"),
    (r"\$", "$"),
];

fn currency_patterns() -> Result<Vec<(Regex, &'static str)>> {
    CURRENCY_SPECS
        .iter()
        .map(|(pattern, token)| re(pattern).map(|r| (r, *token)))
        .collect()
}

fn detect_currency_raw(
    patterns: &[(Regex, &'static str)],
    text: &str,
    cny_cue: bool,
) -> Option<String> {
    for (pattern, token) in patterns {
        if pattern.is_match(text) {
            return Some(if *token == "¥" && cny_cue {
                "CNY".to_string()
            } else {
                (*token).to_string()
            });
        }
    }
    None
}

/// Same shape as `TOTAL`/`AMOUNT DUE`/`BALANCE DUE` matching used below; kept
/// separate so currency detection can prefer the total line before amounts
/// are parsed (amount parsing needs the currency's exponent).
fn find_total_line<'a>(lines: &[&'a str]) -> Option<&'a str> {
    let mut found = None;
    for line in lines {
        let upper = line.to_uppercase();
        if (upper.contains("TOTAL")
            || upper.contains("AMOUNT DUE")
            || upper.contains("BALANCE DUE"))
            && !upper.contains("SUB")
            && !upper.contains("TAX")
            && !upper.contains("SAVING")
        {
            found = Some(*line);
        }
    }
    found
}

// Candidate date substrings covering the formats `sanitize::normalize_date`
// accepts: ISO, numeric D/M/Y (or M/D/Y), `D Month Y` and `Month D, Y`.
const DATE_CANDIDATE_PATTERN: &str = r"(?i)\b\d{4}[-/.]\d{1,2}[-/.]\d{1,2}\b|\b\d{1,2}[-/.]\d{1,2}[-/.]\d{2,4}\b|\b\d{1,2}[-\s][A-Za-z]{3,9}\.?,?[-\s]\d{2,4}\b|\b[A-Za-z]{3,9}\.?\s+\d{1,2},?\s+\d{2,4}\b";

const PHONE_PATTERN: &str = r"(?i)\bTEL\b|\bPHONE\b|\+60|\d{7,}";
const REG_NUMBER_PATTERN: &str =
    r"(?i)\bSST\b|\bGST\b|\bREG\b|CO\.?\s*NO\.?|\bROC\b|\(\d+-[A-Z0-9]+\)";
const ADDRESS_PATTERN: &str = r"(?i)\b\d{5}\b|\bJALAN\b|\bJLN\b|\bSTREET\b|\bROAD\b|\bLOT\b|\bNO\.\s*\d+|\bTAMAN\b|KUALA LUMPUR|SELANGOR";
const BOILERPLATE_PATTERN: &str = r"(?i)\bRECEIPT\b|\bINVOICE\b|TAX INVOICE|\bWELCOME\b|THANK YOU|CASH SALE|\bOFFICIAL\b|\bCOPY\b";
const TIME_PATTERN: &str = r"\b\d{1,2}:\d{2}\b";
const SUFFIX_PATTERN: &str = r"(?i)SDN\.?\s*BHD\.?|\bBHD\b|\bENTERPRISE\b|\bTRADING\b|\bRESTAURANT\b|\bCAFE\b|PTE\.?\s*LTD\.?|\bLLC\b|\bINC\b";

pub struct LocalOcrExtractor;

impl LocalOcrExtractor {
    fn extract_with_today(
        &self,
        input: &ExtractionInput<'_>,
        today: chrono::NaiveDate,
    ) -> Result<Extraction> {
        let lines: Vec<&str> = input
            .raw_text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        // --- Currency: prefer whatever's on the TOTAL line, else the whole text.
        let upper_text = input.raw_text.to_uppercase();
        let cny_cue =
            upper_text.contains("RMB") || upper_text.contains("CNY") || upper_text.contains("CN¥");
        let patterns = currency_patterns()?;
        let total_line = find_total_line(&lines);
        let raw_currency = total_line
            .and_then(|line| detect_currency_raw(&patterns, line, cny_cue))
            .or_else(|| detect_currency_raw(&patterns, input.raw_text, cny_cue));
        let normalized_currency = raw_currency
            .as_deref()
            .and_then(|c| sanitize::normalize_currency(c, input.default_currency));
        let detected_currency = normalized_currency.is_some();
        let currency = normalized_currency.or_else(|| Some(input.default_currency.to_string()));
        let exponent = currency_exponent(currency.as_deref().unwrap_or("MYR"))?;

        // --- Total / tax amounts.
        let amount_re = re(r"[0-9][0-9,]*(?:\.[0-9]{1,3})?")?;
        let find_amount = |line: &str| {
            amount_re
                .find_iter(line)
                .last()
                .and_then(|m| parse_money(m.as_str(), exponent).ok())
        };
        let mut total = None;
        let mut tax = None;
        for line in &lines {
            let upper = line.to_uppercase();
            if (upper.contains("TOTAL")
                || upper.contains("AMOUNT DUE")
                || upper.contains("BALANCE DUE"))
                && !upper.contains("SUB")
                && !upper.contains("TAX")
                && !upper.contains("SAVING")
            {
                if let Some(a) = find_amount(line) {
                    total = Some(a);
                }
            }
            if upper.contains("TAX") || upper.contains("SST") || upper.contains("GST") {
                if !upper.contains('%') {
                    tax = find_amount(line);
                } else if line.rsplit('%').next().is_some_and(|s| s.trim().len() > 2) {
                    tax = line.rsplit('%').next().and_then(find_amount);
                }
            }
        }
        if tax.zip(total).is_some_and(|(a, b)| a > b) {
            tax = None;
        }

        // --- Date: try each candidate substring in order of appearance.
        let date_re = re(DATE_CANDIDATE_PATTERN)?;
        let mut date = None;
        let mut date_ambiguous = false;
        for m in date_re.find_iter(input.raw_text) {
            if let Some((d, ambiguous)) =
                sanitize::normalize_date(m.as_str(), currency.as_deref(), today)
            {
                date = Some(d);
                date_ambiguous = ambiguous;
                break;
            }
        }

        // --- Merchant: first 8 non-empty lines, filtered, suffix-preferred.
        let time_re = re(TIME_PATTERN)?;
        let phone_re = re(PHONE_PATTERN)?;
        let reg_re = re(REG_NUMBER_PATTERN)?;
        let address_re = re(ADDRESS_PATTERN)?;
        let boilerplate_re = re(BOILERPLATE_PATTERN)?;
        let suffix_re = re(SUFFIX_PATTERN)?;
        let candidates: Vec<&str> = lines
            .iter()
            .take(8)
            .copied()
            .filter(|line| {
                let alpha = line.chars().filter(|c| c.is_alphabetic()).count();
                alpha >= 3
                    && !date_re.is_match(line)
                    && !time_re.is_match(line)
                    && !phone_re.is_match(line)
                    && !reg_re.is_match(line)
                    && !address_re.is_match(line)
                    && !boilerplate_re.is_match(line)
            })
            .collect();
        let suffix_line = candidates.iter().find(|line| suffix_re.is_match(line));
        let (merchant, merchant_has_suffix) = match suffix_line.or_else(|| candidates.first()) {
            Some(line) => (
                Some(line.chars().take(300).collect::<String>()),
                suffix_line.is_some(),
            ),
            None => (None, false),
        };

        let mut confidence = BTreeMap::new();
        for (field, value) in [
            (
                "merchantName",
                if merchant.is_some() {
                    if merchant_has_suffix {
                        0.7
                    } else {
                        0.55
                    }
                } else {
                    0.0
                },
            ),
            (
                "date",
                if date.is_some() {
                    if date_ambiguous {
                        0.5
                    } else {
                        0.75
                    }
                } else {
                    0.0
                },
            ),
            ("total", if total.is_some() { 0.80 } else { 0.0 }),
            ("tax", if tax.is_some() { 0.65 } else { 0.0 }),
            ("currency", if detected_currency { 0.9 } else { 0.25 }),
            ("category", 0.2),
        ] {
            confidence.insert(field.into(), value);
        }
        Ok(Extraction {
            merchant_name: merchant,
            date,
            total_amount_minor: total,
            tax_amount_minor: tax,
            currency,
            suggested_category: Some("Other".into()),
            confidence,
        })
    }
}

impl ReceiptExtractor for LocalOcrExtractor {
    fn extract(&self, input: &ExtractionInput<'_>) -> Result<Extraction> {
        self.extract_with_today(input, chrono::Local::now().date_naive())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 9, 27).unwrap()
    }

    fn run(raw_text: &str, default_currency: &str) -> Extraction {
        let input = ExtractionInput {
            raw_text,
            images: &[],
            default_currency,
        };
        LocalOcrExtractor
            .extract_with_today(&input, today())
            .unwrap()
    }

    #[test]
    fn malaysian_cafe_receipt_with_rm_amounts() {
        let text = "\
KOPI KENANGAN CAFE SDN BHD
TAX INVOICE
No. 12, Jalan Ampang
Taman Desa, 50450 Kuala Lumpur
Tel: +60312345678
SST Reg: (123456-X)
27/09/2026 08:15
1 x Kopi O RM4.50
1 x Nasi Lemak RM8.00
SUBTOTAL RM12.50
SST 6% RM0.75
TOTAL RM13.25
THANK YOU";
        let out = run(text, "MYR");
        assert_eq!(
            out.merchant_name.as_deref(),
            Some("KOPI KENANGAN CAFE SDN BHD")
        );
        assert_eq!(out.date.as_deref(), Some("2026-09-27"));
        assert_eq!(out.currency.as_deref(), Some("MYR"));
        assert_eq!(out.total_amount_minor, Some(1325));
        assert_eq!(out.confidence.get("merchantName"), Some(&0.7));
    }

    #[test]
    fn welcome_to_line_falls_back_to_second_surviving_line() {
        let text = "\
WELCOME TO
ABC TRADING SDN BHD
RECEIPT
27 Sep 2026
Item RM10.00
TOTAL RM10.00";
        let out = run(text, "MYR");
        assert_eq!(out.merchant_name.as_deref(), Some("ABC TRADING SDN BHD"));
        assert_eq!(out.date.as_deref(), Some("2026-09-27"));
        assert_eq!(out.currency.as_deref(), Some("MYR"));
        assert_eq!(out.total_amount_minor, Some(1000));
        assert_eq!(out.confidence.get("merchantName"), Some(&0.7));
    }

    #[test]
    fn singapore_receipt_with_sgd_symbol() {
        let text = "\
GOLDEN DRAGON RESTAURANT
Blk 12 Orchard Road
27 Sep 2026
1 x Fried Rice S$8.00
TOTAL S$8.00";
        let out = run(text, "MYR");
        assert_eq!(
            out.merchant_name.as_deref(),
            Some("GOLDEN DRAGON RESTAURANT")
        );
        assert_eq!(out.date.as_deref(), Some("2026-09-27"));
        assert_eq!(out.currency.as_deref(), Some("SGD"));
        assert_eq!(out.total_amount_minor, Some(800));
    }

    #[test]
    fn us_receipt_with_dollar_and_month_first_date() {
        let text = "\
JOE'S COFFEE INC
123 Main Street
09/03/2026
1 x Latte $4.50
TOTAL $4.50";
        let out = run(text, "USD");
        assert_eq!(out.merchant_name.as_deref(), Some("JOE'S COFFEE INC"));
        assert_eq!(out.date.as_deref(), Some("2026-09-03"));
        assert_eq!(out.currency.as_deref(), Some("USD"));
        assert_eq!(out.total_amount_minor, Some(450));
    }
}
