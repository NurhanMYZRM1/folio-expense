//! Conservative offline item candidates: a description and an explicit trailing
//! price are evidence, never a committed financial record. Excludes totals and
//! payment/tax/contact/footer lines; every candidate still needs human review.
use crate::domain::{
    expense::{currency_exponent, parse_money},
    receipt_items::*,
};
use regex::Regex;
pub(super) fn parse(
    raw: &str,
    currency: &str,
    confidence: Option<f64>,
) -> (Vec<ReceiptItem>, ReceiptItemAdjustments) {
    let exponent = currency_exponent(currency).unwrap_or(2);
    let trailing = Regex::new(
        r"(?i)^(.*?)\s+(?:RM|MYR|USD|SGD|S\$|US\$|\$)?\s*([0-9][0-9,]*(?:\.[0-9]{1,3})?)\s*$",
    )
    .unwrap();
    let excluded=Regex::new(r"(?i)\b(total|subtotal|sub total|tax|sst|gst|vat|discount|disc|rounding|round off|service|cash|change|balance|paid|payment|card|debit|credit|tender|visa|mastercard|duitnow|fpx|tel|phone|fax|taxi id|invoice|receipt|register|reg|thank|loyalty|points|member)\b").unwrap();
    let prefix_quantity = Regex::new(r"(?i)^([0-9]+(?:\.[0-9]{1,3})?)\s*[x×]\s*(.+)$").unwrap();
    let explicit_price=Regex::new(r"(?i)^(.+?)\s+([0-9]+(?:\.[0-9]{1,3})?)\s*[x×@]\s*(?:RM)?\s*([0-9][0-9,]*(?:\.[0-9]{1,3})?)$").unwrap();
    let mut adjustments = ReceiptItemAdjustments::default();
    let mut items = vec![];
    for line in raw.lines().take(10000) {
        let line = line.trim();
        let Some(captures) = trailing.captures(line) else {
            continue;
        };
        let Some(amount) = parse_money(&captures[2], exponent).ok() else {
            continue;
        };
        let description = captures[1].trim();
        let lower = description.to_lowercase();
        if excluded.is_match(description) {
            if lower.contains("discount") || lower == "disc" {
                adjustments.discount_minor = amount;
            } else if lower.contains("service")
                && (lower.contains("charge") || lower.contains("chg"))
            {
                adjustments.service_charge_minor = amount;
            } else if lower.contains("round") {
                adjustments.rounding_minor = if description.ends_with('-') {
                    -amount
                } else {
                    amount
                };
            } else if ["sst", "gst", "vat", "tax"]
                .iter()
                .any(|word| lower.starts_with(word))
                && !lower.contains("total")
            {
                adjustments.tax_minor = amount;
            }
            continue;
        }
        if description.len() > 500
            || description.chars().filter(|c| c.is_alphabetic()).count() < 2
            || description.contains(':')
            || description.contains('/')
            || description.contains('%')
            || description.contains("@") && !explicit_price.is_match(description)
        {
            continue;
        }
        let (mut name, mut quantity, mut unit_price) = (description.to_string(), None, None);
        if let Some(c) = prefix_quantity.captures(description) {
            quantity = parse_money(&c[1], 3).ok();
            name = c[2].trim().into();
        }
        if let Some(c) = explicit_price.captures(description) {
            name = c[1].trim().into();
            quantity = parse_money(&c[2], 3).ok();
            unit_price = parse_money(&c[3], exponent).ok();
        }
        if quantity.is_some_and(|q| q <= 0 || q > 999_999_999) {
            quantity = None;
        }
        items.push(ReceiptItem {
            description: name,
            quantity_milli: quantity,
            unit_price_minor: unit_price,
            amount_minor: amount,
            confidence: if confidence.is_some_and(|c| c < 60.0) {
                0.4
            } else {
                0.65
            },
        });
        if items.len() == 200 {
            break;
        }
    }
    (items, adjustments)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_totals_and_parses_candidates() {
        let (items,a)=parse("KOPI CAFE\n1 x Kopi O RM4.50\n1 x Nasi Lemak RM8.00\nSUBTOTAL RM12.50\nSST 6% RM0.75\nTOTAL RM13.25\nCASH RM20.00\nCHANGE RM6.75\nTEL: 03 12345678","MYR",Some(95.));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].description, "Kopi O");
        assert_eq!(items[0].quantity_milli, Some(1000));
        assert_eq!(items[1].amount_minor, 800);
        assert_eq!(a.tax_minor, 75);
    }
    #[test]
    fn explicit_qty_price_and_zero_decimal_currency() {
        let (items, _) = parse("Apples 2 x 150 300\nTOTAL 300", "JPY", None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].quantity_milli, Some(2000));
        assert_eq!(items[0].unit_price_minor, Some(150));
        assert_eq!(items[0].amount_minor, 300);
    }
}
