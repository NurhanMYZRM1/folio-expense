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

// How confidences are earned
// --------------------------
// An expense is marked ready without review only when merchant, date, total
// and currency all reach 0.9 (see `complete_extraction`). Offline OCR earns
// that level only from evidence printed on the receipt itself:
// - total: the receipt's own arithmetic agrees (subtotal + tax + service ±
//   rounding − discount, cash − change, or a card/e-wallet line paying the
//   same amount), or it is the largest amount on a clearly read page;
// - date: a single possible reading (ISO, month name, a part over 12, or the
//   other reading is in the future), or a day/month order confirmed by the
//   currency printed on the receipt;
// - currency: printed (`RM`, `S$`, `MYR`…), implied by the address/company
//   registration (Malaysia or Singapore), or the home currency from Settings
//   when nothing on the receipt suggests another one;
// - merchant: a business-name line (company suffix or business type) or a
//   clean name line at the very top.
// Anything less stays below 0.9 so the expense waits for review.

/// Tesseract page confidence (0–100) below which the scan counts as
/// unreadable: every field is capped below the ready threshold.
const UNREADABLE_OCR_CONFIDENCE: f64 = 60.0;
const UNREADABLE_FIELD_CAP: f64 = 0.6;
/// Page confidence at or above which a total that is merely the largest
/// amount on the receipt (no arithmetic cross-check) is trusted.
const CLEAR_OCR_CONFIDENCE: f64 = 75.0;

/// Currencies whose receipts print numeric dates day-first. `USD` is the
/// month-first convention `sanitize::normalize_date` already applies.
const DAY_FIRST_CURRENCIES: &[&str] = &[
    "MYR", "SGD", "GBP", "EUR", "INR", "THB", "IDR", "AUD", "HKD",
];

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
    // `BHD` only next to an amount: on its own it is Malaysia's "Berhad"
    // (`ABC SDN BHD`), not the Bahraini dinar.
    (r"\bBHD[ \t]*\d|\d[ \t]*BHD\b", "BHD"),
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

// Address/registration cues that place a receipt without a printed currency
// in Malaysia or Singapore.
const MALAYSIA_CUES: &str = r"(?i)SDN\.?\s*BHD|\bBERHAD\b|\bMALAYSIA\b|KUALA LUMPUR|SELANGOR|\bJOHOR|PENANG|PULAU PINANG|\bPERAK\b|\bSABAH\b|SARAWAK|MELAKA|MALACCA|\bKEDAH\b|PAHANG|NEGERI SEMBILAN|TERENGGANU|KELANTAN|PUTRAJAYA|CYBERJAYA|\bJALAN\b|\bJLN\b|\bTAMAN\b|\+60|\bSST\b";
const SINGAPORE_CUES: &str = r"(?i)\bSINGAPORE\b|PTE\.?\s*LTD|\+65";

fn region_currency(text: &str) -> Result<Option<&'static str>> {
    let malaysia = re(MALAYSIA_CUES)?.is_match(text);
    let singapore = re(SINGAPORE_CUES)?.is_match(text);
    Ok(match (malaysia, singapore) {
        (true, false) => Some("MYR"),
        (false, true) => Some("SGD"),
        _ => None,
    })
}

/// What a receipt line is about, judged from its label. `Total` carries a
/// strength: 3 for explicit final-amount labels (`GRAND TOTAL`,
/// `AMOUNT DUE`, `TOTAL ROUNDED`…), 2 for a plain `TOTAL`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LineKind {
    Total(u8),
    Subtotal,
    Tax,
    Service,
    Rounding,
    Discount,
    Payment,
    Change,
    Other,
}

struct Labels {
    total: Regex,
    subtotal: Regex,
    strong_total: Regex,
    not_a_total: Regex,
    tax: Regex,
    tax_reference: Regex,
    service: Regex,
    rounding: Regex,
    discount: Regex,
    payment: Regex,
    change: Regex,
}

impl Labels {
    fn new() -> Result<Self> {
        Ok(Self {
            // `T0TAL`, `TOTA1` and `TOTAI` are common OCR misreads of TOTAL.
            total: re(r"T[O0]TA[L1I]|AMOUNT DUE|BALANCE DUE|AMOUNT PAYABLE|\bJUMLAH\b")?,
            subtotal: re(r"SUB\s*-?\s*T[O0]TA[L1I]")?,
            strong_total: re(
                r"GRAND|\bNETT?\b|AMOUNT|\bDUE\b|PAYABLE|ROUND|INCL|\bJUMLAH\b|\bBESAR\b",
            )?,
            not_a_total: re(r"\bQTY\b|QUANTITY|\bITEMS?\b|SAVING|EXCL|BEFORE|POINTS?\b")?,
            tax: re(r"\bTAX\b|\bSST\b|\bGST\b|\bVAT\b|\bCUKAI\b")?,
            tax_reference: re(r"\bREG\b|\bNO\b|\bID\b|INVOICE|\bNUMBER\b|\d{5,}")?,
            service: re(r"SERVICE|\bSVC\b|\bS/C\b")?,
            rounding: re(r"ROUND|\bADJ")?,
            discount: re(r"DISCOUNT|\bDISC\b|VOUCHER|\bLESS\b")?,
            payment: re(
                r"\bCASH\b|\bTUNAI\b|\bCARD\b|\bVISA\b|MASTER|\bAMEX\b|\bDEBIT\b|\bCREDIT\b|\bPAID\b|PAYMENT|TENDER|E-?WALLET|\bTNG\b|TOUCH\s*N\s*GO|GRABPAY|\bBOOST\b|DUITNOW|SHOPEEPAY|PAYNOW|\bNETS\b",
            )?,
            change: re(r"\bCHANGE\b|\bBAKI\b")?,
        })
    }

    fn classify(&self, line: &str) -> LineKind {
        let upper = line.to_uppercase();
        if self.change.is_match(&upper) {
            return LineKind::Change;
        }
        if self.subtotal.is_match(&upper) {
            return LineKind::Subtotal;
        }
        let total = self.total.is_match(&upper);
        let rounding = self.rounding.is_match(&upper);
        if rounding && !total {
            return LineKind::Rounding;
        }
        if self.discount.is_match(&upper) {
            return LineKind::Discount;
        }
        let tax = self.tax.is_match(&upper);
        if total && !self.not_a_total.is_match(&upper) && (!tax || upper.contains("INCL")) {
            let strength = if self.strong_total.is_match(&upper) {
                3
            } else {
                2
            };
            return LineKind::Total(strength);
        }
        if total {
            // e.g. `TOTAL TAX`, `TOTAL QTY 3`: never the amount paid.
            return if tax { LineKind::Tax } else { LineKind::Other };
        }
        if tax && !self.tax_reference.is_match(&upper) {
            return LineKind::Tax;
        }
        if self.service.is_match(&upper) {
            return LineKind::Service;
        }
        if self.payment.is_match(&upper) {
            return LineKind::Payment;
        }
        LineKind::Other
    }
}

/// Repairs common OCR misreads inside amount-shaped tokens only (`1O.5O` →
/// `10.50`, `l3.25` → `13.25`, `13,25` → `13.25`); other text is untouched.
fn repair_amounts(line: &str, token_re: &Regex) -> String {
    token_re
        .replace_all(line, |caps: &regex::Captures| {
            let token = &caps[0];
            if !token.bytes().any(|b| b.is_ascii_digit()) {
                return token.to_string();
            }
            let decimal_comma = token.len() - 3;
            token
                .char_indices()
                .map(|(i, c)| match c {
                    'O' | 'o' => '0',
                    'I' | 'l' => '1',
                    ',' if i == decimal_comma => '.',
                    other => other,
                })
                .collect()
        })
        .into_owned()
}

/// Signed amounts on a (repaired) line, in order. With a currency that has
/// decimals, only amounts printed with decimals count unless
/// `allow_integer` (a bare `TOTAL 85` is still a total, but `Table 12` or a
/// registration number are not amounts).
fn amounts_in(line: &str, number_re: &Regex, exponent: u32, allow_integer: bool) -> Vec<i64> {
    let mut found = Vec::new();
    for m in number_re.find_iter(line) {
        let token = m.as_str();
        if exponent > 0 && !token.contains('.') && !allow_integer {
            continue;
        }
        let Ok(value) = parse_money(token, exponent) else {
            continue;
        };
        let before = line[..m.start()]
            .trim_end()
            .trim_end_matches(|c: char| c.is_alphabetic() || "$€£¥₹฿₩".contains(c))
            .trim_end();
        let negative =
            before.ends_with('-') || before.ends_with('(') || line[m.end()..].starts_with('-');
        found.push(if negative { -value } else { value });
    }
    found
}

/// A line that holds nothing but an amount (OCR often puts the value of a
/// right-aligned `TOTAL` on its own line).
fn is_amount_only(line: &str) -> bool {
    let rest = line
        .trim()
        .trim_start_matches(|c: char| c.is_alphabetic() || "$€£¥₹฿₩:".contains(c))
        .trim();
    !rest.is_empty()
        && rest.bytes().any(|b| b.is_ascii_digit())
        && rest
            .bytes()
            .all(|b| b.is_ascii_digit() || b",.- ()".contains(&b))
}

/// Everything the amount lines of a receipt say, used to pick and verify the
/// total.
#[derive(Default)]
struct AmountEvidence {
    /// (strength, amount) for each total-labelled line, in order.
    totals: Vec<(u8, i64)>,
    subtotal: Option<i64>,
    taxes: Vec<i64>,
    service: i64,
    rounding: i64,
    discount: i64,
    payments: Vec<i64>,
    change: Option<i64>,
    /// Item, subtotal and tax amounts: everything that must not exceed the total.
    others: Vec<i64>,
}

impl AmountEvidence {
    fn collect(lines: &[String], labels: &Labels, number_re: &Regex, exponent: u32) -> Self {
        let mut evidence = Self::default();
        for (index, line) in lines.iter().enumerate() {
            let kind = labels.classify(line);
            // For `SST 6% 0.75` read after the `%`, so the rate is not taken
            // for the amount; `SST @ 6%` has no amount at all.
            let text = match kind {
                LineKind::Tax | LineKind::Service => line.rsplit('%').next().unwrap_or(line),
                _ => line.as_str(),
            };
            let allow_integer = matches!(kind, LineKind::Total(_));
            let mut amount = amounts_in(text, number_re, exponent, allow_integer)
                .last()
                .copied();
            if amount.is_none() && kind != LineKind::Other {
                amount = lines
                    .get(index + 1)
                    .filter(|next| is_amount_only(next))
                    .and_then(|next| amounts_in(next, number_re, exponent, allow_integer).pop());
            }
            let Some(amount) = amount else { continue };
            match kind {
                LineKind::Total(strength) if amount > 0 => evidence.totals.push((strength, amount)),
                LineKind::Subtotal => {
                    evidence.subtotal = Some(amount.abs());
                    evidence.others.push(amount.abs());
                }
                LineKind::Tax => {
                    evidence.taxes.push(amount.abs());
                    evidence.others.push(amount.abs());
                }
                LineKind::Service => evidence.service += amount.abs(),
                LineKind::Rounding => evidence.rounding += amount,
                LineKind::Discount => evidence.discount += amount.abs(),
                LineKind::Payment => evidence.payments.push(amount.abs()),
                LineKind::Change => evidence.change = Some(amount.abs()),
                LineKind::Other => evidence.others.push(amount.abs()),
                LineKind::Total(_) => {}
            }
        }
        evidence
    }

    /// The strongest total label wins; among equals, the last one printed
    /// (e.g. `TOTAL ROUNDED` after `TOTAL`).
    fn total(&self) -> Option<i64> {
        let best = self.totals.iter().map(|(s, _)| *s).max()?;
        self.totals
            .iter()
            .rev()
            .find(|(s, _)| *s == best)
            .map(|(_, a)| *a)
    }

    /// Whether another part of the receipt independently agrees with `total`.
    fn confirms(&self, total: i64, exponent: u32) -> bool {
        // Cash rounding (e.g. Malaysia's 5-sen rounding) may be unprinted.
        let tolerance = if exponent >= 2 { 2 } else { 0 };
        let close = |value: i64| (value - total).abs() <= tolerance;
        let tax: i64 = self.taxes.iter().sum();
        let arithmetic = self.subtotal.is_some_and(|subtotal| {
            let base = subtotal - self.discount + self.service;
            // Tax added on top, or already included in the subtotal.
            close(base + tax) || close(base + tax + self.rounding) || close(base + self.rounding)
        });
        let paid = self.payments.contains(&total);
        let tendered = self
            .change
            .is_some_and(|change| self.payments.iter().any(|p| p - change == total));
        arithmetic || paid || tendered
    }

    fn is_largest(&self, total: i64) -> bool {
        self.others.iter().all(|a| *a <= total)
    }
}

// Candidate date substrings covering the formats `sanitize::normalize_date`
// accepts: ISO, numeric D/M/Y (or M/D/Y), `D Month Y` and `Month D, Y`.
const DATE_CANDIDATE_PATTERN: &str = r"(?i)\b\d{4}[-/.]\d{1,2}[-/.]\d{1,2}\b|\b\d{1,2}[-/.]\d{1,2}[-/.]\d{2,4}\b|\b\d{1,2}[-\s./][A-Za-z]{3,9}\.?,?[-\s./]\d{2,4}\b|\b[A-Za-z]{3,9}\.?\s+\d{1,2},?\s+\d{2,4}\b";
const DATE_LABEL_PATTERN: &str = r"(?i)\bDATE\b|\bTARIKH\b|\bDATED\b";

const PHONE_PATTERN: &str = r"(?i)\bTEL\b|\bPHONE\b|\+60|\d{7,}";
const REG_NUMBER_PATTERN: &str =
    r"(?i)\bSST\b|\bGST\b|\bREG\b|CO\.?\s*NO\.?|\bROC\b|\(\d+-[A-Z0-9]+\)";
const ADDRESS_PATTERN: &str = r"(?i)\b\d{5}\b|\bJALAN\b|\bJLN\b|\bSTREET\b|\bROAD\b|\bLOT\b|\bNO\.\s*\d+|\bTAMAN\b|KUALA LUMPUR|SELANGOR";
const BOILERPLATE_PATTERN: &str = r"(?i)\bRECEIPT\b|\bINVOICE\b|TAX INVOICE|\bWELCOME\b|THANK YOU|CASH SALE|\bOFFICIAL\b|\bCOPY\b";
const TIME_PATTERN: &str = r"\b\d{1,2}:\d{2}\b";
/// Company suffixes and business types: a line near the top containing one
/// of these is the business name.
const SUFFIX_PATTERN: &str = r"(?i)SDN\.?\s*BHD\.?|\bBHD\b|\bBERHAD\b|\bPLT\b|\bENTERPRISE\b|\bTRADING\b|\bRESTAURANT\b|\bRESTORAN\b|\bCAF[EÉ]\b|PTE\.?\s*LTD\.?|\bLTD\b|\bLIMITED\b|\bLLC\b|\bINC\b|\bCORP(ORATION)?\b|\bHOTEL\b|\bBAKERY\b|\bMART\b|\bSUPERMARKET\b|\bPHARMACY\b|\bFARMASI\b|\bKEDAI\b";

/// A name line OCR read cleanly: letters, digits and the punctuation names
/// use, mostly letters.
fn is_clean_name(line: &str) -> bool {
    let chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    let letters = chars.iter().filter(|c| c.is_alphabetic()).count();
    letters >= 4
        && letters * 10 >= chars.len() * 6
        && chars
            .iter()
            .all(|c| c.is_alphanumeric() || "&'.,-()@/".contains(*c))
}

/// Keyword → category rules, checked in order (the first match wins). The
/// merchant line is checked before the rest of the receipt, so a hotel
/// receipt listing breakfast is still Accommodation.
const CATEGORY_RULES: &[(&str, &str)] = &[
    (
        "Fuel",
        r"PETRONAS|\bSHELL\b|PETRON\b|CALTEX|BHP\b|BH PETROL|\bESSO\b|\bFUEL\b|PETROL|DIESEL|\bRON\s?9[57]\b|LITRE|\bLITER",
    ),
    ("Parking", r"PARKING|PARKIR|CAR\s?PARK"),
    (
        "Accommodation",
        r"\bHOTEL|\bINN\b|RESORT|MOTEL|HOSTEL|\bLODGE\b|AIRBNB|ROOM CHARGE|\bSUITES?\b",
    ),
    (
        "Travel",
        r"AIRASIA|AIR ASIA|AIRLINES?\b|AIRWAYS|\bFLIGHT|BOARDING|BAGGAGE|FIREFLY|BATIK AIR",
    ),
    (
        "Transport",
        r"\bGRAB\b|\bTAXI\b|\bTEKSI\b|\bMRT\b|\bLRT\b|\bKTM\b|RAPID\s?KL|\bTOLL\b|\bBUS\b|\bTRAIN\b|\bERL\b|\bUBER\b|\bLYFT\b",
    ),
    (
        "Meals",
        r"RESTAURANT|RESTORAN|\bCAF[EÉ]\b|COFFEE|\bKOPI|BAKERY|\bFOOD|\bNASI\b|MAMAK|BISTRO|KITCHEN|DINING|MCDONALD|\bKFC\b|STARBUCKS|PIZZA|BURGER|\bMEE\b|\bROTI\b|LUNCH|DINNER|BREAKFAST|CANTEEN|BEVERAGE",
    ),
    (
        "Office Supplies",
        r"STATIONER|PRINTING|PRINTER|\bTONER\b|\bINK\b|PHOTOSTAT|\bA4\b|OFFICE SUPPL",
    ),
    (
        "Software",
        r"SOFTWARE|SUBSCRIPTION|LICEN[CS]E|\bSAAS\b|HOSTING|\bDOMAIN\b",
    ),
    (
        "Entertainment",
        r"CINEMA|\bGSC\b|\bTGV\b|KARAOKE|\bMOVIE|BOWLING",
    ),
];

fn suggest_category(merchant: Option<&str>, text: &str) -> Result<(&'static str, f64)> {
    for (source, confidence) in [(merchant.unwrap_or(""), 0.85), (text, 0.7)] {
        let upper = source.to_uppercase();
        for (category, pattern) in CATEGORY_RULES {
            if re(pattern)?.is_match(&upper) {
                return Ok((category, confidence));
            }
        }
    }
    Ok(("Other", 0.2))
}

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
        let labels = Labels::new()?;

        // --- Currency: the TOTAL line, else anywhere, else the address.
        let upper_text = input.raw_text.to_uppercase();
        let cny_cue =
            upper_text.contains("RMB") || upper_text.contains("CNY") || upper_text.contains("CN¥");
        let patterns = currency_patterns()?;
        let normalize = |raw: Option<String>| {
            raw.as_deref()
                .and_then(|c| sanitize::normalize_currency(c, input.default_currency))
        };
        let on_total_line = normalize(
            lines
                .iter()
                .rev()
                .find(|line| matches!(labels.classify(line), LineKind::Total(_)))
                .and_then(|line| detect_currency_raw(&patterns, line, cny_cue)),
        );
        let anywhere = detect_currency_raw(&patterns, input.raw_text, cny_cue);
        let unresolved_symbol = anywhere.is_some() && normalize(anywhere.clone()).is_none();
        let (currency, currency_confidence, currency_from_receipt) = if let Some(c) = on_total_line
        {
            (c, 0.95, true)
        } else if let Some(c) = normalize(anywhere) {
            (c, 0.9, true)
        } else if let Some(c) = region_currency(input.raw_text)? {
            (c.to_string(), 0.9, true)
        } else if unresolved_symbol {
            // e.g. a bare `$` while Settings says MYR: the receipt is probably
            // foreign, so the default is only a placeholder.
            (input.default_currency.to_string(), 0.25, false)
        } else {
            // Nothing on the receipt names or implies a currency, and nothing
            // contradicts the user's home currency from Settings.
            (input.default_currency.to_string(), 0.9, false)
        };
        let exponent = currency_exponent(&currency)?;

        // --- Total / tax amounts, cross-checked against the rest of the receipt.
        let token_re = re(r"\b[0-9OoIl]+[.,][0-9Oo]{2}\b")?;
        let number_re = re(r"\d[\d,]*(?:\.\d+)?")?;
        let date_re = re(DATE_CANDIDATE_PATTERN)?;
        // Dates are blanked first so `27.09.2026` is not read as 27.09.
        let repaired: Vec<String> = lines
            .iter()
            .map(|line| repair_amounts(&date_re.replace_all(line, " "), &token_re))
            .collect();
        let evidence = AmountEvidence::collect(&repaired, &labels, &number_re, exponent);
        let total = evidence.total();
        let mut tax = evidence.taxes.last().copied();
        if tax.zip(total).is_some_and(|(a, b)| a > b) {
            tax = None;
        }
        let clear_page = input
            .ocr_confidence
            .is_some_and(|c| c >= CLEAR_OCR_CONFIDENCE);
        let total_confirmed = total.is_some_and(|t| evidence.confirms(t, exponent));
        let total_confidence = match total {
            None => 0.0,
            Some(_) if total_confirmed => 0.97,
            Some(t) if clear_page && evidence.is_largest(t) => 0.9,
            Some(_) if evidence.totals.iter().any(|(s, _)| *s == 3) => 0.8,
            Some(_) => 0.75,
        };
        let tax_confidence = match tax {
            None => 0.0,
            Some(_) if total_confirmed => 0.9,
            Some(_) => 0.65,
        };

        // --- Date: a labelled line first, then anywhere in the text.
        let date_label_re = re(DATE_LABEL_PATTERN)?;
        let labelled = lines.iter().filter(|line| date_label_re.is_match(line));
        let mut date = None;
        'search: for (text, is_labelled) in labelled
            .map(|line| (*line, true))
            .chain(std::iter::once((input.raw_text, false)))
        {
            for m in date_re.find_iter(text) {
                if let Some((d, ambiguous)) =
                    sanitize::normalize_date(m.as_str(), Some(&currency), today)
                {
                    date = Some((d, ambiguous, is_labelled));
                    break 'search;
                }
            }
        }
        // An ambiguous numeric date follows the convention of the currency
        // printed on the receipt; it is only uncertain when the currency was
        // a guess from Settings.
        let convention_confirmed = currency_from_receipt
            && (currency == "USD" || DAY_FIRST_CURRENCIES.contains(&currency.as_str()));
        let date_confidence = match &date {
            None => 0.0,
            Some((_, true, _)) if convention_confirmed => 0.9,
            Some((_, true, _)) => 0.5,
            Some((_, false, true)) => 0.95,
            Some((_, false, false)) => 0.9,
        };

        // --- Merchant: first 8 non-empty lines, filtered, business-name preferred.
        let time_re = re(TIME_PATTERN)?;
        let phone_re = re(PHONE_PATTERN)?;
        let reg_re = re(REG_NUMBER_PATTERN)?;
        let address_re = re(ADDRESS_PATTERN)?;
        let boilerplate_re = re(BOILERPLATE_PATTERN)?;
        let suffix_re = re(SUFFIX_PATTERN)?;
        let candidates: Vec<(usize, &str)> = lines
            .iter()
            .take(8)
            .copied()
            .enumerate()
            .filter(|(_, line)| {
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
        let suffix_line = candidates.iter().find(|(_, line)| suffix_re.is_match(line));
        let (merchant, merchant_confidence) = match suffix_line.or_else(|| candidates.first()) {
            Some((index, line)) => {
                let clean = is_clean_name(line);
                let confidence = match (suffix_line.is_some(), clean) {
                    (true, true) => 0.92,
                    (true, false) => 0.75,
                    // A clean name in the top three lines is the business name.
                    (false, true) if *index < 3 => 0.9,
                    (false, _) => 0.55,
                };
                (Some(line.chars().take(300).collect::<String>()), confidence)
            }
            None => (None, 0.0),
        };

        let (category, category_confidence) =
            suggest_category(merchant.as_deref(), input.raw_text)?;

        // A blurry or unreadable scan keeps every field in review.
        let cap = match input.ocr_confidence {
            Some(c) if c < UNREADABLE_OCR_CONFIDENCE => UNREADABLE_FIELD_CAP,
            _ => 1.0,
        };
        let mut confidence = BTreeMap::new();
        for (field, value) in [
            ("merchantName", merchant_confidence),
            ("date", date_confidence),
            ("total", total_confidence),
            ("tax", tax_confidence),
            ("currency", currency_confidence),
            ("category", category_confidence),
        ] {
            confidence.insert(field.into(), f64::min(value, cap));
        }
        Ok(Extraction {
            merchant_name: merchant,
            date: date.map(|(d, _, _)| d),
            total_amount_minor: total,
            tax_amount_minor: tax,
            currency: Some(currency),
            suggested_category: Some(category.into()),
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

    fn run_with(raw_text: &str, default_currency: &str, ocr_confidence: Option<f64>) -> Extraction {
        let input = ExtractionInput {
            raw_text,
            images: &[],
            default_currency,
            ocr_confidence,
        };
        LocalOcrExtractor
            .extract_with_today(&input, today())
            .unwrap()
    }

    fn run(raw_text: &str, default_currency: &str) -> Extraction {
        run_with(raw_text, default_currency, Some(90.0))
    }

    fn conf(out: &Extraction, field: &str) -> f64 {
        out.confidence[field]
    }

    /// The same rule `complete_extraction` applies to mark an expense ready.
    fn would_be_ready(out: &Extraction) -> bool {
        out.merchant_name.is_some()
            && out.date.is_some()
            && out.total_amount_minor.is_some()
            && out.currency.is_some()
            && ["merchantName", "date", "total", "currency"]
                .iter()
                .all(|k| conf(out, k) >= 0.9)
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
        assert_eq!(out.tax_amount_minor, Some(75));
        assert_eq!(out.suggested_category.as_deref(), Some("Meals"));
        // 12.50 + 0.75 = 13.25: the receipt's own arithmetic confirms the total.
        assert_eq!(conf(&out, "total"), 0.97);
        assert_eq!(conf(&out, "merchantName"), 0.92);
        assert!(would_be_ready(&out));
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
        assert_eq!(conf(&out, "merchantName"), 0.92);
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

    #[test]
    fn full_month_name_date_is_recognized() {
        let text = "\
GLOBAL MART SDN BHD
Date: 27 September 2026
1 x Item RM10.00
TOTAL RM10.00";
        let out = run(text, "MYR");
        assert_eq!(out.date.as_deref(), Some("2026-09-27"));
        assert_eq!(conf(&out, "date"), 0.95);
    }

    #[test]
    fn cash_tendered_minus_change_confirms_the_total() {
        let text = "\
RESTORAN SRI MELAYU
Date: 15/09/2026
Nasi Campur 12.00
Teh Tarik 3.00
TOTAL 15.00
CASH 20.00
CHANGE 5.00";
        let out = run(text, "MYR");
        assert_eq!(out.total_amount_minor, Some(1500));
        assert_eq!(conf(&out, "total"), 0.97);
        assert!(would_be_ready(&out));
    }

    #[test]
    fn card_payment_of_the_same_amount_confirms_the_total() {
        let text = "\
MR DIY TRADING SDN BHD
2026-09-20
Extension cord 29.90
TOTAL 29.90
VISA 29.90";
        let out = run(text, "MYR");
        assert_eq!(out.total_amount_minor, Some(2990));
        assert_eq!(conf(&out, "total"), 0.97);
    }

    #[test]
    fn grand_total_with_service_charge_and_rounding() {
        let text = "\
THE BREAKFAST CLUB
Date: 2026-09-21
Big Breakfast 38.00
Latte 12.90
SUBTOTAL 50.90
SERVICE CHARGE 10% 5.09
SST 6% 3.05
ROUNDING -0.04
GRAND TOTAL 59.00
TOTAL ITEMS 2";
        let out = run(text, "MYR");
        // 50.90 + 5.09 + 3.05 − 0.04 = 59.00. `TOTAL ITEMS 2` is a count.
        assert_eq!(out.total_amount_minor, Some(5900));
        assert_eq!(conf(&out, "total"), 0.97);
    }

    #[test]
    fn total_amount_on_the_following_line_is_used() {
        let text = "\
CITY PARKING SDN BHD
2026-09-22
TOTAL
RM 6.00
PAID 6.00";
        let out = run(text, "MYR");
        assert_eq!(out.total_amount_minor, Some(600));
        assert_eq!(out.suggested_category.as_deref(), Some("Parking"));
    }

    #[test]
    fn ocr_misreads_inside_amounts_are_repaired() {
        let text = "\
SHELL STATION
2026-09-23
RON95 1O.5O
T0TAL RM 1O.5O";
        let out = run(text, "MYR");
        assert_eq!(out.total_amount_minor, Some(1050));
        assert_eq!(out.suggested_category.as_deref(), Some("Fuel"));
    }

    #[test]
    fn currency_is_inferred_from_a_malaysian_address() {
        let text = "\
ALI MAMAK CORNER
Jalan Tun Razak, Kuala Lumpur
2026-09-24
TOTAL 8.40
CASH 10.00
CHANGE 1.60";
        let out = run(text, "SGD");
        assert_eq!(out.currency.as_deref(), Some("MYR"));
        assert_eq!(conf(&out, "currency"), 0.9);
        assert!(would_be_ready(&out));
    }

    #[test]
    fn ambiguous_date_follows_the_printed_currency_convention() {
        let text = "\
NASI KANDAR PELITA
03/04/2026
TOTAL RM 12.00
CASH 12.00";
        let out = run(text, "MYR");
        assert_eq!(out.date.as_deref(), Some("2026-04-03"));
        assert_eq!(conf(&out, "date"), 0.9);
    }

    #[test]
    fn ambiguous_date_without_any_currency_evidence_needs_review() {
        let text = "\
PELITA
03/04/2026
TOTAL 12.00
CASH 12.00";
        let out = run(text, "MYR");
        assert_eq!(out.date.as_deref(), Some("2026-04-03"));
        assert_eq!(conf(&out, "date"), 0.5);
        assert!(!would_be_ready(&out));
    }

    #[test]
    fn largest_amount_total_is_trusted_only_on_a_clear_page() {
        let text = "\
CITY HOTEL
Date: 2026-09-27
Accommodation
Tax MYR 18.00
TOTAL MYR 318.00";
        let clear = run(text, "MYR");
        assert_eq!(clear.total_amount_minor, Some(31800));
        assert_eq!(conf(&clear, "total"), 0.9);
        assert_eq!(clear.suggested_category.as_deref(), Some("Accommodation"));
        assert!(would_be_ready(&clear));
        // Without Tesseract's page confidence, nothing vouches for the digits.
        let unknown = run_with(text, "MYR", None);
        assert_eq!(conf(&unknown, "total"), 0.75);
        assert!(!would_be_ready(&unknown));
    }

    #[test]
    fn unreadable_scan_keeps_every_field_in_review() {
        let text = "\
KOPI KENANGAN CAFE SDN BHD
2026-09-27
SUBTOTAL RM12.50
SST 6% RM0.75
TOTAL RM13.25";
        let out = run_with(text, "MYR", Some(41.0));
        // Values are still filled in to save typing, but none is trusted.
        assert_eq!(out.total_amount_minor, Some(1325));
        assert!(out.confidence.values().all(|c| *c <= UNREADABLE_FIELD_CAP));
        assert!(!would_be_ready(&out));
    }

    #[test]
    fn unconfirmed_total_needs_review() {
        let text = "\
SOME SHOP
2026-09-27
Item A 30.00
TOTAL 12.00";
        let out = run(text, "MYR");
        // 12.00 is smaller than an item and nothing else agrees with it.
        assert_eq!(out.total_amount_minor, Some(1200));
        assert!(conf(&out, "total") < 0.9);
        assert!(!would_be_ready(&out));
    }

    #[test]
    fn sdn_bhd_is_not_read_as_bahraini_dinar() {
        let text = "\
MR DIY TRADING SDN BHD
2026-09-20
TOTAL 29.90";
        let out = run(text, "SGD");
        assert_eq!(out.currency.as_deref(), Some("MYR"));
        assert_eq!(out.total_amount_minor, Some(2990));
        let out = run("GULF TRADING\n2026-09-20\nTOTAL BHD 12.500", "MYR");
        assert_eq!(out.currency.as_deref(), Some("BHD"));
        assert_eq!(out.total_amount_minor, Some(12500));
    }

    #[test]
    fn foreign_dollar_sign_with_a_non_dollar_home_currency_needs_review() {
        let text = "\
JOE'S DINER
2026-09-20
TOTAL $15.00
CASH $15.00";
        let out = run(text, "MYR");
        assert_eq!(out.currency.as_deref(), Some("MYR"));
        assert_eq!(conf(&out, "currency"), 0.25);
        assert!(!would_be_ready(&out));
    }

    #[test]
    fn registration_numbers_are_not_read_as_tax() {
        let text = "\
ABC TRADING SDN BHD
SST REG NO: W10-1808-32000123
2026-09-27
TOTAL RM 10.00
CASH 10.00";
        let out = run(text, "MYR");
        assert_eq!(out.tax_amount_minor, None);
        assert_eq!(out.total_amount_minor, Some(1000));
    }
}
