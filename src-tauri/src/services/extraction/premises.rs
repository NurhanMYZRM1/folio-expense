//! Premises: where the purchase was made, read from the address a receipt
//! prints under the business name.
//!
//! Offline OCR finds the address block at the top of the receipt; online AI
//! reports it directly. Both go through [`normalize`], which cross-checks a
//! Malaysian postcode against the state printed beside it (the same way the
//! total is cross-checked against the receipt's own arithmetic) and fills in
//! a missing state or country.

use regex::Regex;
use std::sync::LazyLock;

/// Postcode ranges by state (Pos Malaysia allocation).
const MY_POSTCODE_STATES: &[(u32, u32, &str)] = &[
    (1000, 2999, "Perlis"),
    (5000, 9999, "Kedah"),
    (10000, 14999, "Pulau Pinang"),
    (15000, 18999, "Kelantan"),
    (20000, 24999, "Terengganu"),
    (25000, 28999, "Pahang"),
    (30000, 36999, "Perak"),
    (39000, 39999, "Pahang"),
    (40000, 48999, "Selangor"),
    (49000, 49999, "Pahang"),
    (50000, 60999, "Kuala Lumpur"),
    (62000, 62999, "Putrajaya"),
    (63000, 68999, "Selangor"),
    (69000, 69999, "Pahang"),
    (70000, 73999, "Negeri Sembilan"),
    (75000, 78999, "Melaka"),
    (79000, 86999, "Johor"),
    (87000, 87999, "Labuan"),
    (88000, 91999, "Sabah"),
    (93000, 98999, "Sarawak"),
];

/// Printed state names (and major towns) and the state they place an address
/// in. Checked in order; the first match wins.
const MY_PLACES: &[(&str, &str)] = &[
    (r"KUALA LUMPUR|\bW\.?\s*P\.?\s*KL\b", "Kuala Lumpur"),
    (r"PUTRAJAYA", "Putrajaya"),
    (r"\bLABUAN\b", "Labuan"),
    (
        r"SELANGOR|PETALING JAYA|SHAH ALAM|SUBANG JAYA|\bKLANG\b|\bKAJANG\b|CYBERJAYA|PUCHONG|RAWANG|SEPANG",
        "Selangor",
    ),
    (
        r"PULAU PINANG|\bP\.?\s*PINANG\b|PENANG|GEORGE ?TOWN|BUTTERWORTH",
        "Pulau Pinang",
    ),
    (r"\bJOHOR|\bJB\b", "Johor"),
    (r"\bPERAK\b|\bIPOH\b", "Perak"),
    (r"\bKEDAH\b|ALOR SETAR", "Kedah"),
    (r"\bPERLIS\b|KANGAR", "Perlis"),
    (r"KELANTAN|KOTA BHARU", "Kelantan"),
    (r"TERENGGANU", "Terengganu"),
    (r"PAHANG|KUANTAN|GENTING|CAMERON", "Pahang"),
    (
        r"NEGERI SEMBILAN|\bN\.?\s*SEMBILAN\b|SEREMBAN",
        "Negeri Sembilan",
    ),
    (r"MELAKA|MALACCA", "Melaka"),
    (r"\bSABAH\b|KOTA KINABALU|SANDAKAN|TAWAU", "Sabah"),
    (r"SARAWAK|KUCHING|\bMIRI\b|SIBU|BINTULU", "Sarawak"),
];

static MY_PLACE_RES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    MY_PLACES
        .iter()
        .map(|(p, s)| (Regex::new(&format!("(?i){p}")).expect("place pattern"), *s))
        .collect()
});
static MY_POSTCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^\d-])(\d{5})(?:$|[^\d-])").expect("postcode"));
static SG_POSTCODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)SINGAPORE\s*\(?(\d{6})\)?|\bS\s*\(?(\d{6})\)?").expect("sg postcode")
});
static MALAYSIA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bMALAYSIA\b").expect("malaysia"));
static SINGAPORE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bSINGAPORE\b").expect("singapore"));

/// Words that make a line part of a street address.
static ADDRESS_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b\d{5,6}\b|\bJALAN\b|\bJLN\.?\b|\bLORONG\b|\bLRG\b|\bPERSIARAN\b|\bLEBUH(RAYA)?\b|\bTAMAN\b|\bTMN\b|\bBANDAR\b|\bBDR\b|\bKAMPUNG\b|\bKG\.?\s|\bLOT\b|\bNO\.?\s*\d|\bBLK\b|\bBLOCK\b|\bSTREET\b|\bST\.\s|\bROAD\b|\bRD\.?\b|\bAVENUE\b|\bLANE\b|\bDRIVE\b|\bFLOOR\b|\bLEVEL\b|\bLVL\b|\bTINGKAT\b|\bARAS\b|\bWISMA\b|\bMENARA\b|\bPLAZA\b|\bMALL\b|\bCENTRE\b|\bCENTER\b|\bCOMPLEX\b|\bKOMPLEKS\b|\bSQUARE\b|\bSEKSYEN\b|\bSECTION\b|\bSS\s?\d|\bPJS\b|\bUSJ\b|#\d{1,2}-\d{1,4}|\bMALAYSIA\b|\bSINGAPORE\b",
    )
    .expect("address line")
});
/// Lines near an address that are not part of it.
static NOT_ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bTEL\b|\bTELEFON\b|\bPHONE\b|\bFAX\b|\bH/?P\b|\bMOBILE\b|\+6\d|\b0\d{1,2}-\s?\d{3,4}\s?\d{3,4}\b|\d{7,}|\bSST\b|\bGST\b|\bREG\b|CO\.?\s*NO|\bROC\b|\bBRN\b|@|WWW\.|\.COM\b|\.MY\b|\bEMAIL\b")
        .expect("not address")
});
/// Where the receipt header ends: the first date, time or priced line.
static HEADER_END: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b\d{1,2}[-/.]\d{1,2}[-/.]\d{2,4}\b|\b\d{4}[-/.]\d{1,2}[-/.]\d{1,2}\b|\b\d{1,2}:\d{2}\b|\d[.,]\d{2}\b|\bDATE\b|\bTARIKH\b|\bCASHIER\b|\bINVOICE\s*(NO|#)")
        .expect("header end")
});

fn postcode_state(code: u32) -> Option<&'static str> {
    MY_POSTCODE_STATES
        .iter()
        .find(|(lo, hi, _)| (*lo..=*hi).contains(&code))
        .map(|(_, _, state)| *state)
}

fn printed_state(text: &str) -> Option<&'static str> {
    MY_PLACE_RES
        .iter()
        .find(|(re, _)| re.is_match(text))
        .map(|(_, state)| *state)
}

/// Trims OCR specks and stray separators from an address part.
fn tidy(part: &str) -> String {
    let edge = |c: char| !(c.is_alphanumeric() || "().#&'".contains(c));
    part.trim_matches(edge)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// How far an address can be trusted, from the evidence printed in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Evidence {
    /// A postcode and a printed state/country that agree with it.
    Confirmed,
    /// A postcode, but nothing printed to confirm or contradict it.
    Postcode,
    /// Address words only.
    Unchecked,
    /// The postcode belongs to a different state than the one printed.
    Contradicted,
}

impl Evidence {
    pub fn confidence(self) -> f64 {
        match self {
            Self::Confirmed => 0.95,
            Self::Postcode => 0.85,
            Self::Unchecked => 0.7,
            Self::Contradicted => 0.45,
        }
    }
}

/// Tidies an address into one comma-separated line, adds the state a
/// Malaysian postcode implies (and the country) when they are not printed,
/// and reports how well the parts agree. `home_currency` decides whether a
/// bare five-digit postcode is Malaysian.
pub fn normalize(address: &str, currency: Option<&str>) -> Option<(String, Evidence)> {
    let parts: Vec<String> = address
        .split([',', '\n'])
        .map(tidy)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut line = parts.join(", ");
    if line.chars().count() > 300 {
        line = line.chars().take(300).collect();
    }
    let in_singapore = SINGAPORE.is_match(&line);
    if in_singapore {
        let has_code = SG_POSTCODE.is_match(&line);
        return Some((
            line,
            if has_code {
                Evidence::Confirmed
            } else {
                Evidence::Unchecked
            },
        ));
    }
    let malaysian = MALAYSIA.is_match(&line) || currency == Some("MYR") || currency.is_none();
    let postcode = MY_POSTCODE
        .captures_iter(&line)
        .filter_map(|c| c[1].parse::<u32>().ok())
        .find_map(postcode_state);
    let evidence = match (malaysian, postcode, printed_state(&line)) {
        (true, Some(from_code), Some(printed)) if from_code == printed => Evidence::Confirmed,
        (true, Some(_), Some(_)) => Evidence::Contradicted,
        (true, Some(from_code), None) => {
            line.push_str(", ");
            line.push_str(from_code);
            Evidence::Postcode
        }
        _ => Evidence::Unchecked,
    };
    if malaysian && postcode.is_some() && !MALAYSIA.is_match(&line) {
        line.push_str(", Malaysia");
    }
    Some((line, evidence))
}

/// Finds the premises address in OCR text: the address lines printed in the
/// receipt header (above the first date or priced line), excluding the
/// business-name line, phone and registration lines. A line naming only a
/// town or state (`Kuala Terengganu`) counts too. `clear_page` is whether OCR
/// was confident about the whole page: when it was not, a digit in the
/// postcode or street number may be misread even if the parts agree.
pub fn detect(
    lines: &[&str],
    merchant: Option<&str>,
    currency: &str,
    clear_page: bool,
) -> Option<(String, f64)> {
    let header: Vec<&str> = lines
        .iter()
        .take(14)
        .take_while(|line| !HEADER_END.is_match(line))
        .copied()
        .collect();
    let merchant = merchant.map(tidy);
    let address: Vec<String> = header
        .iter()
        .map(|line| tidy(line))
        .filter(|line| {
            (ADDRESS_LINE.is_match(line) || printed_state(line).is_some())
                && !NOT_ADDRESS.is_match(line)
                && merchant.as_deref() != Some(line.as_str())
        })
        .collect();
    if address.is_empty() {
        return None;
    }
    let (line, evidence) = normalize(&address.join(", "), Some(currency))?;
    // A single street-word line (`PETRONAS JALAN DUTA`) is weak evidence.
    let confidence = if address.len() == 1 && evidence == Evidence::Unchecked {
        0.55
    } else {
        evidence.confidence()
    };
    Some((
        line,
        if clear_page {
            confidence
        } else {
            confidence.min(0.8)
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_text(text: &str, merchant: &str, currency: &str) -> Option<(String, f64)> {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        detect(&lines, Some(merchant), currency, true)
    }

    #[test]
    fn kuala_lumpur_address_is_confirmed_by_its_postcode() {
        let text = "\
RESTORAN NASI KANDAR PELITA SDN BHD
(Co. No. 123456-X)
No. 149, Jalan Ampang
50450 Kuala Lumpur, Malaysia
Tel: 03-2162 6566
TAX INVOICE
Date: 27/09/2026
TOTAL RM 25.45";
        let (premises, confidence) =
            detect_text(text, "RESTORAN NASI KANDAR PELITA SDN BHD", "MYR").unwrap();
        assert_eq!(
            premises,
            "No. 149, Jalan Ampang, 50450 Kuala Lumpur, Malaysia"
        );
        assert_eq!(confidence, 0.95);
    }

    #[test]
    fn mall_outlet_address_adds_the_country() {
        let text = "\
MR. D.I.Y. (M) SDN BHD
Lot G-23, Sunway Pyramid
3, Jalan PJS 11/15, Bandar Sunway
47500 Petaling Jaya, Selangor
28-09-2026 19:05
TOTAL (INCL SST) 54.30";
        let (premises, confidence) = detect_text(text, "MR. D.I.Y. (M) SDN BHD", "MYR").unwrap();
        assert_eq!(
            premises,
            "Lot G-23, Sunway Pyramid, 3, Jalan PJS 11/15, Bandar Sunway, 47500 Petaling Jaya, Selangor, Malaysia"
        );
        assert_eq!(confidence, 0.95);
    }

    #[test]
    fn missing_state_is_filled_in_from_the_postcode() {
        let (premises, confidence) = detect_text(
            "KEDAI RUNCIT AMAN\nNo 5, Jalan Besar\n20000 Kuala Terengganu\n27/09/2026",
            "KEDAI RUNCIT AMAN",
            "MYR",
        )
        .unwrap();
        // `Kuala Terengganu` names the state, so nothing is added but the country.
        assert_eq!(
            premises,
            "No 5, Jalan Besar, 20000 Kuala Terengganu, Malaysia"
        );
        assert_eq!(confidence, 0.95);
        let (premises, confidence) = detect_text(
            "KOPI HOUSE\nLot 3, Jalan Kenari 5\n47100 Puchong Perdana\n27/09/2026",
            "KOPI HOUSE",
            "MYR",
        )
        .unwrap();
        assert_eq!(confidence, 0.95, "{premises}");
        let (premises, confidence) = detect_text(
            "KOPI HOUSE\nLot 3, Jalan Kenari 5\n47100 Bandar Puteri\n27/09/2026",
            "KOPI HOUSE",
            "MYR",
        )
        .unwrap();
        assert_eq!(
            premises,
            "Lot 3, Jalan Kenari 5, 47100 Bandar Puteri, Selangor, Malaysia"
        );
        assert_eq!(confidence, 0.85);
    }

    #[test]
    fn postcode_from_another_state_is_flagged() {
        let (premises, confidence) = detect_text(
            "ABC TRADING SDN BHD\nNo 1, Jalan Mawar\n50450 Johor Bahru\n27/09/2026",
            "ABC TRADING SDN BHD",
            "MYR",
        )
        .unwrap();
        assert_eq!(premises, "No 1, Jalan Mawar, 50450 Johor Bahru, Malaysia");
        assert!(confidence < 0.5);
    }

    #[test]
    fn singapore_address_with_postal_code() {
        let (premises, confidence) = detect_text(
            "YA KUN KAYA TOAST PTE LTD\n18 China Street #01-01\nFar East Square, Singapore 049560\n30 Sep 2026 09:10",
            "YA KUN KAYA TOAST PTE LTD",
            "SGD",
        )
        .unwrap();
        assert_eq!(
            premises,
            "18 China Street #01-01, Far East Square, Singapore 049560"
        );
        assert_eq!(confidence, 0.95);
    }

    #[test]
    fn phone_registration_and_receipt_body_are_not_the_address() {
        let text = "\
GOLDEN CAFE
SST REG NO: W10-1808-32000123
Tel: 03-7956 1234
27/09/2026
1 x Nasi Lemak, Jalan Special 8.00
TOTAL 8.00";
        assert_eq!(detect_text(text, "GOLDEN CAFE", "MYR"), None);
    }

    #[test]
    fn a_town_line_alone_is_a_premises_to_verify() {
        // Real local-OCR text of a shadowed iPhone photo.
        let (premises, confidence) = detect_text(
            "KEDAI RUNCIT AMAN\nKuala Terengganu\nDate : 27/09/2026\nTotal 283.00",
            "KEDAI RUNCIT AMAN",
            "MYR",
        )
        .unwrap();
        assert_eq!(premises, "Kuala Terengganu");
        assert!(confidence < 0.6);
    }

    #[test]
    fn an_unclear_page_keeps_even_a_consistent_address_in_verify() {
        let lines = [
            "CAFE",
            "No. 140, Jalan Ampang",
            "50450 Kuala Lumpur",
            "27/09/2026",
        ];
        let (_, clear) = detect(&lines, Some("CAFE"), "MYR", true).unwrap();
        let (_, unclear) = detect(&lines, Some("CAFE"), "MYR", false).unwrap();
        assert_eq!(clear, 0.95);
        assert!(unclear < 0.85);
    }

    #[test]
    fn a_lone_street_word_is_weak_evidence() {
        let (premises, confidence) = detect_text(
            "PETRONAS JALAN DUTA\nKAMAL ENTERPRISE\n03-10-2026",
            "KAMAL ENTERPRISE",
            "MYR",
        )
        .unwrap();
        assert_eq!(premises, "PETRONAS JALAN DUTA");
        assert!(confidence < 0.6);
    }

    #[test]
    fn ai_reported_address_is_normalized_the_same_way() {
        let (line, evidence) =
            normalize("  3, Jalan PJS 11/15,\n47500 Petaling Jaya ", Some("MYR")).unwrap();
        assert_eq!(line, "3, Jalan PJS 11/15, 47500 Petaling Jaya, Malaysia");
        assert_eq!(evidence, Evidence::Confirmed);
        assert_eq!(normalize(" , ", Some("MYR")), None);
    }
}
