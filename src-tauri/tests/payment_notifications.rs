use folio::{
    dispatch::dispatch, error::Result, security::secrets::SecretStore, services::AppService,
};
use serde_json::json;
use zeroize::Zeroizing;
struct Vault;
impl SecretStore for Vault {
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
#[test]
fn payment_notification_is_opt_in_supported_deduplicated_and_reviewed() {
    let root = tempfile::tempdir().unwrap();
    let s = AppService::open(root.path().into(), std::sync::Arc::new(Vault)).unwrap();
    let input = json!({"packageName":"my.com.tngdigital.ewallet","notificationKey":"wallet|1","postedAt":1793851200000_i64,"text":"Payment successful: RM 12.50","occurredOn":"2026-11-05"});
    let invoke = |i: serde_json::Value| {
        dispatch(&s, "ingest_payment_notification", json!({"input":i})).unwrap()
    };
    assert!(invoke(input.clone()).is_null());
    let mut settings = s.settings().unwrap();
    settings.payment_alerts_enabled = true;
    s.save_settings(settings).unwrap();
    let first = invoke(input.clone());
    assert_eq!(first["amountMinor"], 1250);
    assert_eq!(first["status"], "pending");
    assert_eq!(invoke(input.clone())["id"], first["id"]);
    let mut unsupported = input.clone();
    unsupported["packageName"] = json!("com.chat.app");
    assert!(invoke(unsupported).is_null());
    for content in [
        "OTP RM12.50",
        "Payment failed RM12.50",
        "You received RM12.50",
        "Your balance RM12.50",
        "Payment successful RM12.50 Balance RM100",
    ] {
        let mut message = input.clone();
        message["text"] = json!(content);
        assert!(invoke(message).is_null());
    }
    let w = s.workspaces().unwrap().remove(0);
    let report = s.monthly_report(&w.id, "2026-11").unwrap();
    assert_eq!(report.expense_minor, 0);
    assert_eq!(s.suggestions(&w.id).unwrap().len(), 1);
}
