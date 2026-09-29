use folio::{
    domain::{
        claim::ClaimStatus,
        expense::{currency_exponent, parse_money, ExpenseEdit, ExpenseStatus},
        extraction::{Extraction, FieldSource},
    },
    error::Result,
    security::secrets::SecretStore,
    services::AppService,
    storage::paths::AppPaths,
};
use std::{
    fs,
    sync::{Arc, Mutex},
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
fn workspace() -> (TempDir, AppService) {
    let t = tempfile::tempdir().unwrap();
    let s = AppService::open(t.path().join("app"), Arc::new(MemorySecrets::default())).unwrap();
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
    assert_eq!(count, 8);
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
    db.execute_batch(
        "CREATE TABLE future(x TEXT); INSERT INTO future VALUES('keep me'); PRAGMA user_version=3;",
    )
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
    assert!(error.message.contains("version 3"), "{}", error.message);

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
fn offline_heuristics_extract_integer_money_and_mark_review() {
    let (t, s) = workspace();
    let id = import(&t, &s);
    let j = extraction_job(&s);
    s.complete_ocr(
        &j.job.id,
        &j.token,
        "KOPI HOUSE\n2026-09-27\nSubtotal RM 79.72\nSST 6% 4.78\nTOTAL MYR 84.50\n".into(),
    )
    .unwrap();
    let e = s.expense(&id).unwrap();
    assert_eq!(e.total_amount_minor, Some(8450));
    assert_eq!(e.tax_amount_minor, Some(478));
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.status, ExpenseStatus::NeedsReview);
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
    let mut change = edit(&s, &id);
    change.currency = Some("USD".into());
    assert!(s.edit_expense(change).is_err());
    s.transition_claim(&claim.id, ClaimStatus::Submitted)
        .unwrap();
    assert_eq!(s.expense(&id).unwrap().status, ExpenseStatus::Submitted);
    assert!(s.edit_expense(edit(&s, &id)).is_err());
    s.transition_claim(&claim.id, ClaimStatus::Draft).unwrap();
    s.edit_expense(edit(&s, &id)).unwrap();
    let d = s.set_claim_expense(&claim.id, &id, false).unwrap();
    assert_eq!(d.claim.total_amount_minor, 0);
    assert!(s.request_pdf(&claim.id).is_err());
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
        .complete_ocr(&j.job.id, &j.token, "text".into())
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
    assert_eq!(manual_cells[8], "20.00");
    let receipted_cells: Vec<&str> = lines[2].split(',').collect();
    assert_eq!(receipted_cells[2], "R002");
    assert_eq!(receipted_cells[4], "Receipted Cafe");
    assert_eq!(receipted_cells[8], "10.00");
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
    assert!(text.contains(",JPY,1500,0,"));
    assert!(text.contains(",BHD,12.345,0.005,"));
    let dedup = s
        .export_expenses_csv(vec![jpy_id.clone(), jpy_id.clone()])
        .unwrap();
    assert_eq!(dedup.rows, 1);
    assert!(s.export_expenses_csv(vec![]).is_err());
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
}
