use super::AppService;
use crate::{
    domain::{
        exchange::ExchangeRate,
        expense::{currency_exponent, Expense, ExpenseStatus},
        now,
        settings::Settings,
    },
    error::{AppError, Result},
    repository::{self, expense_repository as expenses},
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{io::Read, time::Duration};

/// Where published exchange rates come from. Tests substitute a fixed table.
pub trait RateSource: Send + Sync {
    /// The rate from `base` to `quote` for `date` (`YYYY-MM-DD`), or for the
    /// latest business day before it when no rate was published that day.
    fn fetch(&self, base: &str, quote: &str, date: &str) -> Result<ExchangeRate>;
}

/// The European Central Bank's daily reference rates, served free and
/// without an account by Frankfurter (https://frankfurter.dev). Only the two
/// currency codes and the date are sent.
pub struct FrankfurterRates;

const FRANKFURTER: &str = "https://api.frankfurter.dev/v1";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

impl RateSource for FrankfurterRates {
    fn fetch(&self, base: &str, quote: &str, date: &str) -> Result<ExchangeRate> {
        let unavailable = || {
            AppError::new(
                "RateUnavailable",
                format!("No published {base}→{quote} exchange rate is available for {date}."),
            )
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_| unavailable())?;
        let response = client
            .get(format!("{FRANKFURTER}/{date}"))
            .query(&[("base", base), ("symbols", quote)])
            .header("User-Agent", "Folio expense app")
            .send()
            .map_err(|_| {
                AppError::new(
                    "RateOffline",
                    "Exchange rates could not be downloaded. Folio will convert this receipt when you are online.",
                )
            })?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut body)
            .map_err(|_| unavailable())?;
        parse_frankfurter(&body, base, quote, date).ok_or_else(unavailable)
    }
}

/// Reads `{"base":"USD","date":"2026-09-25","rates":{"MYR":4.2105}}`.
fn parse_frankfurter(body: &[u8], base: &str, quote: &str, date: &str) -> Option<ExchangeRate> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    if value["base"].as_str()? != base {
        return None;
    }
    let rate_date = value["date"].as_str()?;
    // Published on or before the receipt date, and not from long before it.
    let requested = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let published = chrono::NaiveDate::parse_from_str(rate_date, "%Y-%m-%d").ok()?;
    if published > requested || requested - published > chrono::Duration::days(10) {
        return None;
    }
    let rate = value["rates"][quote].as_number()?.to_string();
    // A rate must be a plain positive decimal for the integer conversion.
    crate::domain::exchange::convert_minor(1, base, quote, &rate).ok()?;
    Some(ExchangeRate {
        base: base.into(),
        quote: quote.into(),
        rate_date: rate_date.into(),
        rate,
        source: "European Central Bank (via Frankfurter)".into(),
    })
}

fn cached(db: &Connection, base: &str, quote: &str, date: &str) -> Result<Option<ExchangeRate>> {
    Ok(db
        .query_row(
            "SELECT rate_date,rate,source FROM exchange_rates WHERE base=?1 AND quote=?2 AND requested_date=?3",
            params![base, quote, date],
            |r| {
                Ok(ExchangeRate {
                    base: base.into(),
                    quote: quote.into(),
                    rate_date: r.get(0)?,
                    rate: r.get(1)?,
                    source: r.get(2)?,
                })
            },
        )
        .optional()?)
}

/// The receipt-currency → home-currency conversion this expense needs, if
/// any: `(base, quote, date)`.
fn conversion_needed(e: &Expense, settings: &Settings) -> Option<(String, String, String)> {
    let (currency, total, _) = e.receipt_amounts();
    let base = currency?;
    let quote = settings.default_currency.clone();
    if !settings.currency_conversion_enabled
        || total.is_none()
        || base == quote
        || currency_exponent(&base).is_err()
    {
        return None;
    }
    Some((base, quote, e.occurred_at.clone()?))
}

impl AppService {
    /// Makes sure the rate is in the local cache, downloading it if needed.
    /// Runs without holding the database lock during the download. Failures
    /// (offline, unpublished currency) are reported but never lose data.
    pub(crate) fn prefetch_rate(&self, base: &str, quote: &str, date: &str) -> Result<()> {
        if base == quote || cached(&*self.conn()?, base, quote, date)?.is_some() {
            return Ok(());
        }
        let rate = self.rates.fetch(base, quote, date)?;
        self.conn()?.execute(
            "INSERT OR REPLACE INTO exchange_rates(base,quote,requested_date,rate_date,rate,source,fetched_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![base, quote, date, rate.rate_date, rate.rate, rate.source, now()],
        )?;
        Ok(())
    }

    /// Downloads the rate a set of receipt values will need, ignoring failures.
    pub(crate) fn prefetch_for(&self, currency: Option<&str>, date: Option<&str>) {
        let Ok(settings) = self.settings() else {
            return;
        };
        if let (Some(base), Some(date)) = (currency, date) {
            if settings.currency_conversion_enabled && base != settings.default_currency {
                let _ = self.prefetch_rate(base, &settings.default_currency, date);
            }
        }
    }

    /// Converts `e` from its receipt amounts using cached rates only (no
    /// network inside a transaction). When no rate is cached yet, the receipt
    /// amounts stay as they are and `convert_pending` finishes the job later.
    /// With conversion turned off, existing conversions are left untouched.
    pub(crate) fn convert_in_tx(
        db: &Connection,
        e: &mut Expense,
        settings: &Settings,
    ) -> Result<()> {
        if !settings.currency_conversion_enabled {
            return Ok(());
        }
        e.clear_conversion();
        if let Some((base, quote, date)) = conversion_needed(e, settings) {
            if let Some(rate) = cached(db, &base, &quote, &date)? {
                e.apply_conversion(&rate)?;
            }
        }
        Ok(())
    }

    /// Re-derives the claimed amounts after the receipt amounts or date of `e`
    /// may have changed from `before` (an edit or a new extraction). An
    /// unchanged, already-converted receipt keeps its conversion as is.
    pub(crate) fn reconvert_in_tx(
        db: &Connection,
        e: &mut Expense,
        before: &Expense,
        settings: &Settings,
    ) -> Result<()> {
        let unchanged =
            e.receipt_amounts() == before.receipt_amounts() && e.occurred_at == before.occurred_at;
        if unchanged && before.original_currency.is_some() {
            e.currency.clone_from(&before.currency);
            e.total_amount_minor = before.total_amount_minor;
            e.tax_amount_minor = before.tax_amount_minor;
            e.original_currency.clone_from(&before.original_currency);
            e.original_total_amount_minor = before.original_total_amount_minor;
            e.original_tax_amount_minor = before.original_tax_amount_minor;
            e.exchange_rate.clone_from(&before.exchange_rate);
            e.exchange_rate_date.clone_from(&before.exchange_rate_date);
            return Ok(());
        }
        Self::convert_in_tx(db, e, settings)
    }

    /// Converts every expense still waiting for an exchange rate (e.g. imported
    /// while offline). Returns how many were converted.
    pub fn convert_pending(&self) -> Result<u32> {
        let settings = self.settings()?;
        if !settings.currency_conversion_enabled {
            return Ok(0);
        }
        let waiting: Vec<Expense> = expenses::all(&*self.conn()?)?
            .into_iter()
            .filter(|e| {
                e.original_currency.is_none()
                    && !matches!(
                        e.status,
                        ExpenseStatus::Submitted
                            | ExpenseStatus::Archived
                            | ExpenseStatus::Extracting
                    )
                    && conversion_needed(e, &settings).is_some()
            })
            .take(50)
            .collect();
        let mut converted = 0;
        for waiting in waiting {
            let Some((base, quote, date)) = conversion_needed(&waiting, &settings) else {
                continue;
            };
            if self.prefetch_rate(&base, &quote, &date).is_err() {
                continue;
            }
            let mut db = self.conn()?;
            let tx = db.transaction()?;
            let mut e = expenses::get(&tx, &waiting.id)?;
            // Skip anything edited meanwhile, or in a claim in another currency.
            if e.version != waiting.version || e.original_currency.is_some() {
                continue;
            }
            if let Some(ref claim_id) = e.claim_id {
                let claim_currency: String = tx.query_row(
                    "SELECT currency FROM expense_claims WHERE id=?1",
                    [claim_id],
                    |r| r.get(0),
                )?;
                if claim_currency != settings.default_currency {
                    continue;
                }
            }
            Self::convert_in_tx(&tx, &mut e, &settings)?;
            if e.original_currency.is_none() {
                continue;
            }
            e.updated_at = now();
            expenses::save(&tx, &e)?;
            if let Some(ref claim_id) = e.claim_id {
                tx.execute(
                    "UPDATE expense_claims SET version=version+1,updated_at=?1 WHERE id=?2",
                    params![e.updated_at, claim_id],
                )?;
            }
            repository::audit(
                &tx,
                "expense.converted",
                &e.id,
                serde_json::json!({"from":e.original_currency,"to":e.currency,"rate":e.exchange_rate,"rateDate":e.exchange_rate_date}),
            )?;
            tx.commit()?;
            converted += 1;
        }
        Ok(converted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_frankfurter_response() {
        let body = br#"{"amount":1.0,"base":"USD","date":"2026-09-25","rates":{"MYR":4.2105}}"#;
        let rate = parse_frankfurter(body, "USD", "MYR", "2026-09-27").unwrap();
        assert_eq!(rate.rate, "4.2105");
        assert_eq!(rate.rate_date, "2026-09-25");
    }

    #[test]
    fn rejects_mismatched_stale_or_future_rates() {
        let body = br#"{"amount":1.0,"base":"USD","date":"2026-09-25","rates":{"MYR":4.2105}}"#;
        assert!(parse_frankfurter(body, "EUR", "MYR", "2026-09-27").is_none());
        assert!(parse_frankfurter(body, "USD", "SGD", "2026-09-27").is_none());
        assert!(parse_frankfurter(body, "USD", "MYR", "2026-09-24").is_none());
        assert!(parse_frankfurter(body, "USD", "MYR", "2026-10-30").is_none());
        assert!(parse_frankfurter(b"not json", "USD", "MYR", "2026-09-27").is_none());
        let negative = br#"{"base":"USD","date":"2026-09-25","rates":{"MYR":-4.2}}"#;
        assert!(parse_frankfurter(negative, "USD", "MYR", "2026-09-27").is_none());
    }
}
