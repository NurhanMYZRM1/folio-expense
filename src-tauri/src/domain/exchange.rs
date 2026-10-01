use super::expense::{currency_exponent, Expense, MAX_MONEY};
use crate::error::{AppError, Result};

/// A published exchange rate: `rate` units of `quote` per one unit of `base`,
/// kept as the exact decimal text the source published (e.g. `"4.2105"`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeRate {
    pub base: String,
    pub quote: String,
    /// The business day the rate was published for (on or before the receipt date).
    pub rate_date: String,
    pub rate: String,
    pub source: String,
}

/// Splits a positive decimal like `"4.2105"` into `(42105, 4)` so money is
/// converted with integers only.
fn decimal_parts(rate: &str) -> Result<(i128, u32)> {
    let invalid = || AppError::invalid("The exchange rate is not a valid number.");
    let (whole, fraction) = rate.trim().split_once('.').unwrap_or((rate.trim(), ""));
    if whole.is_empty()
        || fraction.len() > 12
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let mantissa: i128 = format!("{whole}{fraction}")
        .parse()
        .map_err(|_| invalid())?;
    if mantissa <= 0 {
        return Err(invalid());
    }
    Ok((mantissa, fraction.len() as u32))
}

/// Converts an amount in minor units (cents, sen…) of `from` to minor units of
/// `to`, rounding half up to the nearest minor unit of `to`.
pub fn convert_minor(amount: i64, from: &str, to: &str, rate: &str) -> Result<i64> {
    let (mantissa, scale) = decimal_parts(rate)?;
    let from_exp = currency_exponent(from)?;
    let to_exp = currency_exponent(to)?;
    // amount / 10^from_exp × mantissa / 10^scale × 10^to_exp
    let numerator = i128::from(amount) * mantissa * 10_i128.pow(to_exp);
    let denominator = 10_i128.pow(from_exp + scale);
    let converted = (numerator + denominator / 2) / denominator;
    i64::try_from(converted)
        .ok()
        .filter(|v| (0..=MAX_MONEY).contains(v))
        .ok_or_else(|| AppError::invalid("The converted amount is too large."))
}

impl Expense {
    /// Currency, total and tax exactly as printed on the receipt.
    pub fn receipt_amounts(&self) -> (Option<String>, Option<i64>, Option<i64>) {
        match self.original_currency {
            Some(ref currency) => (
                Some(currency.clone()),
                self.original_total_amount_minor,
                self.original_tax_amount_minor,
            ),
            None => (
                self.currency.clone(),
                self.total_amount_minor,
                self.tax_amount_minor,
            ),
        }
    }
    /// Puts the receipt's own amounts back in the main fields, dropping any conversion.
    pub fn clear_conversion(&mut self) {
        let (currency, total, tax) = self.receipt_amounts();
        self.currency = currency;
        self.total_amount_minor = total;
        self.tax_amount_minor = tax;
        self.original_currency = None;
        self.original_total_amount_minor = None;
        self.original_tax_amount_minor = None;
        self.exchange_rate = None;
        self.exchange_rate_date = None;
    }
    /// Converts the receipt amounts with `rate`, keeping them as the original.
    pub fn apply_conversion(&mut self, rate: &ExchangeRate) -> Result<()> {
        self.clear_conversion();
        if self.currency.as_deref() != Some(rate.base.as_str()) {
            return Err(AppError::invalid(
                "The exchange rate does not match the receipt.",
            ));
        }
        let convert = |amount: Option<i64>| {
            amount
                .map(|a| convert_minor(a, &rate.base, &rate.quote, &rate.rate))
                .transpose()
        };
        let total = convert(self.total_amount_minor)?;
        let tax = convert(self.tax_amount_minor)?;
        self.original_currency = self.currency.replace(rate.quote.clone());
        self.original_total_amount_minor = std::mem::replace(&mut self.total_amount_minor, total);
        self.original_tax_amount_minor = std::mem::replace(&mut self.tax_amount_minor, tax);
        self.exchange_rate = Some(rate.rate.clone());
        self.exchange_rate_date = Some(rate.rate_date.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_with_integer_math_and_half_up_rounding() {
        // USD 45.00 at 4.2105 = MYR 189.4725 → 189.47
        assert_eq!(convert_minor(4500, "USD", "MYR", "4.2105").unwrap(), 18947);
        // USD 10.01 at 4.2105 = 42.147105 → 42.15
        assert_eq!(convert_minor(1001, "USD", "MYR", "4.2105").unwrap(), 4215);
        // Exactly half a sen rounds up: 0.05 × 0.1 = 0.005 → 0.01
        assert_eq!(convert_minor(5, "USD", "MYR", "0.1").unwrap(), 1);
    }

    #[test]
    fn handles_currencies_with_different_decimal_places() {
        // JPY 1,500 (no decimals) at 0.029 = MYR 43.50
        assert_eq!(convert_minor(1500, "JPY", "MYR", "0.029").unwrap(), 4350);
        // MYR 10.00 at 33.42 = JPY 334.2 → 334
        assert_eq!(convert_minor(1000, "MYR", "JPY", "33.42").unwrap(), 334);
        // Whole-number rate text is accepted.
        assert_eq!(convert_minor(100, "EUR", "MYR", "5").unwrap(), 500);
    }

    #[test]
    fn rejects_bad_rates_and_overflow() {
        for rate in ["", "-4.2", "0", "abc", "4.2.1", "1e5", ".5"] {
            assert!(convert_minor(100, "USD", "MYR", rate).is_err(), "{rate}");
        }
        assert!(convert_minor(MAX_MONEY, "USD", "IDR", "16000").is_err());
    }
}
