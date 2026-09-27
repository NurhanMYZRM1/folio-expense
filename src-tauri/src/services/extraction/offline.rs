use super::{ExtractionInput, ReceiptExtractor};
use crate::{
    domain::{
        expense::{currency_exponent, parse_money},
        extraction::Extraction,
    },
    error::Result,
};
use regex::Regex;
use std::collections::BTreeMap;
pub struct LocalOcrExtractor;
impl ReceiptExtractor for LocalOcrExtractor {
    fn extract(&self, input: &ExtractionInput<'_>) -> Result<Extraction> {
        let lines: Vec<_> = input
            .raw_text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let uppercase = input.raw_text.to_uppercase();
        let mut currency = None;
        for c in [
            "MYR", "USD", "SGD", "EUR", "GBP", "AUD", "CAD", "CHF", "CNY", "HKD", "INR", "THB",
            "IDR", "JPY", "KRW", "BHD", "KWD", "OMR",
        ] {
            if uppercase
                .split(|c: char| !c.is_ascii_alphabetic())
                .any(|word| word == c)
            {
                currency = Some(c.to_string());
                break;
            }
        }
        if currency.is_none()
            && (uppercase.contains("RM ")
                || uppercase.contains("RM\t")
                || uppercase.contains("RM\n"))
        {
            currency = Some("MYR".into());
        }
        let detected_currency = currency.is_some();
        let currency = currency.or_else(|| Some(input.default_currency.into()));
        let exponent = currency_exponent(currency.as_deref().unwrap_or("MYR"))?;
        let amount_re = Regex::new(r"[0-9][0-9,]*(?:\.[0-9]{1,3})?")
            .map_err(|_| crate::error::AppError::invalid("OCR pattern failed."))?;
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
        let iso = Regex::new(r"\b\d{4}[-/]\d{2}[-/]\d{2}\b")
            .map_err(|_| crate::error::AppError::invalid("OCR pattern failed."))?;
        let dmy = Regex::new(r"\b\d{2}[-/]\d{2}[-/]\d{4}\b")
            .map_err(|_| crate::error::AppError::invalid("OCR pattern failed."))?;
        let date = iso
            .find(input.raw_text)
            .and_then(|m| {
                chrono::NaiveDate::parse_from_str(&m.as_str().replace('/', "-"), "%Y-%m-%d").ok()
            })
            .or_else(|| {
                dmy.find(input.raw_text).and_then(|m| {
                    chrono::NaiveDate::parse_from_str(&m.as_str().replace('/', "-"), "%d-%m-%Y")
                        .ok()
                })
            })
            .map(|d| d.to_string());
        let merchant = lines
            .iter()
            .take(6)
            .find(|line| {
                line.chars().filter(|c| c.is_alphabetic()).count() >= 3
                    && !line.to_uppercase().contains("RECEIPT")
                    && !line.to_uppercase().contains("INVOICE")
            })
            .map(|s| s.chars().take(300).collect());
        let mut confidence = BTreeMap::new();
        for (field, value) in [
            ("merchantName", if merchant.is_some() { 0.65 } else { 0.0 }),
            ("date", if date.is_some() { 0.75 } else { 0.0 }),
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
