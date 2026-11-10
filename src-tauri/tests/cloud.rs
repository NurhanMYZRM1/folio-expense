//! Accounts, device sync and recovery, driven through the same commands the
//! apps send, against a local stand-in for the Supabase project (mocked cloud:
//! see `support/fake_cloud.rs`). The SQL contract itself is verified separately
//! by `supabase/tests/run-cloud-security.py`.
#[path = "support/fake_cloud.rs"]
mod fake_cloud;
use fake_cloud::{FakeCloud, Fault};
use folio::{
    error::{AppError, Result},
    security::secrets::SecretStore,
    services::cloud_transport::{CloudConfig, Transport, CALLBACK},
    workspaces::Workspaces,
};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
};
use tempfile::TempDir;
use zeroize::Zeroizing;

const TABLES: [&str; 22] = [
    "receipt_files",
    "expenses",
    "expense_claims",
    "claim_expenses",
    "extraction_runs",
    "audit_events",
    "exchange_rates",
    "workspaces",
    "categories",
    "accounts",
    "bills",
    "transactions",
    "budgets",
    "budget_alerts",
    "transaction_suggestions",
    "business_profiles",
    "customers",
    "inventory_items",
    "invoices",
    "stock_movements",
    "invoice_payments",
    "receipt_item_reviews",
];

/// One device's credential store, kept in memory.
type Keychain = Arc<Mutex<HashMap<String, String>>>;
struct Slot(Keychain, String);
impl SecretStore for Slot {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&self.1)
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(self.1.clone(), value.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        self.0.lock().unwrap().remove(&self.1);
        Ok(())
    }
}

/// An installed copy of the app: its own data folder and credential store.
struct Device {
    dir: TempDir,
    keychain: Keychain,
    cloud: Arc<FakeCloud>,
    app: Workspaces,
}
impl Device {
    fn new(cloud: &Arc<FakeCloud>) -> Self {
        Self::launch(
            tempfile::tempdir().unwrap(),
            Keychain::default(),
            cloud.clone(),
        )
    }
    fn launch(dir: TempDir, keychain: Keychain, cloud: Arc<FakeCloud>) -> Self {
        let slots = keychain.clone();
        let transport = Transport::with_vaults(
            CloudConfig {
                environment: "test".into(),
                url: cloud.url.clone(),
                key: "sb_publishable_test".into(),
                callback: CALLBACK.into(),
            },
            Arc::new(move |account| {
                Ok(Box::new(Slot(slots.clone(), account.to_string())) as Box<dyn SecretStore>)
            }),
        )
        .unwrap();
        let app = Workspaces::open_with_cloud(
            dir.path().join("expense-app"),
            Arc::new(Slot(keychain.clone(), "vision-key".into())),
            Arc::new(transport),
        )
        .unwrap();
        Self {
            dir,
            keychain,
            cloud,
            app,
        }
    }
    /// Quits and reopens the app; data and stored credentials survive.
    fn relaunch(self) -> Self {
        let Self {
            dir,
            keychain,
            cloud,
            app,
        } = self;
        drop(app);
        Self::launch(dir, keychain, cloud)
    }
    fn try_call(&self, command: &str, args: Value) -> Result<Value> {
        self.app.dispatch(command, args)
    }
    fn call(&self, command: &str, args: Value) -> Value {
        self.try_call(command, args)
            .unwrap_or_else(|e| panic!("{command}: {}", e.message))
    }
    fn sign_in(&self, email: &str) -> Value {
        self.call(
            "cloud_sign_in",
            json!({"email":email,"password":format!("pw-{email}")}),
        )
    }
    /// A sync the person asked for; never held back by backoff.
    fn sync(&self) -> Value {
        self.call("cloud_sync", json!({"force":true}))
    }
    fn status(&self) -> Value {
        self.call("cloud_status", json!({}))
    }
    fn root(&self) -> PathBuf {
        PathBuf::from(
            self.call("workspace_info", json!({}))["root"]
                .as_str()
                .unwrap(),
        )
    }
    fn db(&self) -> Connection {
        Connection::open_with_flags(
            self.root().join("database/expenses.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap()
    }
    fn count(&self, sql: &str) -> i64 {
        self.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }
    /// Every synchronized table, row for row.
    fn records(&self) -> Vec<(String, Vec<String>)> {
        let db = self.db();
        TABLES
            .iter()
            .map(|table| {
                let mut q = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
                let n = q.column_count();
                let mut rows: Vec<String> = q
                    .query_map([], |r| {
                        Ok((0..n)
                            .map(|i| match r.get_ref(i).unwrap() {
                                ValueRef::Null => "∅".to_string(),
                                ValueRef::Integer(v) => v.to_string(),
                                ValueRef::Real(v) => v.to_string(),
                                ValueRef::Text(v) => String::from_utf8_lossy(v).to_string(),
                                ValueRef::Blob(v) => format!("{v:?}"),
                            })
                            .collect::<Vec<_>>()
                            .join("│"))
                    })
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                rows.sort();
                (table.to_string(), rows)
            })
            .collect()
    }
    fn expense(&self, merchant: &str, minor: i64) -> Value {
        let created = self.call("create_expense", json!({}));
        self.edit(&created, merchant, minor)
    }
    fn edit(&self, expense: &Value, merchant: &str, minor: i64) -> Value {
        self.call(
            "edit_expense",
            json!({"edit":{"id":expense["id"],"version":expense["version"],"occurredAt":"2026-11-01",
                "merchantName":merchant,"premises":null,"totalAmountMinor":minor,"taxAmountMinor":0,
                "currency":"MYR","category":"Meals","description":"","markReady":true}}),
        )
    }
    fn expenses(&self) -> Vec<Value> {
        self.call("list_expenses", json!({}))
            .as_array()
            .unwrap()
            .clone()
    }
    fn merchants(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .expenses()
            .iter()
            .map(|e| e["merchantName"].as_str().unwrap_or_default().to_string())
            .collect();
        names.sort();
        names
    }
    /// Imports a receipt image and finishes its reading, as the job runner would.
    fn import(&self, shade: u8) -> String {
        let path = self.dir.path().join(format!("receipt-{shade}.png"));
        image::RgbImage::from_pixel(40, 70, image::Rgb([shade, 200, 200]))
            .save(&path)
            .unwrap();
        let outcome = self.call("import_receipts", json!({"paths":[path]}));
        let id = outcome[0]["expenseId"]
            .as_str()
            .expect("imported")
            .to_string();
        loop {
            let lease = self.call("take_job", json!({}));
            if lease.is_null() {
                break;
            }
            let args = json!({"id":lease["job"]["id"],"token":lease["token"]});
            if lease["job"]["jobType"] == "extract_receipt" {
                let mut done = args.clone();
                done["rawText"] = json!("KEDAI KOPI\nTOTAL RM 12.50");
                done["ocrConfidence"] = json!(90.0);
                self.call("complete_ocr", done);
            } else {
                let mut failed = args.clone();
                failed["message"] = json!("not part of this test");
                self.call("fail_job", failed);
            }
        }
        id
    }
    fn receipt_bytes(&self) -> Vec<(String, Vec<u8>)> {
        let db = self.db();
        let mut q = db
            .prepare("SELECT relative_path FROM receipt_files ORDER BY sha256")
            .unwrap();
        let paths: Vec<String> = q
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        paths
            .into_iter()
            .map(|p| {
                let bytes = std::fs::read(self.root().join(&p)).unwrap_or_default();
                (p, bytes)
            })
            .collect()
    }
}

fn message(result: Result<Value>) -> String {
    result
        .err()
        .map(|e: AppError| e.message)
        .unwrap_or_default()
}

#[test]
fn two_devices_converge_after_create_edit_delete_and_offline_edits() {
    let cloud = FakeCloud::start();
    let user = cloud.user("owner@example.com", "email");
    let (mac, phone) = (Device::new(&cloud), Device::new(&cloud));
    assert_eq!(mac.sign_in("owner@example.com")["outcome"], "signedIn");
    let lunch = mac.expense("Lunch", 1250);
    let taxi = mac.expense("Taxi", 3000);
    let claim = mac.call("create_claim", json!({"title":"November","currency":"MYR"}));
    mac.call(
        "set_claim_expense",
        json!({"claimId":claim["id"],"expenseId":lunch["id"],"add":true}),
    );
    assert_eq!(mac.sync()["state"], "upToDate");

    assert_eq!(phone.sign_in("owner@example.com")["outcome"], "signedIn");
    assert_eq!(phone.sync()["state"], "upToDate");
    assert_eq!(phone.records(), mac.records(), "first pull matches the Mac");

    // Edit on the phone, delete on the Mac, both while the other is away.
    let on_phone = phone
        .expenses()
        .into_iter()
        .find(|e| e["id"] == lunch["id"])
        .unwrap();
    phone.edit(&on_phone, "Lunch with client", 4580);
    mac.call("delete_expenses", json!({"ids":[taxi["id"]]}));
    phone.sync();
    mac.sync();
    phone.sync();
    assert_eq!(mac.merchants(), ["Lunch with client"]);
    assert_eq!(phone.records(), mac.records());
    assert_eq!(
        cloud.records(&user, "expenses").len(),
        1,
        "the delete is a tombstone, not a row"
    );

    // Offline: the edit is kept, reported honestly, and uploaded on reconnection.
    cloud.set_offline(true);
    let current = mac.expenses().remove(0);
    mac.edit(&current, "Lunch with client (corrected)", 4600);
    let offline = mac.sync();
    assert_eq!(offline["state"], "offline");
    assert_eq!(offline["failure"], "CloudRetry");
    assert!(offline["pending"].as_i64().unwrap() > 0);
    assert_eq!(mac.merchants(), ["Lunch with client (corrected)"]);
    // Automatic attempts wait out the backoff instead of hammering a dead connection.
    let before = cloud.requests("/");
    mac.call("cloud_sync", json!({}));
    cloud.set_offline(false);
    mac.call("cloud_sync", json!({}));
    assert_eq!(cloud.requests("/"), before, "backoff holds automatic syncs");
    assert_eq!(mac.sync()["state"], "upToDate");
    phone.sync();
    assert_eq!(phone.records(), mac.records());
    let total: i64 = phone.count("SELECT total_amount_minor FROM expenses");
    assert_eq!(total, 4600, "the amount is exactly what was entered");
    assert_eq!(mac.status()["conflicts"], 0);
}

#[test]
fn a_lost_acknowledgement_never_loses_duplicates_or_conflicts_an_edit() {
    let cloud = FakeCloud::start();
    let user = cloud.user("owner@example.com", "email");
    let mac = Device::new(&cloud);
    mac.sign_in("owner@example.com");
    let expense = mac.expense("Coffee", 900);
    // The server applies the upload but its reply never arrives.
    cloud.fault("folio_sync_push", Fault::DropReply);
    let lost = mac.sync();
    assert_eq!(lost["failure"], "CloudRetry");
    assert!(
        lost["pending"].as_i64().unwrap() > 0,
        "nothing is assumed delivered"
    );
    assert_eq!(cloud.records(&user, "expenses").len(), 1);
    // The person keeps editing before the next sync.
    let current = mac.expenses().remove(0);
    mac.edit(&current, "Coffee and cake", 1700);
    let after = mac.sync();
    assert_eq!(after["state"], "upToDate", "{after}");
    assert_eq!(
        after["conflicts"], 0,
        "the device's own change is not a conflict"
    );
    let remote = cloud.records(&user, "expenses");
    assert_eq!(remote.len(), 1, "replayed, not duplicated");
    assert_eq!(remote[0]["data"]["merchant_name"], "Coffee and cake");
    assert_eq!(remote[0]["data"]["total_amount_minor"], 1700);
    assert_eq!(remote[0]["key"]["id"], expense["id"]);
    // Sending the very same operation again is answered from its receipt.
    cloud.fault("folio_sync_push", Fault::DropReply);
    let current = mac.expenses().remove(0);
    mac.edit(&current, "Coffee, cake and tea", 2100);
    mac.sync();
    assert_eq!(mac.sync()["state"], "upToDate");
    assert_eq!(cloud.records(&user, "expenses")[0]["revision"], 3);
}

#[test]
fn sessions_refresh_once_survive_restart_and_pause_safely_when_revoked() {
    let cloud = FakeCloud::start();
    let user = cloud.user("owner@example.com", "email");
    let mut mac = Device::new(&cloud);
    mac.sign_in("owner@example.com");
    mac.expense("Before", 100);
    mac.sync();

    // An expired access token is renewed by one coordinated refresh.
    cloud.expire_access();
    mac.expense("After expiry", 200);
    assert_eq!(mac.sync()["state"], "upToDate");
    assert_eq!(cloud.requests("/auth/v1/token refresh_token"), 1);

    // The refresh reply is lost: it is retried at once and the rotated credential is kept.
    cloud.expire_access();
    cloud.fault("/auth/v1/token", Fault::DropReply);
    assert_eq!(mac.sync()["state"], "upToDate");
    cloud.expire_access();
    assert_eq!(
        mac.sync()["state"],
        "upToDate",
        "the rotated credential works"
    );

    // Relaunch: the session is restored from the credential store, not from the database.
    mac = mac.relaunch();
    assert_eq!(mac.status()["signedIn"], true);
    assert_eq!(mac.sync()["state"], "upToDate");
    let leaked: i64 = mac.count(
        "SELECT count(*) FROM cloud_control WHERE coalesce(email,'')||coalesce(last_error,'')||coalesce(providers,'') LIKE '%r-%-%-%' OR coalesce(last_error,'') LIKE '%a-%-%-%'",
    );
    assert_eq!(leaked, 0, "no token is written to SQLite");

    // A busy server must not sign anyone out.
    cloud.expire_access();
    cloud.fault("/auth/v1/token", Fault::Status(503, json!({}), None));
    cloud.fault("/auth/v1/token", Fault::Status(503, json!({}), None));
    let busy = mac.sync();
    assert_eq!(
        (busy["signedIn"].clone(), busy["state"].clone()),
        (json!(true), json!("offline"))
    );

    // Revoked: sync pauses and asks for sign-in; the pending edit is untouched.
    cloud.revoke(&user);
    mac.expense("While revoked", 300);
    let paused = mac.sync();
    assert_eq!(paused["state"], "signInAgain", "{paused}");
    assert_eq!(paused["signedIn"], false);
    assert!(paused["pending"].as_i64().unwrap() > 0);
    assert_eq!(mac.merchants(), ["After expiry", "Before", "While revoked"]);
    let before = cloud.requests("/");
    mac.call("cloud_sync", json!({}));
    assert_eq!(
        cloud.requests("/"),
        before,
        "no retry loop while signed out"
    );
    // Signing in again resumes exactly where it stopped.
    assert_eq!(mac.sign_in("owner@example.com")["outcome"], "signedIn");
    assert_eq!(mac.sync()["state"], "upToDate");
    assert_eq!(cloud.records(&user, "expenses").len(), 3);
}

#[test]
fn rate_limits_refusals_and_rejections_are_classified_and_never_storm() {
    let cloud = FakeCloud::start();
    cloud.user("owner@example.com", "email");
    let mac = Device::new(&cloud);
    mac.sign_in("owner@example.com");
    mac.expense("Pending", 100);
    for (status, header, failure, state) in [
        (429, Some(("Retry-After", "120")), "CloudQuota", "offline"),
        (403, None, "CloudPermission", "needsAttention"),
        (400, None, "CloudValidation", "needsAttention"),
        (500, None, "CloudRetry", "offline"),
    ] {
        cloud.fault(
            "folio_sync_pull",
            Fault::Status(status, json!({"message":"reason from the server"}), header),
        );
        let result = mac.sync();
        assert_eq!(result["failure"], failure, "{status}");
        assert_eq!(result["state"], state, "{status}");
        assert_eq!(result["requestId"], format!("req-{status}"));
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("reason from the server"));
        assert!(
            result["pending"].as_i64().unwrap() > 0,
            "pending work is never dropped"
        );
        let wait = chrono::DateTime::parse_from_rfc3339(result["retryAt"].as_str().unwrap())
            .unwrap()
            .signed_duration_since(chrono::Utc::now())
            .num_seconds();
        if status == 429 {
            assert!(
                (110..=121).contains(&wait),
                "waits as long as the server asked: {wait}"
            );
        }
        let before = cloud.requests("/");
        mac.call("cloud_sync", json!({}));
        assert_eq!(
            cloud.requests("/"),
            before,
            "{status} is not retried in a loop"
        );
    }
    assert_eq!(mac.sync()["state"], "upToDate");
    let diagnosis = mac.call("cloud_diagnose", json!({}));
    assert_eq!(diagnosis["reachable"], "ok");
    assert_eq!(diagnosis["authenticated"], "ok");
    assert_eq!(diagnosis["keyKind"], "publishable");
    assert_eq!(diagnosis["environment"], "test");
    let text = diagnosis.to_string();
    assert!(!text.contains("a-") || !text.contains("Bearer"), "{text}");
    assert!(
        !text.contains("sb_publishable"),
        "the key itself is not shown"
    );
}

#[test]
fn a_reply_that_arrives_after_sign_out_changes_nothing() {
    let cloud = FakeCloud::start();
    cloud.user("owner@example.com", "email");
    let (mac, phone) = (Device::new(&cloud), Arc::new(Device::new(&cloud)));
    mac.sign_in("owner@example.com");
    mac.expense("From the Mac", 100);
    mac.sync();
    phone.sign_in("owner@example.com");
    // The phone's first pull is answered only after the person has signed out.
    let (release, held) = mpsc::channel();
    cloud.fault("folio_sync_pull", Fault::Hold(held));
    let syncing = {
        let phone = phone.clone();
        thread::spawn(move || phone.call("cloud_sync", json!({"force":true})))
    };
    while cloud.requests("folio_sync_pull") < 2 {
        thread::yield_now();
    }
    let signing_out = {
        let phone = phone.clone();
        thread::spawn(move || phone.call("cloud_sign_out", json!({})))
    };
    thread::sleep(std::time::Duration::from_millis(150));
    release.send(()).unwrap();
    syncing.join().unwrap();
    let status = signing_out.join().unwrap();
    assert_eq!(status["signedIn"], false);
    assert!(phone.expenses().is_empty(), "the late page was discarded");
    assert_eq!(phone.count("SELECT cursor FROM cloud_control"), 0);
    assert!(
        status["error"].is_null(),
        "a cancelled sync is not an error"
    );
}

#[test]
fn two_accounts_on_one_device_never_share_records_files_or_jobs() {
    let cloud = FakeCloud::start();
    let (ana, ben) = (
        cloud.user("ana@example.com", "email"),
        cloud.user("ben@example.com", "email"),
    );
    let device = Device::new(&cloud);
    assert_eq!(device.sign_in("ana@example.com")["outcome"], "signedIn");
    let ana_root = device.root();
    device.import(10);
    device.expense("Ana's dinner", 5000);
    // A receipt job Ana's session started and has not finished.
    let path = device.dir.path().join("late.png");
    image::RgbImage::from_pixel(40, 70, image::Rgb([77, 1, 1]))
        .save(&path)
        .unwrap();
    device.call("import_receipts", json!({"paths":[path]}));
    let late_job = device.call("take_job", json!({}));
    assert!(!late_job.is_null());

    // Ben signs in on the same device and gets his own empty workspace.
    let switched = device.sign_in("ben@example.com");
    assert_eq!(switched["outcome"], "switched");
    assert_eq!(switched["status"]["email"], "ben@example.com");
    assert_ne!(device.root(), ana_root);
    assert!(
        device.expenses().is_empty(),
        "Ben sees none of Ana's expenses"
    );
    assert_eq!(device.count("SELECT count(*) FROM receipt_files"), 0);
    // Ana's unfinished job cannot complete into Ben's workspace.
    let late = device.try_call(
        "complete_ocr",
        json!({"id":late_job["job"]["id"],"token":late_job["token"],"rawText":"TOTAL 1.00","ocrConfidence":90.0}),
    );
    assert!(late.is_err());
    assert!(device.expenses().is_empty());
    device.expense("Ben's taxi", 700);
    device.import(20);
    assert_eq!(device.sync()["state"], "upToDate");

    // The cloud holds each account's records and files under that account only.
    assert_eq!(cloud.records(&ben, "expenses").len(), 2);
    assert!(
        cloud.records(&ana, "expenses").is_empty(),
        "Ana never synced; nothing leaked upward"
    );
    assert!(cloud
        .object_paths()
        .iter()
        .all(|p| p.starts_with(&format!("folio-receipts/{ben}/"))));
    assert!(!cloud.object_paths().is_empty());

    // Ana returns: her workspace is as she left it, and syncs to her account.
    assert_eq!(device.sign_in("ana@example.com")["outcome"], "switched");
    assert_eq!(device.root(), ana_root);
    assert!(device.merchants().contains(&"Ana's dinner".to_string()));
    assert!(!device.merchants().contains(&"Ben's taxi".to_string()));
    // Her unfinished job is still hers to finish; until it settles, upload waits for it.
    device.call("cloud_sync", json!({"force":true}));
    assert!(
        cloud.records(&ana, "expenses").is_empty(),
        "a half-read receipt is not uploaded"
    );
    device.call(
        "fail_job",
        json!({"id":late_job["job"]["id"],"token":late_job["token"],"message":"stopped for this test"}),
    );
    loop {
        let lease = device.call("take_job", json!({}));
        if lease.is_null() {
            break;
        }
        device.call(
            "fail_job",
            json!({"id":lease["job"]["id"],"token":lease["token"],"message":"stopped for this test"}),
        );
    }
    assert_eq!(device.sync()["state"], "upToDate");
    assert!(cloud
        .records(&ana, "expenses")
        .iter()
        .any(|e| e["data"]["merchant_name"] == "Ana's dinner"));
    assert!(cloud
        .records(&ben, "expenses")
        .iter()
        .all(|e| e["data"]["merchant_name"] != "Ana's dinner"));
    let (ana_files, ben_files): (Vec<_>, Vec<_>) = cloud
        .object_paths()
        .into_iter()
        .partition(|p| p.starts_with(&format!("folio-receipts/{ana}/")));
    assert!(!ana_files.is_empty());
    assert!(ben_files
        .iter()
        .all(|p| p.starts_with(&format!("folio-receipts/{ben}/"))));

    // The choice of account survives a restart, and each keeps its own session.
    let device = device.relaunch();
    assert_eq!(device.root(), ana_root);
    assert_eq!(device.status()["email"], "ana@example.com");
    assert_eq!(
        device.keychain.lock().unwrap().len(),
        2,
        "one credential slot per account"
    );
}

#[test]
fn records_already_on_a_device_join_an_account_only_when_chosen() {
    let cloud = FakeCloud::start();
    let user = cloud.user("owner@example.com", "email");
    let mut phone = Device::new(&cloud);
    phone.expense("Bought before any account", 4200);
    let original = phone.root();

    let asked = phone.sign_in("owner@example.com");
    assert_eq!(asked["outcome"], "chooseData");
    assert_eq!(asked["email"], "owner@example.com");
    let waiting = phone.status();
    assert_eq!(
        (waiting["signedIn"].clone(), waiting["linked"].clone()),
        (json!(false), json!(false))
    );
    assert!(
        phone.keychain.lock().unwrap().is_empty(),
        "no session is stored before the choice"
    );
    assert!(
        cloud.records(&user, "expenses").is_empty(),
        "nothing is uploaded before the choice"
    );

    // The app is closed before choosing: nothing happened, and it can simply be done again.
    phone = phone.relaunch();
    assert!(message(phone.try_call("cloud_adopt", json!({"choice":"join"}))).contains("expired"));
    assert_eq!(phone.sign_in("owner@example.com")["outcome"], "chooseData");

    // Keeping it separate opens the account's own workspace and leaves the records alone.
    assert_eq!(
        phone.call("cloud_adopt", json!({"choice":"separate"}))["outcome"],
        "switched"
    );
    assert!(phone.expenses().is_empty());
    assert_ne!(phone.root(), original);
    phone.sync();
    assert!(cloud.records(&user, "expenses").is_empty());
    let kept = Connection::open_with_flags(
        original.join("database/expenses.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let owner: Option<String> = kept
        .query_row("SELECT owner_id FROM cloud_control", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        owner, None,
        "the original records still belong to no account"
    );
    assert_eq!(
        kept.query_row("SELECT merchant_name FROM expenses", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "Bought before any account"
    );

    // On another device the person chooses to add what is there to the account.
    let mut tablet = Device::new(&cloud);
    tablet.expense("Tablet purchase", 9900);
    assert_eq!(tablet.sign_in("owner@example.com")["outcome"], "chooseData");
    assert_eq!(
        tablet.call("cloud_adopt", json!({"choice":"join"}))["outcome"],
        "signedIn"
    );
    let backups = std::fs::read_dir(tablet.root().join("database"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("before-account-")
        })
        .count();
    assert_eq!(
        backups, 1,
        "a snapshot is taken before records join an account"
    );
    // The first upload is interrupted and the app closed; it resumes without duplicates.
    cloud.fault("folio_sync_push", Fault::DropReply);
    tablet.sync();
    tablet = tablet.relaunch();
    assert_eq!(tablet.sync()["state"], "upToDate");
    let uploaded = cloud.records(&user, "expenses");
    assert_eq!(uploaded.len(), 1);
    assert_eq!(uploaded[0]["data"]["total_amount_minor"], 9900);
    assert_eq!(tablet.merchants(), ["Tablet purchase"]);
    // Declining entirely leaves a third device exactly as it was.
    let other = Device::new(&cloud);
    other.expense("Stays local", 1);
    other.sign_in("owner@example.com");
    assert_eq!(
        other.call("cloud_adopt", json!({"choice":"cancel"}))["outcome"],
        "cancelled"
    );
    assert_eq!(other.status()["state"], "local");
    assert_eq!(other.merchants(), ["Stays local"]);
}

#[test]
fn google_and_apple_sign_in_use_the_system_browser_and_keep_the_owner() {
    let cloud = FakeCloud::start();
    let owner = cloud.user("owner@example.com", "email");
    let (mac, phone) = (Device::new(&cloud), Device::new(&cloud));
    // The existing owner signs in the way they always have, by emailed code.
    mac.call("cloud_request_code", json!({"email":"owner@example.com"}));
    assert_eq!(
        mac.call(
            "cloud_verify_code",
            json!({"email":"owner@example.com","code":"123456"})
        )["outcome"],
        "signedIn"
    );
    mac.expense("Owner's record", 1500);
    mac.sync();

    // Adding Apple is started by the signed-in owner, which is the proof of ownership.
    let start = mac.call("cloud_oauth_start", json!({"provider":"apple","link":true}));
    let callback = cloud.browser_sign_in(
        start["url"].as_str().unwrap(),
        "relay@privaterelay.appleid.com",
    );
    let linked = mac.call("cloud_oauth_finish", json!({"url":callback}));
    assert_eq!(linked["outcome"], "linked");
    assert_eq!(linked["status"]["providers"], json!(["email", "apple"]));
    assert_eq!(linked["status"]["email"], "owner@example.com");
    // A repeated callback is refused and changes nothing.
    assert!(
        message(mac.try_call("cloud_oauth_finish", json!({"url":callback})))
            .contains("already finished")
    );
    assert_eq!(mac.status()["signedIn"], true);

    // On the phone, Apple now signs in to the same account: same records, no merge by email.
    let start = phone.call("cloud_oauth_start", json!({"provider":"apple"}));
    let address = start["url"].as_str().unwrap();
    assert!(address.contains("redirect_to=expenseapp%3A%2F%2Fauth%2Fcallback"));
    // A receipt link that arrives meanwhile is not a sign-in and does not end the attempt.
    assert!(message(phone.try_call(
        "cloud_oauth_finish",
        json!({"url":"expenseapp://receipt/123"})
    ))
    .contains("not a Folio sign-in"));
    // The provider reports a cancellation: nothing is signed in.
    let cancelled =
        "expenseapp://auth/callback?error=access_denied&error_description=The+user+cancelled";
    assert!(
        message(phone.try_call("cloud_oauth_finish", json!({"url":cancelled})))
            .contains("cancelled")
    );
    assert_eq!(phone.status()["state"], "local");
    // A code issued for the Mac's browser session cannot be redeemed by the phone.
    let stolen = cloud.browser_sign_in(
        mac.call("cloud_oauth_start", json!({"provider":"apple"}))["url"]
            .as_str()
            .unwrap(),
        "relay@privaterelay.appleid.com",
    );
    phone.call("cloud_oauth_start", json!({"provider":"apple"}));
    assert!(phone
        .try_call("cloud_oauth_finish", json!({"url":stolen}))
        .is_err());
    assert_eq!(phone.status()["signedIn"], false);
    mac.call("cloud_oauth_cancel", json!({}));

    let start = phone.call("cloud_oauth_start", json!({"provider":"apple"}));
    let callback = cloud.browser_sign_in(
        start["url"].as_str().unwrap(),
        "relay@privaterelay.appleid.com",
    );
    assert_eq!(
        phone.call("cloud_oauth_finish", json!({"url":callback}))["outcome"],
        "signedIn"
    );
    phone.sync();
    assert_eq!(phone.merchants(), ["Owner's record"]);
    assert_eq!(cloud.records(&owner, "expenses").len(), 1);

    // A different Google identity is a different account: it gets its own workspace.
    let start = mac.call("cloud_oauth_start", json!({"provider":"google"}));
    let callback = cloud.browser_sign_in(start["url"].as_str().unwrap(), "someone.else@gmail.com");
    let other = mac.call("cloud_oauth_finish", json!({"url":callback}));
    assert_eq!(other["outcome"], "switched");
    assert_eq!(other["status"]["providers"], json!(["google"]));
    assert!(
        mac.expenses().is_empty(),
        "the owner's records are not shown to another account"
    );
    mac.sync();
    assert_eq!(
        cloud.records(&owner, "expenses").len(),
        1,
        "and are not re-homed"
    );
    // Linking can never be used to attach to somebody else's account.
    let start = mac.call("cloud_oauth_start", json!({"provider":"apple","link":true}));
    assert!(start["url"].as_str().unwrap().contains("link_user="));
    assert!(
        message(mac.try_call("cloud_oauth_start", json!({"provider":"facebook"})))
            .contains("Google or Apple")
    );
}

#[test]
fn the_same_receipt_imported_on_two_devices_becomes_one_reviewed_record() {
    let cloud = FakeCloud::start();
    let user = cloud.user("owner@example.com", "email");
    let (mac, phone) = (Device::new(&cloud), Device::new(&cloud));
    mac.sign_in("owner@example.com");
    phone.sign_in("owner@example.com");
    // Both import the same photo before either has synced.
    let mac_expense = mac.import(42);
    let phone_expense = phone.import(42);
    assert_ne!(mac_expense, phone_expense);
    let on_phone = phone.expenses().remove(0);
    phone.edit(&on_phone, "Corrected on the phone", 1250);
    assert_eq!(mac.sync()["state"], "upToDate");

    // The phone keeps its values as a candidate against the cloud's record.
    let held = phone.sync();
    assert_eq!(held["conflicts"], 1, "{held}");
    assert_eq!(phone.count("SELECT count(*) FROM receipt_files"), 1);
    assert_eq!(phone.expenses().len(), 1);
    assert_eq!(
        phone.expenses()[0]["id"],
        mac_expense,
        "the cloud's identity is adopted"
    );
    assert_eq!(
        phone.merchants(),
        ["Corrected on the phone"],
        "the local correction is intact"
    );
    let (path, bytes) = phone.receipt_bytes().remove(0);
    assert!(
        !bytes.is_empty(),
        "the original file followed its record: {path}"
    );
    let conflict = phone.call("cloud_conflicts", json!({}))[0].clone();
    assert_eq!(conflict["entity"], "expenses");
    assert_eq!(conflict["local"]["merchant_name"], "Corrected on the phone");

    phone.call(
        "cloud_resolve_conflict",
        json!({"id":conflict["id"],"choice":"local"}),
    );
    assert_eq!(phone.sync()["state"], "upToDate");
    assert_eq!(mac.sync()["state"], "upToDate");
    assert_eq!(mac.merchants(), ["Corrected on the phone"]);
    assert_eq!(phone.records(), mac.records());
    assert_eq!(cloud.records(&user, "receipt_files").len(), 1);
    assert_eq!(cloud.records(&user, "expenses").len(), 1);
    assert_eq!(phone.receipt_bytes(), mac.receipt_bytes());
    // The folder left behind by the phone's first import is removed on the next launch.
    let phone = phone.relaunch();
    assert_eq!(
        std::fs::read_dir(phone.root().join("receipts"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn an_interrupted_receipt_download_retries_without_blocking_other_files() {
    let cloud = FakeCloud::start();
    cloud.user("owner@example.com", "email");
    let (mac, mut phone) = (Device::new(&cloud), Device::new(&cloud));
    mac.sign_in("owner@example.com");
    mac.import(1);
    mac.import(2);
    assert_eq!(mac.sync()["state"], "upToDate");
    phone.sign_in("owner@example.com");
    // The connection drops halfway through the first file.
    cloud.fault("/storage/v1/object/", Fault::Truncate);
    let partial = phone.sync();
    assert_eq!(partial["failure"], "CloudRetry", "{partial}");
    assert_eq!(
        phone.records(),
        mac.records(),
        "records arrive before their files"
    );
    let files = phone.receipt_bytes();
    assert_eq!(
        files.iter().filter(|(_, b)| !b.is_empty()).count(),
        1,
        "the other file still arrived"
    );
    assert!(
        files
            .iter()
            .all(|(_, b)| b.is_empty() || mac.receipt_bytes().iter().any(|(_, m)| m == b)),
        "no partial file is ever left in place"
    );
    // The app closes mid-recovery; startup cleanup must not disturb what is valid.
    phone = phone.relaunch();
    assert_eq!(
        phone
            .receipt_bytes()
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .count(),
        1
    );
    assert_eq!(phone.sync()["state"], "upToDate");
    assert_eq!(
        phone.receipt_bytes(),
        mac.receipt_bytes(),
        "byte-identical originals"
    );
    assert!(
        std::fs::read_dir(phone.root().join("cache"))
            .unwrap()
            .count()
            == 0
    );
}
