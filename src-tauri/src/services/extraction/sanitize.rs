use crate::domain::{
    expense::{currency_exponent, CATEGORIES, MAX_MONEY},
    extraction::Extraction,
};
use chrono::NaiveDate;
use regex::Regex;
use std::collections::BTreeMap;

const DOLLAR_CURRENCIES: [&str; 5] = ["USD", "SGD", "AUD", "CAD", "HKD"];

/// Whether `code` (an ISO currency code) is one of the currencies a bare `$`
/// can resolve to. Exposed so other extraction modules (e.g. the online
/// provider's printed-currency override) can reuse this list instead of
/// duplicating it.
pub(crate) fn is_dollar_currency(code: &str) -> bool {
    DOLLAR_CURRENCIES.contains(&code.trim().to_uppercase().as_str())
}

pub fn normalize_currency(raw: &str, default_currency: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_uppercase();
    let code = if upper == "RM" {
        "MYR".to_string()
    } else if upper == "US$" {
        "USD".to_string()
    } else if upper == "S$" {
        "SGD".to_string()
    } else if upper.contains('¥') {
        if upper.contains("CN") || upper.contains("RMB") || upper.contains("CNY") {
            "CNY".to_string()
        } else {
            "JPY".to_string()
        }
    } else if upper.contains('€') {
        "EUR".to_string()
    } else if upper.contains('£') {
        "GBP".to_string()
    } else if upper.contains('₹') {
        "INR".to_string()
    } else if upper.contains('฿') {
        "THB".to_string()
    } else if upper.contains('₩') {
        "KRW".to_string()
    } else if upper == "RP" {
        "IDR".to_string()
    } else if upper == "$" {
        let default_upper = default_currency.trim().to_uppercase();
        if DOLLAR_CURRENCIES.contains(&default_upper.as_str()) {
            default_upper
        } else {
            return None;
        }
    } else {
        upper
    };
    currency_exponent(&code).ok().map(|_| code)
}

fn month_from_name(s: &str) -> Option<u32> {
    Some(match s.to_uppercase().as_str() {
        "JAN" => 1,
        "FEB" => 2,
        "MAR" | "MAC" => 3,
        "APR" => 4,
        "MAY" | "MEI" => 5,
        "JUN" => 6,
        "JUL" => 7,
        "AUG" | "OGOS" => 8,
        "SEP" | "SEPT" => 9,
        "OCT" | "OKT" => 10,
        "NOV" => 11,
        "DEC" | "DIS" => 12,
        _ => return None,
    })
}

fn expand_year(raw: &str) -> Option<i32> {
    let value: i32 = raw.parse().ok()?;
    Some(if raw.len() == 2 {
        if value <= 69 {
            2000 + value
        } else {
            1900 + value
        }
    } else {
        value
    })
}

fn finalize(
    year: i32,
    month: u32,
    day: u32,
    ambiguous: bool,
    today: NaiveDate,
) -> Option<(String, bool)> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let min = NaiveDate::from_ymd_opt(2000, 1, 1)?;
    if date < min || date > today + chrono::Duration::days(1) {
        return None;
    }
    Some((date.format("%Y-%m-%d").to_string(), ambiguous))
}

fn capture_iso(text: &str) -> Option<(i32, u32, u32)> {
    let re = Regex::new(r"(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})").ok()?;
    let caps = re.captures(text)?;
    Some((
        caps[1].parse().ok()?,
        caps[2].parse().ok()?,
        caps[3].parse().ok()?,
    ))
}

fn resolve_numeric(
    p1: u32,
    p2: u32,
    year: i32,
    currency: Option<&str>,
) -> Option<(i32, u32, u32, bool)> {
    if p1 == 0 || p2 == 0 || p1 > 31 || p2 > 31 {
        return None;
    }
    if p1 <= 12 && p2 <= 12 {
        let use_month_first = currency.is_some_and(|c| c.eq_ignore_ascii_case("USD"));
        let (month, day) = if use_month_first { (p1, p2) } else { (p2, p1) };
        Some((year, month, day, true))
    } else if p1 > 12 && p2 <= 12 {
        Some((year, p2, p1, false))
    } else if p2 > 12 && p1 <= 12 {
        Some((year, p1, p2, false))
    } else {
        None
    }
}

fn capture_numeric_dmy(text: &str) -> Option<(u32, u32, i32)> {
    let re = Regex::new(r"\b(\d{1,2})[-/.](\d{1,2})[-/.](\d{2,4})\b").ok()?;
    let caps = re.captures(text)?;
    let p1: u32 = caps[1].parse().ok()?;
    let p2: u32 = caps[2].parse().ok()?;
    let year = expand_year(&caps[3])?;
    Some((p1, p2, year))
}

fn capture_day_month_year(text: &str) -> Option<(i32, u32, u32)> {
    let re = Regex::new(r"(?i)\b(\d{1,2})[-\s]([A-Za-z]{3,9})\.?,?[-\s](\d{2,4})\b").ok()?;
    let caps = re.captures(text)?;
    let day: u32 = caps[1].parse().ok()?;
    let month = month_from_name(&caps[2])?;
    let year = expand_year(&caps[3])?;
    Some((year, month, day))
}

fn capture_month_day_year(text: &str) -> Option<(i32, u32, u32)> {
    let re = Regex::new(r"(?i)\b([A-Za-z]{3,9})\.?\s+(\d{1,2}),?\s+(\d{2,4})\b").ok()?;
    let caps = re.captures(text)?;
    let month = month_from_name(&caps[1])?;
    let day: u32 = caps[2].parse().ok()?;
    let year = expand_year(&caps[3])?;
    Some((year, month, day))
}

pub fn normalize_date(
    raw: &str,
    currency: Option<&str>,
    today: NaiveDate,
) -> Option<(String, bool)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((y, m, d)) = capture_iso(trimmed) {
        return finalize(y, m, d, false, today);
    }
    if let Some((p1, p2, y)) = capture_numeric_dmy(trimmed) {
        let (y, m, d, ambiguous) = resolve_numeric(p1, p2, y, currency)?;
        return finalize(y, m, d, ambiguous, today);
    }
    if let Some((y, m, d)) = capture_day_month_year(trimmed) {
        return finalize(y, m, d, false, today);
    }
    if let Some((y, m, d)) = capture_month_day_year(trimmed) {
        return finalize(y, m, d, false, today);
    }
    None
}

fn clean_merchant(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let stripped = collapsed.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
    let capped: String = stripped.chars().take(200).collect();
    let letters = capped.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        None
    } else {
        Some(capped)
    }
}

fn zero(map: &mut BTreeMap<String, f64>, key: &str) {
    map.insert(key.into(), 0.0);
}

pub fn sanitize(result: Extraction, default_currency: &str, today: NaiveDate) -> Extraction {
    let mut r = result;

    let normalized_currency = r
        .currency
        .as_deref()
        .and_then(|c| normalize_currency(c, default_currency));
    if r.currency.is_some() && normalized_currency.is_none() {
        zero(&mut r.confidence, "currency");
    }
    r.currency = normalized_currency;

    match r
        .date
        .as_deref()
        .and_then(|d| normalize_date(d, r.currency.as_deref(), today))
    {
        Some((date, ambiguous)) => {
            r.date = Some(date);
            if ambiguous {
                let entry = r.confidence.entry("date".into()).or_insert(0.0);
                *entry = entry.min(0.5);
            }
        }
        None => {
            if r.date.is_some() {
                zero(&mut r.confidence, "date");
            }
            r.date = None;
        }
    }

    match r.merchant_name.as_deref().and_then(clean_merchant) {
        Some(name) => r.merchant_name = Some(name),
        None => {
            if r.merchant_name.is_some() {
                zero(&mut r.confidence, "merchantName");
            }
            r.merchant_name = None;
        }
    }

    if let Some(t) = r.total_amount_minor {
        if !(0..=MAX_MONEY).contains(&t) {
            r.total_amount_minor = None;
            zero(&mut r.confidence, "total");
        }
    }
    if let Some(t) = r.tax_amount_minor {
        if !(0..=MAX_MONEY).contains(&t) {
            r.tax_amount_minor = None;
            zero(&mut r.confidence, "tax");
        }
    }
    if let (Some(tax), Some(total)) = (r.tax_amount_minor, r.total_amount_minor) {
        if tax > total {
            r.tax_amount_minor = None;
            zero(&mut r.confidence, "tax");
        }
    }

    if r.suggested_category
        .as_ref()
        .is_some_and(|c| !CATEGORIES.contains(&c.as_str()))
    {
        r.suggested_category = None;
        zero(&mut r.confidence, "category");
    }

    // Rebuild the confidence map with exactly the six known keys: this both
    // fills in any that are missing and drops any extras (e.g. from a
    // non-strict provider), which `normalizer::validate` requires to be
    // exactly six. Values are clamped into 0..=1, with non-finite treated as 0.
    let mut confidence = BTreeMap::new();
    for key in [
        "merchantName",
        "date",
        "total",
        "tax",
        "currency",
        "category",
    ] {
        let value = r.confidence.get(key).copied().unwrap_or(0.0);
        let value = if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        };
        confidence.insert(key.to_string(), value);
    }
    r.confidence = confidence;

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 27).unwrap()
    }

    fn extraction() -> Extraction {
        Extraction {
            merchant_name: Some("  ACME   Cafe!! ".into()),
            date: Some("2026-09-27".into()),
            total_amount_minor: Some(1000),
            tax_amount_minor: Some(100),
            currency: Some("RM".into()),
            suggested_category: Some("Meals".into()),
            confidence: BTreeMap::from([
                ("merchantName".to_string(), 0.9),
                ("date".to_string(), 0.9),
                ("total".to_string(), 0.9),
                ("tax".to_string(), 0.9),
                ("currency".to_string(), 0.9),
                ("category".to_string(), 0.9),
            ]),
        }
    }

    // --- normalize_currency ---

    #[test]
    fn currency_rm_variants() {
        assert_eq!(normalize_currency("RM", "MYR"), Some("MYR".into()));
        assert_eq!(normalize_currency("rm", "MYR"), Some("MYR".into()));
        assert_eq!(normalize_currency("Rm", "MYR"), Some("MYR".into()));
    }

    #[test]
    fn currency_symbols() {
        assert_eq!(normalize_currency("S$", "MYR"), Some("SGD".into()));
        assert_eq!(normalize_currency("US$", "MYR"), Some("USD".into()));
        assert_eq!(normalize_currency("€", "MYR"), Some("EUR".into()));
        assert_eq!(normalize_currency("£", "MYR"), Some("GBP".into()));
    }

    #[test]
    fn currency_bare_dollar_depends_on_default() {
        assert_eq!(normalize_currency("$", "MYR"), None);
        assert_eq!(normalize_currency("$", "SGD"), Some("SGD".into()));
    }

    #[test]
    fn currency_yen_cny_cue() {
        assert_eq!(normalize_currency("¥", "MYR"), Some("JPY".into()));
        assert_eq!(normalize_currency("CN¥", "MYR"), Some("CNY".into()));
    }

    #[test]
    fn currency_other_symbols_and_codes() {
        assert_eq!(normalize_currency("₹", "MYR"), Some("INR".into()));
        assert_eq!(normalize_currency("฿", "MYR"), Some("THB".into()));
        assert_eq!(normalize_currency("Rp", "MYR"), Some("IDR".into()));
        assert_eq!(normalize_currency("₩", "MYR"), Some("KRW".into()));
        assert_eq!(normalize_currency("sgd", "MYR"), Some("SGD".into()));
    }

    #[test]
    fn currency_unrecognized_is_none() {
        assert_eq!(normalize_currency("XYZ", "MYR"), None);
        assert_eq!(normalize_currency("", "MYR"), None);
    }

    // --- normalize_date: accepted formats ---

    #[test]
    fn date_iso_dash() {
        assert_eq!(
            normalize_date("2026-09-27", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    #[test]
    fn date_iso_slash() {
        assert_eq!(
            normalize_date("2026/09/27", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    #[test]
    fn date_iso_dot() {
        assert_eq!(
            normalize_date("2026.09.27", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    #[test]
    fn date_numeric_two_digit_year() {
        assert_eq!(
            normalize_date("27/09/26", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    #[test]
    fn date_numeric_dash_and_dot_separators() {
        assert_eq!(
            normalize_date("13-04-2026", None, today()),
            Some(("2026-04-13".into(), false))
        );
        assert_eq!(
            normalize_date("13.04.2026", None, today()),
            Some(("2026-04-13".into(), false))
        );
    }

    #[test]
    fn date_month_name_day_month_year() {
        assert_eq!(
            normalize_date("27 Sep 2026", None, today()),
            Some(("2026-09-27".into(), false))
        );
        assert_eq!(
            normalize_date("27-SEP-26", None, today()),
            Some(("2026-09-27".into(), false))
        );
        assert_eq!(
            normalize_date("27 Sept 2026", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    #[test]
    fn date_month_name_month_day_year() {
        assert_eq!(
            normalize_date("Sep 27, 2026", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    #[test]
    fn date_malay_month_names() {
        assert_eq!(
            normalize_date("27 Mac 2026", None, today()),
            Some(("2026-03-27".into(), false))
        );
        assert_eq!(
            normalize_date("27 Ogos 2026", None, today()),
            Some(("2026-08-27".into(), false))
        );
        assert_eq!(
            normalize_date("27 Dis 2025", None, today()),
            Some(("2025-12-27".into(), false))
        );
    }

    #[test]
    fn date_surrounding_text_and_time() {
        assert_eq!(
            normalize_date("Date: 27/09/2026 14:05", None, today()),
            Some(("2026-09-27".into(), false))
        );
    }

    // --- normalize_date: ambiguity rules ---

    #[test]
    fn date_ambiguous_myr_day_first() {
        assert_eq!(
            normalize_date("03/04/2026", Some("MYR"), today()),
            Some(("2026-04-03".into(), true))
        );
    }

    #[test]
    fn date_ambiguous_usd_month_first() {
        assert_eq!(
            normalize_date("03/04/2026", Some("USD"), today()),
            Some(("2026-03-04".into(), true))
        );
    }

    #[test]
    fn date_unambiguous_regardless_of_currency() {
        assert_eq!(
            normalize_date("13/04/2026", Some("MYR"), today()),
            Some(("2026-04-13".into(), false))
        );
        assert_eq!(
            normalize_date("13/04/2026", Some("USD"), today()),
            Some(("2026-04-13".into(), false))
        );
    }

    #[test]
    fn date_unambiguous_month_over_twelve_in_second_slot() {
        assert_eq!(
            normalize_date("04/13/2026", None, today()),
            Some(("2026-04-13".into(), false))
        );
    }

    // --- normalize_date: rejection ---

    #[test]
    fn date_rejects_future_beyond_one_day() {
        assert_eq!(normalize_date("2026-09-29", None, today()), None);
        // exactly one day ahead is allowed
        assert!(normalize_date("2026-09-28", None, today()).is_some());
    }

    #[test]
    fn date_rejects_pre_2000() {
        assert_eq!(normalize_date("1999-12-31", None, today()), None);
    }

    // --- sanitize ---

    #[test]
    fn sanitize_keeps_valid_fields_while_nulling_bad_date() {
        let mut e = extraction();
        e.date = Some("not a date".into());
        let out = sanitize(e, "MYR", today());
        assert_eq!(out.date, None);
        assert_eq!(out.confidence.get("date"), Some(&0.0));
        assert_eq!(out.merchant_name, Some("ACME Cafe".into()));
        assert_eq!(out.currency, Some("MYR".into()));
        assert_eq!(out.total_amount_minor, Some(1000));
    }

    #[test]
    fn sanitize_drops_tax_greater_than_total() {
        let mut e = extraction();
        e.total_amount_minor = Some(100);
        e.tax_amount_minor = Some(200);
        let out = sanitize(e, "MYR", today());
        assert_eq!(out.tax_amount_minor, None);
        assert_eq!(out.confidence.get("tax"), Some(&0.0));
        assert_eq!(out.total_amount_minor, Some(100));
    }

    #[test]
    fn sanitize_drops_negative_and_over_max_money() {
        let mut e = extraction();
        e.total_amount_minor = Some(-5);
        e.tax_amount_minor = Some(MAX_MONEY + 1);
        let out = sanitize(e, "MYR", today());
        assert_eq!(out.total_amount_minor, None);
        assert_eq!(out.tax_amount_minor, None);
    }

    #[test]
    fn sanitize_fills_missing_confidence_keys() {
        let mut e = extraction();
        e.confidence = BTreeMap::new();
        let out = sanitize(e, "MYR", today());
        for key in [
            "merchantName",
            "date",
            "total",
            "tax",
            "currency",
            "category",
        ] {
            assert_eq!(out.confidence.get(key), Some(&0.0));
        }
    }

    #[test]
    fn sanitize_clamps_out_of_range_and_non_finite_confidence() {
        let mut e = extraction();
        e.confidence.insert("total".into(), 5.0);
        e.confidence.insert("tax".into(), -1.0);
        e.confidence.insert("category".into(), f64::NAN);
        let out = sanitize(e, "MYR", today());
        assert_eq!(out.confidence.get("total"), Some(&1.0));
        assert_eq!(out.confidence.get("tax"), Some(&0.0));
        assert_eq!(out.confidence.get("category"), Some(&0.0));
    }

    #[test]
    fn sanitize_caps_ambiguous_date_confidence_at_half() {
        let mut e = extraction();
        e.date = Some("03/04/2026".into());
        e.currency = Some("MYR".into());
        e.confidence.insert("date".into(), 0.9);
        let out = sanitize(e, "MYR", today());
        assert_eq!(out.date, Some("2026-04-03".into()));
        assert_eq!(out.confidence.get("date"), Some(&0.5));
    }

    #[test]
    fn sanitize_drops_short_merchant_and_invalid_category() {
        let mut e = extraction();
        e.merchant_name = Some("12".into());
        e.suggested_category = Some("Bogus".into());
        let out = sanitize(e, "MYR", today());
        assert_eq!(out.merchant_name, None);
        assert_eq!(out.confidence.get("merchantName"), Some(&0.0));
        assert_eq!(out.suggested_category, None);
        assert_eq!(out.confidence.get("category"), Some(&0.0));
    }

    #[test]
    fn sanitize_output_always_satisfies_normalizer_validate_even_when_hostile() {
        use crate::services::extraction::normalizer;
        let hostile = Extraction {
            merchant_name: Some("Z".repeat(400)),
            date: Some("not a real date".into()),
            total_amount_minor: Some(-500),
            tax_amount_minor: Some(100),
            currency: Some("ZZZ".into()),
            suggested_category: Some("NotACategory".into()),
            confidence: BTreeMap::from([
                ("merchantName".to_string(), 0.9),
                ("date".to_string(), f64::NAN),
                ("total".to_string(), -3.0),
                ("tax".to_string(), 7.0),
                ("currency".to_string(), 0.9),
                ("category".to_string(), 0.9),
                // Extra keys a non-strict provider might add: must be dropped
                // or `normalizer::validate`'s `len() != 6` check will fail.
                ("bogusExtraKey".to_string(), 0.42),
                ("anotherBogusKey".to_string(), 999.0),
            ]),
        };
        let out = sanitize(hostile, "MYR", today());
        normalizer::validate(&out)
            .expect("sanitize output must always satisfy normalizer::validate");

        // Fields with no salvageable reading are nulled...
        assert_eq!(out.date, None);
        assert_eq!(out.total_amount_minor, None);
        assert_eq!(out.currency, None);
        assert_eq!(out.suggested_category, None);
        // ...but a field that can be salvaged (an overlong merchant name) is
        // preserved (truncated) rather than discarded, and a legitimately
        // in-range value (tax, once compared against a *dropped* total) is
        // kept rather than nulled by the confidence clean-up.
        assert_eq!(out.merchant_name.as_deref(), Some("Z".repeat(200).as_str()));
        assert_eq!(out.tax_amount_minor, Some(100));
        // Exactly the six known keys survive, extras are gone.
        assert_eq!(out.confidence.len(), 6);
        assert!(!out.confidence.contains_key("bogusExtraKey"));
        assert!(!out.confidence.contains_key("anotherBogusKey"));
    }
}
