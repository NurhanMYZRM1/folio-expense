//! Upgrading a data folder written by the pre-finance release (schema v3,
//! `main` @ 126d459). The fixture was produced by that release's own test
//! driver: a synthetic image with a supplied OCR result, edited and marked
//! ready, a submitted claim with a minimal PDF and real CSV export, a manual
//! draft and settings. The fixture itself does not verify OCR or PDF rendering.
use folio::{
    dispatch::dispatch, error::Result, security::secrets::SecretStore, services::AppService,
};
use rusqlite::{types::Value as Sql, Connection};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc};
use zeroize::Zeroizing;
struct Secrets;
impl SecretStore for Secrets {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        Ok(None)
    }
    fn set(&self, _: &str) -> Result<()> {
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        Ok(())
    }
}
const RECEIPT: &str = "4af62a37-46e7-40f4-a38f-90f49b87a43f";
const EXPENSE: &str = "1b8a4adf-f332-4261-836c-0e22efdf6a8c";
const DRAFT: &str = "90e02a0b-ffe5-4d72-b785-d0a56135d3ae";
const CLAIM: &str = "a40d4a88-d098-4cd6-aabd-6f8a2ea56510";
/// Tables and columns as they existed at schema v3.
const V3: &[(&str, &str)] = &[
    ("receipt_files", "id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at,version"),
    ("expenses", "id,receipt_id,occurred_at,merchant_name,total_amount_minor,tax_amount_minor,currency,category,description,status,extraction_confidence,field_meta,created_at,updated_at,version,original_currency,original_total_amount_minor,original_tax_amount_minor,exchange_rate,exchange_rate_date,premises"),
    ("extraction_runs", "id,expense_id,provider,raw_text,result_json,error,created_at"),
    ("expense_claims", "id,claim_number,title,description,status,currency,created_at,updated_at,version"),
    ("claim_expenses", "claim_id,expense_id,created_at,updated_at,version"),
    ("audit_events", "id,event_type,entity_id,details,created_at"),
    ("jobs", "id,job_type,entity_id,payload,status,attempts,last_error,created_at,updated_at"),
    ("settings", "key,value,version,updated_at"),
];
fn rows(db: &Connection, table: &str, columns: &str) -> Vec<Vec<Sql>> {
    let n = columns.split(',').count();
    let mut q = db
        .prepare(&format!("SELECT {columns} FROM {table} ORDER BY 1,2"))
        .unwrap();
    q.query_map([], |r| (0..n).map(|i| r.get(i)).collect())
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn call(s: &AppService, c: &str, a: Value) -> Value {
    dispatch(s, c, a).unwrap()
}
#[test]
fn v3_data_folder_upgrades_without_losing_changing_or_uploading_anything() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v3-main"),
        &root,
    );
    let path = root.join("database/expenses.sqlite");
    let before: Vec<_> = {
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0))
                .unwrap(),
            3
        );
        V3.iter().map(|(t, c)| rows(&db, t, c)).collect()
    };
    let s = AppService::open(root.clone(), Arc::new(Secrets)).unwrap();

    // Every v3 row survives with the same ids and values, including audit history.
    let db = Connection::open(&path).unwrap();
    let version: i32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, folio::db::SCHEMA_VERSION);
    for ((table, columns), old) in V3.iter().zip(&before) {
        assert_eq!(
            &rows(&db, table, columns),
            old,
            "{table} changed during the upgrade"
        );
    }
    assert_eq!(before[5].len(), 12, "fixture audit history");

    // The upgraded data still works through the normal commands.
    let expense = call(&s, "get_expense", json!({ "id": EXPENSE }));
    assert_eq!(expense["merchantName"], "KOPI HOUSE");
    assert_eq!(expense["totalAmountMinor"], 8450);
    assert_eq!(expense["status"], "submitted");
    assert_eq!(
        call(&s, "get_expense", json!({ "id": DRAFT }))["status"],
        "draft"
    );
    let claim = call(&s, "get_claim", json!({ "id": CLAIM }));
    assert_eq!(claim["claim"]["status"], "submitted");
    assert_eq!(claim["claim"]["totalAmountMinor"], 8450);
    assert_eq!(claim["expenses"][0]["id"], EXPENSE);
    let receipt = call(&s, "read_receipt", json!({ "id": RECEIPT }));
    let original = fs::read(root.join(format!("receipts/{RECEIPT}/original.png"))).unwrap();
    use base64::Engine;
    let served = base64::engine::general_purpose::STANDARD
        .decode(receipt["base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        served, original,
        "the original receipt bytes are served unchanged"
    );
    for relative in [
        "receipts/4af62a37-46e7-40f4-a38f-90f49b87a43f/original.png",
        "exports/claims/dda5ea0e-f173-4ff2-b663-322918c6f121.pdf",
        "exports/csv/5ad91b9f-8a4b-42f0-be32-0fe1b6ba9bdf.csv",
    ] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/v3-main")
            .join(relative);
        assert_eq!(
            fs::read(root.join(relative)).unwrap(),
            fs::read(source).unwrap(),
            "{relative} changed during upgrade"
        );
    }
    assert_eq!(
        call(&s, "get_settings", json!({}))["company"],
        "Acme Sdn Bhd"
    );

    // Finance starts empty: one personal workspace, receipts are not recorded
    // into the ledger implicitly, and nothing is queued for upload.
    let workspaces = call(&s, "list_workspaces", json!({}));
    assert_eq!(workspaces.as_array().unwrap().len(), 1);
    assert_eq!(workspaces[0]["kind"], "personal");
    let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(count("SELECT count(*) FROM transactions"), 0);
    assert_eq!(count("SELECT count(*) FROM sync_outbox"), 0);
    assert_eq!(
        count("SELECT count(*) FROM sync_connections WHERE enabled=1"),
        0
    );

    // Opening again is a no-op: no duplicate workspace, same rows.
    drop(s);
    let again = AppService::open(root, Arc::new(Secrets)).unwrap();
    assert_eq!(
        call(&again, "list_workspaces", json!({}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for ((table, columns), old) in V3.iter().zip(&before) {
        assert_eq!(&rows(&db, table, columns), old, "{table} changed on reopen");
    }
}

#[test]
fn full_data_export_round_trips_the_live_database_receipts_and_reports() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v3-main"),
        &root,
    );
    let s = AppService::open(root.clone(), Arc::new(Secrets)).unwrap();
    let workspace = call(&s, "list_workspaces", json!({}))[0]["id"].clone();
    let account = call(
        &s,
        "save_account",
        json!({"input": {
            "workspaceId": workspace, "kind": "bank", "name": "Export test bank", "currency": "MYR",
            "openingBalanceMinor": 12345, "note": "", "archived": false
        }}),
    );
    fs::write(root.join("cache/do-not-export.txt"), "cache sentinel").unwrap();
    fs::write(root.join("logs/do-not-export.txt"), "log sentinel").unwrap();
    let export = call(&s, "export_all_data", json!({}));
    let path = export["path"].as_str().unwrap();
    assert_eq!(
        call(&s, "open_data_export", json!({"id": export["id"]})),
        path
    );
    assert!(dispatch(&s, "open_data_export", json!({"id": "../other"})).is_err());
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    assert_eq!(zip.len(), export["files"].as_u64().unwrap() as usize);
    let names: Vec<_> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    assert!(!names.iter().any(|n| n.starts_with("cache/")
        || n.starts_with("logs/")
        || n.starts_with("exports/data/")));
    let records: Value = serde_json::from_reader(zip.by_name("records.json").unwrap()).unwrap();
    assert_eq!(records["tables"]["accounts"][0]["id"], account["id"]);
    assert_eq!(
        records["tables"]["accounts"][0]["opening_balance_minor"],
        12345
    );
    assert_eq!(
        records["tables"]["audit_events"].as_array().unwrap().len(),
        12
    );
    assert_eq!(records["tables"]["expenses"].as_array().unwrap().len(), 2);
    assert_eq!(records["tables"]["sync_outbox"], json!([]));
    let restored = t.path().join("restored");
    zip.extract(&restored).unwrap();
    // Everything except the rebuilt database is copied byte-for-byte.
    for name in names
        .iter()
        .filter(|n| n.starts_with("receipts/") || n.starts_with("exports/"))
    {
        assert_eq!(
            fs::read(root.join(name)).unwrap(),
            fs::read(restored.join(name)).unwrap(),
            "{name}"
        );
    }
    let snapshot = Connection::open(restored.join("database/expenses.sqlite")).unwrap();
    assert_eq!(
        snapshot
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(!snapshot
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .exists([])
        .unwrap());
    let again = AppService::open(restored, Arc::new(Secrets)).unwrap();
    assert_eq!(
        call(&again, "get_expense", json!({"id": EXPENSE})),
        call(&s, "get_expense", json!({"id": EXPENSE}))
    );
    assert_eq!(
        call(&again, "get_claim", json!({"id": CLAIM})),
        call(&s, "get_claim", json!({"id": CLAIM}))
    );
    assert_eq!(
        call(&again, "list_accounts", json!({"workspaceId": workspace}))[0],
        account
    );
    let second = call(&s, "export_all_data", json!({}));
    assert_ne!(export["id"], second["id"]);
    let second_zip =
        zip::ZipArchive::new(fs::File::open(second["path"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(
        second_zip.len(),
        zip.len(),
        "previous archives must not be nested"
    );
}

#[test]
fn an_export_failure_leaves_no_archive_or_staging_data() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v3-main"),
        &root,
    );
    let s = AppService::open(root.clone(), Arc::new(Secrets)).unwrap();
    fs::remove_file(root.join(format!("receipts/{RECEIPT}/original.png"))).unwrap();
    assert_eq!(s.export_all_data().unwrap_err().code, "ExportError");
    assert_eq!(fs::read_dir(root.join("cache")).unwrap().count(), 0);
    assert!(!root.join("exports/data").exists());
}

#[cfg(unix)]
#[test]
fn data_export_refuses_symbolic_links_in_attachments() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("app");
    let s = AppService::open(root.clone(), Arc::new(Secrets)).unwrap();
    let outside = t.path().join("private.txt");
    fs::write(&outside, "must not be exported").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("receipts/linked.txt")).unwrap();
    assert!(s.export_all_data().is_err());
    assert_eq!(fs::read_dir(root.join("cache")).unwrap().count(), 0);
    assert!(!root.join("exports/data").exists());
}
