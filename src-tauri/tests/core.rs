use folio::{
    domain::{
        claim::ClaimStatus,
        exchange::ExchangeRate,
        expense::{currency_exponent, parse_money, ExpenseEdit, ExpenseStatus},
        extraction::{Extraction, FieldSource},
    },
    error::{AppError, Result},
    security::secrets::SecretStore,
    services::{exchange_rates::RateSource, AppService},
    storage::paths::AppPaths,
};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tempfile::TempDir;
use zeroize::Zeroizing;
#[derive(Default)]
struct MemorySecrets(Mutex<Option<String>>);
impl SecretStore for MemorySecrets {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        Ok(self.0.lock().unwrap().clone().map(Zeroizing::new))
    }
    fn set(&self, s: &str) -> Result<()> {
        *self.0.lock().unwrap() = Some(s.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
/// A fixed table of exchange rates standing in for the internet.
#[derive(Default)]
struct FixedRates {
    offline: AtomicBool,
    downloads: AtomicUsize,
}
impl RateSource for FixedRates {
    fn fetch(&self, base: &str, quote: &str, date: &str) -> Result<ExchangeRate> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(AppError::new("RateOffline", "offline"));
        }
        self.downloads.fetch_add(1, Ordering::SeqCst);
        let rate = match (base, quote) {
            ("USD", "MYR") => "4.2105",
            ("JPY", "MYR") => "0.029",
            _ => return Err(AppError::new("RateUnavailable", "no rate")),
        };
        Ok(ExchangeRate {
            base: base.into(),
            quote: quote.into(),
            rate_date: date.into(),
            rate: rate.into(),
            source: "test".into(),
        })
    }
}
fn workspace_with_rates() -> (TempDir, AppService, Arc<FixedRates>) {
    let t = tempfile::tempdir().unwrap();
    let rates = Arc::new(FixedRates::default());
    let s = AppService::open(t.path().join("app"), Arc::new(MemorySecrets::default()))
        .unwrap()
        .with_rate_source(rates.clone());
    (t, s, rates)
}
fn workspace() -> (TempDir, AppService) {
    let (t, s, _) = workspace_with_rates();
    (t, s)
}
fn receipt(t: &TempDir) -> std::path::PathBuf {
    let path = t.path().join("receipt.png");
    let image = image::RgbImage::from_pixel(40, 70, image::Rgb([255, 255, 255]));
    image.save(&path).unwrap();
    path
}
fn import(t: &TempDir, s: &AppService) -> String {
    s.import_receipt(&receipt(t)).unwrap()
}
/// Imports a distinct receipt image (a different shade per `n`).
fn import_another(t: &TempDir, s: &AppService, n: u8) -> String {
    let path = t.path().join(format!("receipt-{n}.png"));
    image::RgbImage::from_pixel(40, 70, image::Rgb([n, 200, 200]))
        .save(&path)
        .unwrap();
    s.import_receipt(&path).unwrap()
}
fn usd_result() -> Extraction {
    serde_json::from_value(serde_json::json!({"merchantName":"Joe's Diner","date":"2026-09-25","totalAmountMinor":4500,"taxAmountMinor":250,"currency":"USD","suggestedCategory":"Meals","confidence":{"merchantName":0.98,"date":0.99,"total":0.99,"tax":0.82,"currency":0.99,"category":0.9}})).unwrap()
}
/// Imports a receipt and completes its extraction with `result`.
fn extracted(t: &TempDir, s: &AppService, n: u8, result: Extraction) -> String {
    let id = import_another(t, s, n);
    let j = extraction_job(s);
    assert_eq!(j.job.entity_id, id);
    s.complete_extraction(&j.job.id, &j.token, result, FieldSource::OnlineAi, None)
        .unwrap();
    // Leave the thumbnail job out of later `extraction_job` calls.
    while let Some(j) = s.take_job().unwrap() {
        s.fail_job(&j.job.id, &j.token, "not part of this test")
            .unwrap();
    }
    id
}
fn extraction_job(s: &AppService) -> folio::domain::extraction::JobLease {
    loop {
        let j = s.take_job().unwrap().unwrap();
        if j.job.job_type == "extract_receipt" {
            return j;
        }
        s.fail_job(
            &j.job.id,
            &j.token,
            "Thumbnail not part of this service test.",
        )
        .unwrap();
    }
}
fn edit(s: &AppService, id: &str) -> ExpenseEdit {
    let e = s.expense(id).unwrap();
    ExpenseEdit {
        id: id.into(),
        version: e.version,
        occurred_at: Some("2026-09-27".into()),
        merchant_name: Some("Client lunch".into()),
        premises: e.premises.clone(),
        total_amount_minor: Some(8450),
        tax_amount_minor: Some(478),
        currency: Some("MYR".into()),
        category: "Meals".into(),
        description: "Client meeting".into(),
        mark_ready: true,
    }
}
fn result() -> Extraction {
    serde_json::from_value(serde_json::json!({"merchantName":"OCR merchant","date":"2026-09-27","totalAmountMinor":5000,"taxAmountMinor":250,"currency":"MYR","suggestedCategory":"Meals","confidence":{"merchantName":0.98,"date":0.99,"total":0.99,"tax":0.82,"currency":0.99,"category":0.9}})).unwrap()
}
#[test]
fn money_is_exact_and_currency_aware() {
    assert_eq!(parse_money("42.50", 2).unwrap(), 4250);
    assert_eq!(parse_money("0.29", 2).unwrap(), 29);
    assert_eq!(parse_money("1.234", 3).unwrap(), 1234);
    assert_eq!(parse_money("120", 0).unwrap(), 120);
    assert!(parse_money("12.99", 0).is_err());
    assert!(parse_money("-1", 2).is_err());
    assert!(parse_money("9e12", 2).is_err());
    assert!(parse_money("10000000000", 2).is_err());
    // Thousands separators and decimal commas (EUR-style receipts).
    assert_eq!(parse_money("1,234.50", 2).unwrap(), 123450);
    assert_eq!(parse_money("1,234", 2).unwrap(), 123400);
    assert_eq!(parse_money("12,50", 2).unwrap(), 1250);
    assert_eq!(parse_money("12,5", 2).unwrap(), 1250);
    assert!(parse_money("12,50", 0).is_err());
    assert!(parse_money("1,23,4", 2).is_err());
    assert!(parse_money("12,345,67", 2).is_err());
    assert_eq!(currency_exponent("JPY").unwrap(), 0);
}
#[test]
fn migrations_are_idempotent_and_constraints_work() {
    let t = tempfile::tempdir().unwrap();
    let mut db = folio::db::open(&t.path().join("test.sqlite")).unwrap();
    folio::db::migrate(&mut db).unwrap();
    let count: i32 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 9);
    let version: i32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    // The migrations must stamp exactly the version this build says it supports.
    assert_eq!(version, folio::db::SCHEMA_VERSION);
    assert!(db.execute("INSERT INTO expenses(id,status,created_at,updated_at) VALUES('x','nonsense','now','now')",[]).is_err());
    db.execute_batch("PRAGMA user_version=999").unwrap();
    assert!(folio::db::migrate(&mut db).is_err());
}
/// Writes a database stamped with a schema version this build doesn't know,
/// holding one row that must survive untouched.
fn future_database(root: &std::path::Path) -> std::path::PathBuf {
    let dir = root.join("database");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("expenses.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(&format!(
        "CREATE TABLE future(x TEXT); INSERT INTO future VALUES('keep me'); PRAGMA user_version={};",
        folio::db::SCHEMA_VERSION + 1
    ))
    .unwrap();
    path
}
#[test]
fn startup_refuses_newer_database_without_modifying_it() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    let path = future_database(&root);
    let before = fs::read(&path).unwrap();

    let error = AppService::open(root.clone(), Arc::new(MemorySecrets::default()))
        .err()
        .expect("a newer database must be refused");
    assert_eq!(error.code, "DatabaseVersion");
    let future = format!("version {}", folio::db::SCHEMA_VERSION + 1);
    assert!(error.message.contains(&future), "{}", error.message);

    // Refusing must not write anything: same bytes, no journal files.
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!root.join("database/expenses.sqlite-wal").exists());
    assert!(!root.join("database/expenses.sqlite-shm").exists());

    // Opening it again gives the same answer, not a different failure.
    let again = AppService::open(root, Arc::new(MemorySecrets::default()))
        .err()
        .expect("still refused");
    assert_eq!(again.code, "DatabaseVersion");
}
#[test]
fn startup_after_moving_newer_database_aside_creates_a_fresh_one() {
    // The recovery the user performed: rename the data folder, start again.
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    future_database(&root);
    fs::rename(&root, t.path().join("app-backup")).unwrap();

    AppService::open(root.clone(), Arc::new(MemorySecrets::default())).unwrap();
    let db = rusqlite::Connection::open(root.join("database/expenses.sqlite")).unwrap();
    let version: i32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, folio::db::SCHEMA_VERSION);
    let kept: String =
        rusqlite::Connection::open(t.path().join("app-backup/database/expenses.sqlite"))
            .unwrap()
            .query_row("SELECT x FROM future", [], |r| r.get(0))
            .unwrap();
    assert_eq!(kept, "keep me");
}
#[test]
fn exact_duplicate_does_not_create_expense_or_change_original() {
    let (t, s) = workspace();
    let path = receipt(&t);
    let original = fs::read(&path).unwrap();
    let first = s.import_receipt(&path).unwrap();
    let second = s.import_receipt(&path).unwrap_err();
    assert_eq!(second.code, "DuplicateReceipt");
    assert_eq!(second.existing_expense_id.as_deref(), Some(first.as_str()));
    assert_eq!(s.expenses().unwrap().len(), 1);
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(fs::read_dir(s.paths.root.join("cache")).unwrap().count(), 0);
}
#[test]
fn rejects_disguised_and_oversized_files() {
    let (t, s) = workspace();
    let path = t.path().join("fake.png");
    fs::write(&path, b"not a PNG").unwrap();
    assert_eq!(s.import_receipt(&path).unwrap_err().code, "FileUnsupported");
    let large = t.path().join("large.pdf");
    fs::File::create(&large)
        .unwrap()
        .set_len(26 * 1024 * 1024)
        .unwrap();
    assert_eq!(s.import_receipt(&large).unwrap_err().code, "FileTooLarge");
    assert!(s.expenses().unwrap().is_empty());
}
#[test]
fn internal_paths_reject_traversal_and_absolute_paths() {
    let (_t, s) = workspace();
    assert!(s.paths.resolve("../other").is_err());
    assert!(s.paths.resolve("/etc/passwd").is_err());
    assert!(s.paths.resolve("receipts/../../outside").is_err());
    assert!(s.paths.resolve("receipts\\evil").is_err());
    let id = uuid::Uuid::new_v4().to_string();
    let p = AppPaths::receipt_relative(&id, "jpg").unwrap();
    assert_eq!(p, format!("receipts/{id}/original.jpg"));
    assert!(AppPaths::receipt_relative("../bad", "jpg").is_err());
    assert!(AppPaths::receipt_relative(&id, "exe").is_err());
    assert_eq!(
        AppPaths::receipt_relative(&id, "heic").unwrap(),
        format!("receipts/{id}/original.heic")
    );
}
#[cfg(unix)]
#[test]
fn internal_paths_reject_symlink_escape() {
    let (t, s) = workspace();
    std::os::unix::fs::symlink(t.path(), s.paths.root.join("escape")).unwrap();
    assert!(s.paths.resolve("escape/outside.txt").is_err());
}
#[test]
fn explicit_status_transitions() {
    use ExpenseStatus::*;
    assert!(Draft.can_transition(Extracting));
    assert!(Extracting.can_transition(NeedsReview));
    assert!(NeedsReview.can_transition(Ready));
    assert!(Ready.can_transition(Submitted));
    assert!(!Draft.can_transition(Submitted));
    assert!(!Submitted.can_transition(Extracting));
}
#[test]
fn model_output_validation_is_strict() {
    let good = result();
    folio::services::extraction::normalizer::validate(&good).unwrap();
    let mut bad = good.clone();
    bad.total_amount_minor = Some(-1);
    assert!(folio::services::extraction::normalizer::validate(&bad).is_err());
    bad = good.clone();
    bad.confidence.insert("total".into(), 1.1);
    assert!(folio::services::extraction::normalizer::validate(&bad).is_err());
    bad = good.clone();
    bad.date = Some("2026-02-31".into());
    assert!(folio::services::extraction::normalizer::validate(&bad).is_err());
    bad = good;
    bad.tax_amount_minor = Some(100000);
    assert!(folio::services::extraction::normalizer::validate(&bad).is_err());
    let extra = serde_json::json!({"merchantName":null,"date":null,"totalAmountMinor":1.5,"taxAmountMinor":null,"currency":null,"suggestedCategory":null,"confidence":{},"extra":"injection"});
    assert!(serde_json::from_value::<Extraction>(extra).is_err());
}
#[test]
fn offline_cross_checked_receipt_is_ready_without_review() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    s.complete_ocr(
        &j.job.id,
        &j.token,
        "KOPI HOUSE\n2026-09-27\nSubtotal RM 79.72\nSST 6% 4.78\nTOTAL MYR 84.50\n".into(),
        Some(91.0),
    )
    .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.merchant_name.as_deref(), Some("KOPI HOUSE"));
    assert_eq!(e.occurred_at.as_deref(), Some("2026-09-27"));
    assert_eq!(e.total_amount_minor, Some(8450));
    assert_eq!(e.tax_amount_minor, Some(478));
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.category, "Meals");
    assert_eq!(e.status, ExpenseStatus::Ready);
}
#[test]
fn premises_are_read_from_the_address_and_manual_corrections_stick() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let ocr = "MR. D.I.Y. (M) SDN BHD\nLot G-23, Sunway Pyramid\n3, Jalan PJS 11/15, Bandar Sunway\n47500 Petaling Jaya, Selangor\n28-09-2026 19:05\nTOTAL (INCL SST) 54.30\nTNG EWALLET 54.30\n";
    let j = extraction_job(&s);
    s.complete_ocr(&j.job.id, &j.token, ocr.into(), Some(90.0))
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(
        e.premises.as_deref(),
        Some("Lot G-23, Sunway Pyramid, 3, Jalan PJS 11/15, Bandar Sunway, 47500 Petaling Jaya, Selangor, Malaysia")
    );
    assert_eq!(e.field_meta["premises"].confidence, 0.95);
    assert_eq!(e.status, ExpenseStatus::Ready);
    // The user shortens it; a later re-scan must not overwrite the correction.
    let mut change = edit(&s, &id);
    change.premises = Some("Sunway Pyramid, Petaling Jaya".into());
    s.edit_expense(change).unwrap();
    s.queue_extraction(&id).unwrap();
    let j = extraction_job(&s);
    s.complete_ocr(&j.job.id, &j.token, ocr.into(), Some(90.0))
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.premises.as_deref(), Some("Sunway Pyramid, Petaling Jaya"));
    assert_eq!(e.field_meta["premises"].source, FieldSource::Manual);
}
#[test]
fn offline_unreadable_or_unconfirmed_receipts_still_need_review() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    // The same receipt, but Tesseract was unsure of what it read.
    s.complete_ocr(
        &j.job.id,
        &j.token,
        "KOPI HOUSE\n2026-09-27\nSubtotal RM 79.72\nSST 6% 4.78\nTOTAL MYR 84.50\n".into(),
        Some(38.0),
    )
    .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.total_amount_minor, Some(8450));
    assert_eq!(e.status, ExpenseStatus::NeedsReview);
    // A clear page whose total nothing else on the receipt agrees with.
    s.queue_extraction(&id).unwrap();
    let j = extraction_job(&s);
    s.complete_ocr(
        &j.job.id,
        &j.token,
        "KOPI HOUSE\n2026-09-27\nNasi lemak RM 30.00\nTOTAL MYR 12.00\n".into(),
        Some(91.0),
    )
    .unwrap();
    assert_eq!(s.expense(&id).unwrap().status, ExpenseStatus::NeedsReview);
}
#[test]
fn manual_edits_and_clears_survive_later_extraction() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    let mut change = edit(&s, &id);
    change.tax_amount_minor = None;
    let saved = s.edit_expense(change).unwrap();
    assert_eq!(saved.field_meta["merchantName"].source, FieldSource::Manual);
    s.queue_extraction(&id).unwrap();
    let j = extraction_job(&s);
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.merchant_name.as_deref(), Some("Client lunch"));
    assert_eq!(e.total_amount_minor, Some(8450));
    assert_eq!(e.tax_amount_minor, None);
}
#[test]
fn online_extraction_with_tax_over_total_completes_instead_of_failing_the_job() {
    // Regression for I-2: an online-shaped extraction with one bad field
    // (tax > total) must still sanitize into a valid result and complete the
    // job, rather than surfacing an InvalidExtraction error that would make
    // `try_online` fall back to OCR and discard a correct AI extraction.
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    let mut hostile = result();
    hostile.tax_amount_minor = Some(9_999_999);
    assert!(hostile.total_amount_minor.unwrap() < hostile.tax_amount_minor.unwrap());
    s.complete_extraction(&j.job.id, &j.token, hostile, FieldSource::OnlineAi, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.merchant_name.as_deref(), Some("OCR merchant"));
    assert_eq!(e.occurred_at.as_deref(), Some("2026-09-27"));
    assert_eq!(e.total_amount_minor, Some(5000));
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.tax_amount_minor, None);
}
fn extraction(v: serde_json::Value) -> Extraction {
    serde_json::from_value(v).unwrap()
}
fn manual_only(s: &AppService, id: &str, f: impl FnOnce(&mut ExpenseEdit)) {
    let e = s.expense(id).unwrap();
    let mut change = ExpenseEdit {
        id: id.into(),
        version: e.version,
        occurred_at: None,
        merchant_name: None,
        premises: None,
        total_amount_minor: None,
        tax_amount_minor: None,
        currency: None,
        category: "Other".into(),
        description: String::new(),
        mark_ready: false,
    };
    f(&mut change);
    s.edit_expense(change).unwrap();
}
#[test]
fn reextracting_a_claimed_ready_expense_converts_to_the_claim_currency_and_needs_review() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    assert_eq!(s.expense(&id).unwrap().status, ExpenseStatus::Ready);
    let claim = s.create_claim("C".into(), "MYR".into()).unwrap();
    s.set_claim_expense(&claim.id, &id, true).unwrap();
    s.queue_extraction(&id).unwrap();
    let j = extraction_job(&s);
    let low = extraction(
        serde_json::json!({"merchantName":"Other","date":"2026-09-20","totalAmountMinor":999,"taxAmountMinor":null,"currency":"USD","suggestedCategory":"Meals","confidence":{"merchantName":0.1,"date":0.1,"total":0.1,"tax":0.0,"currency":0.1,"category":0.1}}),
    );
    s.complete_extraction(&j.job.id, &j.token, low, FieldSource::LocalOcr, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(
        e.currency.as_deref(),
        Some("MYR"),
        "claim currency is fixed"
    );
    // A USD receipt never lands in a MYR claim as-is: it is converted.
    assert_eq!(e.original_currency.as_deref(), Some("USD"));
    assert_eq!(e.original_total_amount_minor, Some(999));
    assert_eq!(e.total_amount_minor, Some(4206));
    assert_eq!(
        e.status,
        ExpenseStatus::NeedsReview,
        "changed low-confidence values need review"
    );
    assert_eq!(s.claim(&claim.id).unwrap().claim.total_amount_minor, 4206);
    assert!(s
        .transition_claim(&claim.id, ClaimStatus::Submitted)
        .is_err());
}
#[test]
fn reextraction_with_identical_values_keeps_a_ready_expense_ready() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    s.queue_extraction(&id).unwrap();
    let j = extraction_job(&s);
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    assert_eq!(s.expense(&id).unwrap().status, ExpenseStatus::Ready);
}
#[test]
fn manual_currency_rejects_amounts_extracted_in_another_currency() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    manual_only(&s, &id, |c| c.currency = Some("JPY".into()));
    // RM 50.00 arrives as 5000 minor units at exponent 2; as JPY that is ¥5,000.
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.currency.as_deref(), Some("JPY"));
    assert_eq!(e.total_amount_minor, None);
    assert_eq!(e.tax_amount_minor, None);
    assert_eq!(
        e.merchant_name.as_deref(),
        Some("OCR merchant"),
        "other fields still merge"
    );
    assert_eq!(e.status, ExpenseStatus::NeedsReview);
}
#[test]
fn manual_total_below_extracted_tax_keeps_the_rest_of_the_extraction() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    manual_only(&s, &id, |c| c.total_amount_minor = Some(100));
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.total_amount_minor, Some(100));
    assert_eq!(
        e.tax_amount_minor, None,
        "extracted tax above the manual total is dropped"
    );
    assert_eq!(e.merchant_name.as_deref(), Some("OCR merchant"));
    assert_eq!(e.occurred_at.as_deref(), Some("2026-09-27"));
    assert_eq!(e.status, ExpenseStatus::NeedsReview);
}
#[test]
fn manual_tax_above_extracted_total_keeps_the_rest_of_the_extraction() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    manual_only(&s, &id, |c| c.tax_amount_minor = Some(300));
    let small = extraction(
        serde_json::json!({"merchantName":"OCR merchant","date":"2026-09-27","totalAmountMinor":200,"taxAmountMinor":null,"currency":"MYR","suggestedCategory":"Meals","confidence":{"merchantName":0.98,"date":0.99,"total":0.99,"tax":0.0,"currency":0.99,"category":0.9}}),
    );
    s.complete_extraction(&j.job.id, &j.token, small, FieldSource::OnlineAi, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.tax_amount_minor, Some(300));
    assert_eq!(
        e.total_amount_minor, None,
        "extracted total below the manual tax is dropped"
    );
    assert_eq!(e.merchant_name.as_deref(), Some("OCR merchant"));
    assert_eq!(e.status, ExpenseStatus::NeedsReview);
}
#[test]
fn removing_an_expense_that_is_not_in_the_claim_is_rejected_without_side_effects() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let claim = s.create_claim("C".into(), "MYR".into()).unwrap();
    let expense_version = s.expense(&id).unwrap().version;
    let claim_version = s.claim(&claim.id).unwrap().claim.version;
    assert!(s.set_claim_expense(&claim.id, &id, false).is_err());
    assert_eq!(s.expense(&id).unwrap().version, expense_version);
    assert_eq!(s.claim(&claim.id).unwrap().claim.version, claim_version);
}
#[test]
fn stale_edits_fail_without_losing_updates() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let old = edit(&s, &id);
    s.edit_expense(old.clone()).unwrap();
    assert_eq!(s.edit_expense(old).unwrap_err().code, "Conflict");
}
#[test]
fn claim_totals_currency_and_membership_are_enforced() {
    let (_t, s) = workspace();
    let id = s.create_expense().unwrap().id;
    s.edit_expense(edit(&s, &id)).unwrap();
    let claim = s.create_claim("September".into(), "MYR".into()).unwrap();
    let usd = s.create_claim("US expenses".into(), "USD".into()).unwrap();
    assert!(s.set_claim_expense(&usd.id, &id, true).is_err());
    let d = s.set_claim_expense(&claim.id, &id, true).unwrap();
    assert_eq!(d.claim.total_amount_minor, 8450);
    assert_eq!(d.claim.expense_count, 1);
    assert!(s.set_claim_expense(&claim.id, &id, true).is_err());
    // BHD has no published rate, so it can't be converted back to the claim's MYR.
    let mut change = edit(&s, &id);
    change.currency = Some("BHD".into());
    assert!(s.edit_expense(change).is_err());
    s.transition_claim(&claim.id, ClaimStatus::Submitted)
        .unwrap();
    assert_eq!(s.expense(&id).unwrap().status, ExpenseStatus::Submitted);
    s.transition_claim(&claim.id, ClaimStatus::Draft).unwrap();
    s.edit_expense(edit(&s, &id)).unwrap();
    let d = s.set_claim_expense(&claim.id, &id, false).unwrap();
    assert_eq!(d.claim.total_amount_minor, 0);
    assert!(s.request_pdf(&claim.id).is_err());
}
#[test]
fn receipt_values_stay_editable_after_the_claim_is_submitted_or_archived() {
    let (t, s) = workspace();
    let id = s.create_expense().unwrap().id;
    s.edit_expense(edit(&s, &id)).unwrap();
    let claim = s.create_claim("September".into(), "MYR".into()).unwrap();
    s.set_claim_expense(&claim.id, &id, true).unwrap();
    s.transition_claim(&claim.id, ClaimStatus::Submitted)
        .unwrap();
    let before = s.claim(&claim.id).unwrap().claim;

    let mut fix = edit(&s, &id);
    fix.merchant_name = Some("Corrected merchant".into());
    fix.total_amount_minor = Some(9000);
    let saved = s.edit_expense(fix).unwrap();
    assert_eq!(saved.merchant_name.as_deref(), Some("Corrected merchant"));
    assert_eq!(saved.total_amount_minor, Some(9000));
    assert_eq!(
        saved.status,
        ExpenseStatus::Submitted,
        "an edit never moves an expense out of its claim's status"
    );
    let detail = s.claim(&claim.id).unwrap();
    let after = detail.claim;
    // Reports exported before this edit are outdated; a status change alone is not a content change.
    let changed_at = detail
        .content_changed_at
        .expect("the edit is a content change");
    assert!(changed_at >= before.updated_at);
    assert_eq!(after.status.as_str(), "submitted");
    assert_eq!(after.total_amount_minor, 9000);
    assert!(after.version > before.version);
    assert!(after.updated_at >= before.updated_at);

    s.transition_claim(&claim.id, ClaimStatus::Archived)
        .unwrap();
    assert_eq!(
        s.claim(&claim.id).unwrap().content_changed_at,
        Some(changed_at),
        "archiving does not make exported reports outdated"
    );
    let mut again = edit(&s, &id);
    again.merchant_name = Some("Corrected again".into());
    again.mark_ready = false;
    let saved = s.edit_expense(again).unwrap();
    assert_eq!(saved.merchant_name.as_deref(), Some("Corrected again"));
    assert_eq!(saved.status, ExpenseStatus::Archived);
    assert_eq!(
        s.claim(&claim.id).unwrap().claim.status.as_str(),
        "archived"
    );

    // The edits are recorded, along with the claim status they were made under.
    let db = rusqlite::Connection::open(t.path().join("app/database/expenses.sqlite")).unwrap();
    let details: Vec<String> = db
        .prepare("SELECT details FROM audit_events WHERE event_type='expense.edited' AND entity_id=?1 ORDER BY created_at, rowid")
        .unwrap()
        .query_map([&id], |r| r.get(0))
        .unwrap()
        .map(|d| d.unwrap())
        .collect();
    assert!(details[details.len() - 2].contains("\"claimStatus\":\"submitted\""));
    assert!(details[details.len() - 1].contains("\"claimStatus\":\"archived\""));

    // Everything else about a finished claim stays locked.
    assert!(s.delete_expenses(vec![id.clone()]).is_err());
    assert!(s.set_claim_expense(&claim.id, &id, false).is_err());
}
#[test]
fn crash_recovery_invalidates_old_lease_and_preserves_data() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    drop(s);
    let reopened =
        AppService::open(t.path().join("app"), Arc::new(MemorySecrets::default())).unwrap();
    assert_eq!(reopened.expenses().unwrap().len(), 1);
    assert_eq!(
        reopened.expense(&id).unwrap().status,
        ExpenseStatus::NeedsReview
    );
    assert!(reopened
        .complete_ocr(&j.job.id, &j.token, "text".into(), None)
        .is_err());
    let new_job = reopened.take_job().unwrap().unwrap();
    assert_eq!(new_job.job.id, j.job.id);
    assert_ne!(new_job.token, j.token);
}
#[test]
fn full_service_workflow_exports_snapshot_and_reopens() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    s.complete_ocr(
        &j.job.id,
        &j.token,
        "KOPI HOUSE\n2026-09-27\nTOTAL MYR 84.50\nTAX 4.78".into(),
        None,
    )
    .unwrap();
    s.edit_expense(edit(&s, &id)).unwrap();
    let claim = s.create_claim("Client visit".into(), "MYR".into()).unwrap();
    s.set_claim_expense(&claim.id, &id, true).unwrap();
    let job_id = s.request_pdf(&claim.id).unwrap();
    let j = s.take_job().unwrap().unwrap();
    assert_eq!(j.job.id, job_id);
    let snapshot = s.export_snapshot(&j.job.id, &j.token).unwrap();
    assert_eq!(snapshot.claim.total_amount_minor, 8450);
    assert_eq!(snapshot.expenses.len(), 1);
    assert!(snapshot.receipts[0].relative_path.starts_with("receipts/"));
    drop(s);
    let reopened =
        AppService::open(t.path().join("app"), Arc::new(MemorySecrets::default())).unwrap();
    assert_eq!(
        reopened.claim(&claim.id).unwrap().claim.total_amount_minor,
        8450
    );
    assert!(reopened
        .read_receipt(snapshot.receipts[0].id.as_str())
        .is_ok());
}
#[test]
fn provider_url_change_clears_key_and_never_puts_secret_in_database() {
    let (_t, s) = workspace();
    s.set_credential("test-secret-keep-out-of-db".into())
        .unwrap();
    assert!(s.app_info().unwrap().credential_configured);
    let mut settings = s.settings().unwrap();
    settings.api_base_url = "https://other.example/v1".into();
    s.save_settings(settings).unwrap();
    assert!(!s.app_info().unwrap().credential_configured);
    let bytes = fs::read(s.paths.root.join("database/expenses.sqlite-wal")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("test-secret"));
}
#[test]
fn png_thumbnail_is_reencoded_as_webp_for_webkit() {
    use base64::Engine;
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = s.take_job().unwrap().unwrap();
    assert_eq!(j.job.job_type, "generate_thumbnail");
    let bytes = fs::read(receipt(&t)).unwrap();
    s.complete_thumbnail(
        &j.job.id,
        &j.token,
        &base64::engine::general_purpose::STANDARD.encode(bytes),
    )
    .unwrap();
    let e = s.expense(&id).unwrap();
    let stored = s
        .read_thumbnail(e.receipt_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(stored.split(',').nth(1).unwrap())
        .unwrap();
    assert_eq!(&decoded[8..12], b"WEBP");
}

#[test]
fn old_reports_and_failed_jobs_remain_reachable_after_queue_rollover() {
    let (_t, s) = workspace();
    let db = rusqlite::Connection::open(s.paths.root.join("database/expenses.sqlite")).unwrap();
    for (id, kind, status) in [
        ("old-report", "generate_pdf", "completed"),
        ("old-failure", "extract_receipt", "failed"),
    ] {
        db.execute("INSERT INTO jobs(id,job_type,entity_id,status,created_at,updated_at) VALUES(?1,?2,?1,?3,'2026-01-01','2026-01-01')", rusqlite::params![id,kind,status]).unwrap();
    }
    for index in 0..205 {
        db.execute("INSERT INTO jobs(id,job_type,entity_id,status,created_at,updated_at) VALUES(?1,'generate_thumbnail',?1,'completed','2026-09-27','2026-09-27')", [format!("recent-{index}")]).unwrap();
    }
    let jobs = s.jobs().unwrap();
    assert!(jobs.iter().any(|job| job.id == "old-report"));
    assert!(jobs.iter().any(|job| job.id == "old-failure"));
}
#[test]
fn csv_export_claim_orders_receipt_refs_like_pdf_and_sums_amounts() {
    let (t, s) = workspace();
    let receipted_id = import(&t, &s);
    let mut receipted_edit = edit(&s, &receipted_id);
    receipted_edit.occurred_at = Some("2026-09-10".into());
    receipted_edit.merchant_name = Some("Receipted Cafe".into());
    receipted_edit.total_amount_minor = Some(1000);
    receipted_edit.tax_amount_minor = Some(0);
    s.edit_expense(receipted_edit).unwrap();
    let manual_id = s.create_expense().unwrap().id;
    let mut manual_edit = edit(&s, &manual_id);
    manual_edit.occurred_at = Some("2026-09-25".into());
    manual_edit.merchant_name = Some("Manual Entry".into());
    manual_edit.total_amount_minor = Some(2000);
    manual_edit.tax_amount_minor = Some(0);
    s.edit_expense(manual_edit).unwrap();
    let claim = s.create_claim("Trip".into(), "MYR".into()).unwrap();
    s.set_claim_expense(&claim.id, &receipted_id, true).unwrap();
    s.set_claim_expense(&claim.id, &manual_id, true).unwrap();
    let export = s.export_claim_csv(&claim.id).unwrap();
    assert_eq!(export.rows, 2);
    assert_eq!(
        export.file_name,
        format!("Folio-{}.csv", claim.claim_number)
    );
    let bytes = fs::read(&export.path).unwrap();
    assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    let lines: Vec<&str> = text.trim_end_matches("\r\n").split("\r\n").collect();
    assert_eq!(lines.len(), 3);
    // Newest occurred_at sorts first, matching the PDF/claim expense order.
    let manual_cells: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(
        manual_cells[2], "",
        "manual expense has a blank Receipt Ref"
    );
    assert_eq!(manual_cells[4], "Manual Entry");
    assert_eq!(manual_cells[5], "", "no premises was entered");
    assert_eq!(manual_cells[9], "20.00");
    let receipted_cells: Vec<&str> = lines[2].split(',').collect();
    assert_eq!(receipted_cells[2], "R002");
    assert_eq!(receipted_cells[4], "Receipted Cafe");
    assert_eq!(receipted_cells[9], "10.00");
}
#[test]
fn csv_export_formats_by_currency_exponent_dedupes_and_rejects_unknown_ids() {
    let (_t, s) = workspace();
    let jpy_id = s.create_expense().unwrap().id;
    let mut jpy_edit = edit(&s, &jpy_id);
    jpy_edit.currency = Some("JPY".into());
    jpy_edit.total_amount_minor = Some(1500);
    jpy_edit.tax_amount_minor = Some(0);
    s.edit_expense(jpy_edit).unwrap();
    let bhd_id = s.create_expense().unwrap().id;
    let mut bhd_edit = edit(&s, &bhd_id);
    bhd_edit.currency = Some("BHD".into());
    bhd_edit.total_amount_minor = Some(12345);
    bhd_edit.tax_amount_minor = Some(5);
    s.edit_expense(bhd_edit).unwrap();
    let export = s
        .export_expenses_csv(vec![jpy_id.clone(), bhd_id.clone()])
        .unwrap();
    let bytes = fs::read(&export.path).unwrap();
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    // JPY 1,500 is converted at 0.029 to MYR 43.50; the receipt amount and
    // rate are kept after the receipt columns, formatted with JPY's 0 decimals,
    // followed by the (empty) personal name and organization.
    assert!(text.contains(",MYR,43.50,0.00,"));
    assert!(text.contains(",JPY,1500,0.029,2026-09-27,,\r\n"));
    // BHD has no published rate, so it stays as printed (3 decimals).
    assert!(text.contains(",BHD,12.345,0.005,"));
    let dedup = s
        .export_expenses_csv(vec![jpy_id.clone(), jpy_id.clone()])
        .unwrap();
    assert_eq!(dedup.rows, 1);
    assert!(s.export_expenses_csv(vec![]).is_err());
    let xlsx = s
        .export_expenses_xlsx(vec![jpy_id.clone(), bhd_id.clone(), jpy_id.clone()])
        .unwrap();
    assert_eq!(xlsx.rows, 2);
    assert!(xlsx.file_name.ends_with(".xlsx"));
    let xlsx_bytes = fs::read(&xlsx.path).unwrap();
    assert!(xlsx_bytes.starts_with(b"PK"));
    assert_eq!(s.xlsx_export_path(&xlsx.id).unwrap(), xlsx.path);
    assert!(s.export_expenses_xlsx(vec![]).is_err());
    let unknown = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        s.export_expenses_csv(vec![unknown]).unwrap_err().code,
        "NotFound"
    );
    assert_eq!(
        s.export_claim_csv(&uuid::Uuid::new_v4().to_string())
            .unwrap_err()
            .code,
        "NotFound"
    );
}
#[test]
fn csv_export_copies_to_export_directory_and_reports_conflicts() {
    let (t, s) = workspace();
    let id = s.create_expense().unwrap().id;
    s.edit_expense(edit(&s, &id)).unwrap();
    let claim = s.create_claim("September".into(), "MYR".into()).unwrap();
    s.set_claim_expense(&claim.id, &id, true).unwrap();
    let export_dir = t.path().join("exports-out");
    fs::create_dir_all(&export_dir).unwrap();
    let mut settings = s.settings().unwrap();
    settings.export_directory = Some(export_dir.to_string_lossy().to_string());
    s.save_settings(settings).unwrap();
    let export = s.export_claim_csv(&claim.id).unwrap();
    let copy_path = export_dir.join(&export.file_name);
    assert!(copy_path.exists());
    assert_eq!(
        fs::read(&copy_path).unwrap(),
        fs::read(&export.path).unwrap()
    );
    assert_eq!(s.csv_export_path(&export.id).unwrap(), export.path);
    assert_eq!(
        s.csv_export_path(&uuid::Uuid::new_v4().to_string())
            .unwrap_err()
            .code,
        "NotFound"
    );
    assert_eq!(
        s.csv_export_path("not-a-uuid").unwrap_err().code,
        "ValidationError"
    );

    // Re-exporting after the expense changes must not fail just because the
    // export directory already has a file with that name: it should retry
    // with a " (2)" suffix instead, and the returned `file_name` must match
    // what actually landed in the export directory.
    let mut change = edit(&s, &id);
    change.total_amount_minor = Some(9999);
    s.edit_expense(change).unwrap();
    let export2 = s.export_claim_csv(&claim.id).unwrap();
    assert_ne!(export2.file_name, export.file_name);
    assert!(
        export2.file_name.ends_with(" (2).csv"),
        "expected a \" (2).csv\" suffix, got {}",
        export2.file_name
    );
    let copy_path2 = export_dir.join(&export2.file_name);
    assert!(copy_path.exists(), "the first export's copy must remain");
    assert!(
        copy_path2.exists(),
        "the second export must land in a distinct file"
    );
    let second_contents = String::from_utf8(fs::read(&copy_path2).unwrap()).unwrap();
    assert!(
        second_contents.contains("99.99"),
        "the second copy must contain the updated amount"
    );
    let workbook = s.export_claim_xlsx(&claim.id).unwrap();
    assert!(workbook.file_name.ends_with(".xlsx"));
    let workbook_copy = export_dir.join(&workbook.file_name);
    assert!(workbook_copy.exists());
    assert!(fs::read(&workbook_copy).unwrap().starts_with(b"PK"));
    assert_eq!(s.xlsx_export_path(&workbook.id).unwrap(), workbook.path);
    let workbook2 = s.export_claim_xlsx(&claim.id).unwrap();
    assert_ne!(workbook2.file_name, workbook.file_name);
    assert!(workbook2.file_name.ends_with(" (2).xlsx"));
}

#[test]
fn foreign_receipt_is_converted_to_home_currency_at_the_receipt_date_rate() {
    let (t, s) = workspace();
    let id = extracted(&t, &s, 1, usd_result());
    let e = s.expense(&id).unwrap();
    // USD 45.00 × 4.2105 = MYR 189.4725 → 189.47; tax 2.50 → 10.53
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.total_amount_minor, Some(18947));
    assert_eq!(e.tax_amount_minor, Some(1053));
    assert_eq!(e.original_currency.as_deref(), Some("USD"));
    assert_eq!(e.original_total_amount_minor, Some(4500));
    assert_eq!(e.original_tax_amount_minor, Some(250));
    assert_eq!(e.exchange_rate.as_deref(), Some("4.2105"));
    assert_eq!(e.exchange_rate_date.as_deref(), Some("2026-09-25"));
    assert_eq!(e.status, ExpenseStatus::Ready);
    // It can go straight into a MYR claim, which totals in MYR.
    let claim = s.create_claim("Trip".into(), "MYR".into()).unwrap();
    let detail = s.add_claim_expenses(&claim.id, vec![id]).unwrap();
    assert_eq!(detail.claim.total_amount_minor, 18947);
}
#[test]
fn offline_conversion_waits_and_finishes_when_back_online() {
    let (t, s, rates) = workspace_with_rates();
    rates.offline.store(true, Ordering::SeqCst);
    let id = extracted(&t, &s, 1, usd_result());
    let e = s.expense(&id).unwrap();
    assert_eq!(e.currency.as_deref(), Some("USD"));
    assert_eq!(e.total_amount_minor, Some(4500));
    assert_eq!(e.original_currency, None);
    assert_eq!(s.convert_pending().unwrap(), 0);
    rates.offline.store(false, Ordering::SeqCst);
    assert_eq!(s.convert_pending().unwrap(), 1);
    let e = s.expense(&id).unwrap();
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.total_amount_minor, Some(18947));
    // Nothing left to convert, and the rate is cached locally.
    assert_eq!(s.convert_pending().unwrap(), 0);
    assert_eq!(rates.downloads.load(Ordering::SeqCst), 1);
}
#[test]
fn editing_the_receipt_amount_reconverts_and_other_edits_keep_the_conversion() {
    let (t, s, rates) = workspace_with_rates();
    let id = extracted(&t, &s, 1, usd_result());
    let e = s.expense(&id).unwrap();
    // The editor works on the receipt's own amounts.
    let receipt_edit = |e: &folio::domain::expense::Expense| ExpenseEdit {
        id: e.id.clone(),
        version: e.version,
        occurred_at: e.occurred_at.clone(),
        merchant_name: e.merchant_name.clone(),
        premises: e.premises.clone(),
        total_amount_minor: e.original_total_amount_minor,
        tax_amount_minor: e.original_tax_amount_minor,
        currency: e.original_currency.clone(),
        category: e.category.clone(),
        description: e.description.clone(),
        mark_ready: true,
    };
    let mut notes = receipt_edit(&e);
    notes.description = "Team dinner".into();
    let e = s.edit_expense(notes).unwrap();
    assert_eq!(e.total_amount_minor, Some(18947));
    assert_eq!(e.original_total_amount_minor, Some(4500));
    let mut corrected = receipt_edit(&e);
    corrected.total_amount_minor = Some(5000);
    let e = s.edit_expense(corrected).unwrap();
    // USD 50.00 × 4.2105 = MYR 210.525 → 210.53
    assert_eq!(e.total_amount_minor, Some(21053));
    assert_eq!(e.original_total_amount_minor, Some(5000));
    assert_eq!(e.field_meta["total"].source, FieldSource::Manual);
    // Switching the receipt to the home currency drops the conversion.
    let mut home = receipt_edit(&e);
    home.currency = Some("MYR".into());
    let e = s.edit_expense(home).unwrap();
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.total_amount_minor, Some(5000));
    assert_eq!(e.original_currency, None);
    assert_eq!(e.exchange_rate, None);
    assert_eq!(rates.downloads.load(Ordering::SeqCst), 1);
}
#[test]
fn conversion_can_be_turned_off_and_unknown_rates_keep_the_receipt_currency() {
    let (t, s) = workspace();
    let mut settings = s.settings().unwrap();
    settings.currency_conversion_enabled = false;
    s.save_settings(settings).unwrap();
    let id = extracted(&t, &s, 1, usd_result());
    assert_eq!(s.expense(&id).unwrap().currency.as_deref(), Some("USD"));
    assert_eq!(s.convert_pending().unwrap(), 0);
    let mut settings = s.settings().unwrap();
    settings.currency_conversion_enabled = true;
    s.save_settings(settings).unwrap();
    // A currency with no published rate (BHD) stays as printed.
    let mut bhd = usd_result();
    bhd.currency = Some("BHD".into());
    bhd.total_amount_minor = Some(12500);
    bhd.tax_amount_minor = None;
    let bhd_id = extracted(&t, &s, 2, bhd);
    assert_eq!(s.expense(&bhd_id).unwrap().currency.as_deref(), Some("BHD"));
    // Turning conversion back on converts the waiting USD receipt.
    assert_eq!(s.convert_pending().unwrap(), 1);
    assert_eq!(s.expense(&id).unwrap().currency.as_deref(), Some("MYR"));
}
#[test]
fn deleting_expenses_removes_receipts_and_allows_reimport() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    // Refused while the receipt is being read.
    let j = extraction_job(&s);
    let err = s.delete_expenses(vec![id.clone()]).unwrap_err();
    assert!(
        err.message.contains("still being processed"),
        "{}",
        err.message
    );
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    let e = s.expense(&id).unwrap();
    let receipt_dir = s
        .paths
        .root
        .join("receipts")
        .join(e.receipt_id.clone().unwrap());
    assert!(receipt_dir.exists());
    let claim = s.create_claim("Trip".into(), "MYR".into()).unwrap();
    s.add_claim_expenses(&claim.id, vec![id.clone()]).unwrap();
    // Refused while its claim is submitted; allowed once reopened as a draft.
    s.transition_claim(&claim.id, ClaimStatus::Submitted)
        .unwrap();
    assert!(s.delete_expenses(vec![id.clone()]).is_err());
    s.transition_claim(&claim.id, ClaimStatus::Draft).unwrap();
    assert_eq!(s.delete_expenses(vec![id.clone(), id.clone()]).unwrap(), 1);
    assert_eq!(s.expense(&id).unwrap_err().code, "NotFound");
    assert!(!receipt_dir.exists());
    let claim = s.claim(&claim.id).unwrap().claim;
    assert_eq!((claim.expense_count, claim.total_amount_minor), (0, 0));
    assert!(s.jobs().unwrap().iter().all(|j| j.entity_id != id));
    // The same file can be imported again.
    let again = import(&t, &s);
    assert_ne!(again, id);
    // Manual expenses (no receipt) can be deleted too; unknown IDs change nothing.
    let manual = s.create_expense().unwrap().id;
    assert!(s
        .delete_expenses(vec![manual.clone(), "missing".into()])
        .is_err());
    assert!(s.expense(&manual).is_ok());
    assert_eq!(s.delete_expenses(vec![manual]).unwrap(), 1);
}
#[test]
fn imported_receipts_needing_review_can_be_added_to_a_claim_in_bulk() {
    let (t, s) = workspace();
    let ready = extracted(&t, &s, 1, result());
    let mut unsure = result();
    unsure.confidence.insert("total".into(), 0.4);
    let review = extracted(&t, &s, 2, unsure);
    assert_eq!(
        s.expense(&review).unwrap().status,
        ExpenseStatus::NeedsReview
    );
    let mut usd = usd_result();
    usd.currency = Some("BHD".into());
    usd.tax_amount_minor = None;
    let foreign = extracted(&t, &s, 3, usd);
    let claim = s.create_claim("Trip".into(), "MYR".into()).unwrap();
    // A wrong-currency expense rejects the whole batch.
    let err = s
        .add_claim_expenses(&claim.id, vec![ready.clone(), foreign])
        .unwrap_err();
    assert!(err.message.contains("BHD"), "{}", err.message);
    assert_eq!(s.claim(&claim.id).unwrap().claim.expense_count, 0);
    let detail = s
        .add_claim_expenses(&claim.id, vec![ready.clone(), review.clone()])
        .unwrap();
    assert_eq!(detail.claim.expense_count, 2);
    assert_eq!(detail.claim.total_amount_minor, 10000);
    // Already claimed.
    assert!(s.add_claim_expenses(&claim.id, vec![ready]).is_err());
    // Submitting and exporting wait until every expense is reviewed.
    assert!(s
        .transition_claim(&claim.id, ClaimStatus::Submitted)
        .is_err());
    assert!(s.request_pdf(&claim.id).is_err());
    let e = s.expense(&review).unwrap();
    s.edit_expense(ExpenseEdit {
        id: review.clone(),
        version: e.version,
        occurred_at: e.occurred_at,
        merchant_name: e.merchant_name,
        premises: e.premises,
        total_amount_minor: e.total_amount_minor,
        tax_amount_minor: e.tax_amount_minor,
        currency: e.currency,
        category: e.category,
        description: e.description,
        mark_ready: true,
    })
    .unwrap();
    s.transition_claim(&claim.id, ClaimStatus::Submitted)
        .unwrap();
}

/// A 48x80 HEIC (top half red, bottom half blue) encoded by macOS `sips`.
const HEIC: &[u8] = include_bytes!("fixtures/receipt.heic");
/// The same picture stored with a 90° `irot` transform (displays 80x48).
const HEIC_ROTATED: &[u8] = include_bytes!("fixtures/rotated.heic");
fn write_fixture(t: &TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = t.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}
fn decode_content(content: &folio::domain::receipt::ReceiptContent) -> image::RgbImage {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&content.base64)
        .unwrap();
    assert!(
        bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "rendition is a JPEG"
    );
    image::load_from_memory(&bytes).unwrap().to_rgb8()
}
fn is_red(p: &image::Rgb<u8>) -> bool {
    p[0] > 150 && p[2] < 100
}
fn is_blue(p: &image::Rgb<u8>) -> bool {
    p[2] > 150 && p[0] < 100
}
#[test]
fn heic_import_keeps_the_original_and_serves_a_jpeg_rendition() {
    let (t, s) = workspace();
    let path = write_fixture(&t, "IMG_0001.HEIC", HEIC);
    let id = s.import_receipt(&path).unwrap();
    let receipt_id = s.expense(&id).unwrap().receipt_id.unwrap();
    let content = s.read_receipt(&receipt_id).unwrap();
    // The stored record describes the untouched original…
    assert_eq!(content.receipt.mime_type, "image/heic");
    assert_eq!(content.receipt.original_filename, "IMG_0001.HEIC");
    assert_eq!(
        content.receipt.relative_path,
        format!("receipts/{receipt_id}/original.heic")
    );
    assert_eq!(content.receipt.size_bytes, HEIC.len() as i64);
    assert_eq!(
        fs::read(s.paths.root.join(&content.receipt.relative_path)).unwrap(),
        HEIC
    );
    // …while the bytes handed to the WebView are a JPEG every engine can draw.
    assert_eq!(content.mime_type, "image/jpeg");
    let image = decode_content(&content);
    assert_eq!(image.dimensions(), (48, 80));
    assert!(
        is_red(image.get_pixel(24, 10)),
        "{:?}",
        image.get_pixel(24, 10)
    );
    assert!(
        is_blue(image.get_pixel(24, 70)),
        "{:?}",
        image.get_pixel(24, 70)
    );
    // Duplicates are detected on the original bytes.
    let again = write_fixture(&t, "copy.heif", HEIC);
    let err = s.import_receipt(&again).unwrap_err();
    assert_eq!(err.code, "DuplicateReceipt");
    assert_eq!(fs::read_dir(s.paths.root.join("cache")).unwrap().count(), 0);
}
#[test]
fn other_receipts_report_their_own_mime_type_as_content() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let receipt_id = s.expense(&id).unwrap().receipt_id.unwrap();
    let content = s.read_receipt(&receipt_id).unwrap();
    assert_eq!(content.mime_type, "image/png");
    assert_eq!(content.receipt.mime_type, "image/png");
}
#[test]
fn heic_rendition_applies_the_stored_rotation() {
    let (t, s) = workspace();
    let id = s
        .import_receipt(&write_fixture(&t, "rotated.heic", HEIC_ROTATED))
        .unwrap();
    let receipt_id = s.expense(&id).unwrap().receipt_id.unwrap();
    let image = decode_content(&s.read_receipt(&receipt_id).unwrap());
    // Rotated 90° anticlockwise for display: red top half becomes the right side.
    assert_eq!(image.dimensions(), (80, 48));
    assert!(
        is_blue(image.get_pixel(10, 24)),
        "{:?}",
        image.get_pixel(10, 24)
    );
    assert!(
        is_red(image.get_pixel(70, 24)),
        "{:?}",
        image.get_pixel(70, 24)
    );
}
#[test]
fn rejects_disguised_and_undecodable_heic_files() {
    let (t, s) = workspace();
    // PNG bytes behind a .heic name.
    let png = fs::read(receipt(&t)).unwrap();
    let cases = [
        ("png.heic", png),
        ("text.heic", b"not a HEIC at all".to_vec()),
        // A HEIF header whose image data is cut off.
        ("truncated.heic", HEIC[..HEIC.len() / 2].to_vec()),
    ];
    for (name, bytes) in cases {
        let err = s
            .import_receipt(&write_fixture(&t, name, &bytes))
            .unwrap_err();
        assert_eq!(err.code, "FileUnsupported", "{name}: {}", err.message);
    }
    // HEIC bytes behind a .jpg name are refused as well.
    let err = s
        .import_receipt(&write_fixture(&t, "photo.jpg", HEIC))
        .unwrap_err();
    assert_eq!(err.code, "FileUnsupported");
    assert!(s.expenses().unwrap().is_empty());
    assert_eq!(fs::read_dir(s.paths.root.join("cache")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(s.paths.root.join("receipts")).unwrap().count(),
        0
    );
}
#[test]
fn rejects_heic_images_over_40_megapixels() {
    // 8000x5100 (40.8 MP) of plain white: 20 KB on disk.
    let (t, s) = workspace();
    let path = write_fixture(&t, "huge.heic", include_bytes!("fixtures/huge.heic"));
    let err = s.import_receipt(&path).unwrap_err();
    assert_eq!(err.code, "FileTooLarge", "{}", err.message);
    assert!(s.expenses().unwrap().is_empty());
}
#[test]
fn heic_rendition_is_rebuilt_when_missing() {
    let (t, s) = workspace();
    let id = s
        .import_receipt(&write_fixture(&t, "receipt.heic", HEIC))
        .unwrap();
    let receipt_id = s.expense(&id).unwrap().receipt_id.unwrap();
    let rendition = s
        .paths
        .root
        .join(format!("receipts/{receipt_id}/display.jpg"));
    assert!(rendition.exists());
    fs::remove_file(&rendition).unwrap();
    let image = decode_content(&s.read_receipt(&receipt_id).unwrap());
    assert_eq!(image.dimensions(), (48, 80));
}
#[test]
fn deleting_a_heic_expense_removes_the_original_and_its_rendition() {
    let (t, s) = workspace();
    let id = s
        .import_receipt(&write_fixture(&t, "receipt.heic", HEIC))
        .unwrap();
    let receipt_dir = s
        .paths
        .root
        .join("receipts")
        .join(s.expense(&id).unwrap().receipt_id.unwrap());
    assert!(receipt_dir.join("display.jpg").exists());
    let j = extraction_job(&s);
    s.complete_extraction(&j.job.id, &j.token, result(), FieldSource::OnlineAi, None)
        .unwrap();
    assert_eq!(s.delete_expenses(vec![id]).unwrap(), 1);
    assert!(!receipt_dir.exists());
}
#[test]
fn startup_removes_orphaned_heic_receipt_folders() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    AppService::open(root.clone(), Arc::new(MemorySecrets::default())).unwrap();
    let orphan = root.join("receipts").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("original.heic"), HEIC).unwrap();
    fs::write(orphan.join("display.jpg"), b"jpeg").unwrap();
    AppService::open(root, Arc::new(MemorySecrets::default())).unwrap();
    assert!(!orphan.exists());
}
