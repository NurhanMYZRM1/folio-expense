use folio::{
    domain::{
        exchange::ExchangeRate, expense::ExpenseEdit, extraction::FieldSource, receipt_items::*,
    },
    error::Result,
    security::secrets::SecretStore,
    services::{exchange_rates::RateSource, AppService},
};
use std::sync::Arc;
use zeroize::Zeroizing;
struct NoSecrets;
impl SecretStore for NoSecrets {
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
struct FixedRate;
impl RateSource for FixedRate {
    fn fetch(&self, base: &str, quote: &str, date: &str) -> Result<ExchangeRate> {
        Ok(ExchangeRate {
            base: base.into(),
            quote: quote.into(),
            rate_date: date.into(),
            rate: "4".into(),
            source: "test".into(),
        })
    }
}
fn setup() -> (tempfile::TempDir, AppService) {
    let dir = tempfile::tempdir().unwrap();
    let s = AppService::open(dir.path().join("app"), Arc::new(NoSecrets))
        .unwrap()
        .with_rate_source(Arc::new(FixedRate));
    (dir, s)
}
fn manual(s: &AppService, currency: &str, total: i64) -> String {
    let e = s.create_expense().unwrap();
    s.edit_expense(ExpenseEdit {
        id: e.id.clone(),
        version: e.version,
        occurred_at: Some("2026-11-01".into()),
        merchant_name: Some("Cafe".into()),
        premises: None,
        total_amount_minor: Some(total),
        tax_amount_minor: None,
        currency: Some(currency.into()),
        category: "Meals".into(),
        description: "".into(),
        mark_ready: true,
    })
    .unwrap();
    e.id
}
fn one_line(amount: i64) -> Vec<ReceiptItem> {
    vec![ReceiptItem {
        description: "Lunch".into(),
        quantity_milli: Some(1000),
        unit_price_minor: Some(amount),
        amount_minor: amount,
        confidence: 0.65,
    }]
}
fn input(r: ReceiptItemsReview) -> ReceiptItemsInput {
    ReceiptItemsInput {
        expense_id: r.expense_id,
        expense_version: r.expense_version,
        version: r.version,
        currency: r.currency,
        items: r.items,
        adjustments: r.adjustments,
        reviewed: false,
    }
}
#[test]
fn item_review_reconciles_integer_adjustments_and_refuses_uncertain_commit() {
    let (_dir, s) = setup();
    let id = manual(&s, "MYR", 1009);
    let mut i = input(s.get_receipt_items(&id).unwrap());
    i.items = one_line(1000);
    i.adjustments = ReceiptItemAdjustments {
        tax_minor: 60,
        discount_minor: 100,
        service_charge_minor: 50,
        rounding_minor: -1,
        tax_included: false,
    };
    i.reviewed = true;
    let review = s.save_receipt_items(i.clone()).unwrap();
    assert_eq!(review.reconciliation.calculated_total_minor, 1009);
    assert!(review.reviewed);
    assert!(review.reconciliation.balanced);
    i.version = review.version;
    i.items[0].amount_minor = 999;
    assert!(s.save_receipt_items(i.clone()).is_err());
    i.reviewed = false;
    let draft = s.save_receipt_items(i).unwrap();
    assert_eq!(draft.reconciliation.difference_minor, Some(-1));
    assert!(!draft.reviewed);
}
#[test]
fn original_currency_stays_authoritative_after_claim_conversion() {
    let (_dir, s) = setup();
    let id = manual(&s, "USD", 1000);
    let e = s.expense(&id).unwrap();
    assert_eq!(e.currency.as_deref(), Some("MYR"));
    assert_eq!(e.total_amount_minor, Some(4000));
    let mut i = input(s.get_receipt_items(&id).unwrap());
    assert_eq!(i.currency, "USD");
    i.items = one_line(1000);
    i.reviewed = true;
    let review = s.save_receipt_items(i).unwrap();
    assert_eq!(review.reconciliation.receipt_total_minor, Some(1000));
    assert!(review.reviewed);
}
#[test]
fn tax_inclusive_receipt_does_not_add_tax_twice_and_stale_edits_are_rejected() {
    let (_dir, s) = setup();
    let id = manual(&s, "MYR", 1060);
    let mut i = input(s.get_receipt_items(&id).unwrap());
    i.items = one_line(1060);
    i.adjustments.tax_minor = 60;
    i.adjustments.tax_included = true;
    i.reviewed = true;
    let review = s.save_receipt_items(i.clone()).unwrap();
    assert_eq!(review.reconciliation.difference_minor, Some(0));
    assert_eq!(s.save_receipt_items(i).unwrap_err().code, "Conflict");
    let e = s.expense(&id).unwrap();
    s.edit_expense(ExpenseEdit {
        id: id.clone(),
        version: e.version,
        occurred_at: e.occurred_at,
        merchant_name: e.merchant_name,
        premises: e.premises,
        total_amount_minor: Some(1100),
        tax_amount_minor: None,
        currency: Some("MYR".into()),
        category: e.category,
        description: e.description,
        mark_ready: true,
    })
    .unwrap();
    let stale = s.get_receipt_items(&id).unwrap();
    assert!(!stale.reviewed);
    assert!(!stale.reconciliation.balanced);
    assert_eq!(stale.reconciliation.difference_minor, Some(-40));
}
#[test]
fn offline_extraction_persists_candidates_and_retries_preserve_manual_review() {
    let (dir, s) = setup();
    let path = dir.path().join("receipt.png");
    image::RgbImage::from_pixel(40, 70, image::Rgb([200, 200, 200]))
        .save(&path)
        .unwrap();
    let id = s.import_receipt(&path).unwrap();
    let job = s.take_job().unwrap().unwrap();
    assert_eq!(job.job.job_type, "extract_receipt");
    s.complete_ocr(&job.job.id,&job.token,"CAFE SDN BHD\n2026-11-01\n1 x Kopi RM4.50\n1 x Nasi RM8.00\nSUBTOTAL RM12.50\nSST 6% RM0.75\nTOTAL RM13.25".into(),Some(95.)).unwrap();
    let draft = s.get_receipt_items(&id).unwrap();
    assert_eq!(draft.source, FieldSource::LocalOcr);
    assert_eq!(draft.items.len(), 2);
    assert!(!draft.reviewed);
    assert!(draft.reconciliation.balanced);
    let mut edited = input(draft);
    edited.items[0].description = "Reviewed coffee".into();
    edited.reviewed = true;
    s.save_receipt_items(edited).unwrap();
    s.queue_extraction(&id).unwrap();
    loop {
        let job = s.take_job().unwrap().unwrap();
        if job.job.job_type == "extract_receipt" {
            s.complete_ocr(&job.job.id,&job.token,"CAFE SDN BHD\n2026-11-01\nCoffee RM4.50\nRice RM8.00\nSUBTOTAL RM12.50\nSST 6% RM0.75\nTOTAL RM13.25".into(),Some(95.)).unwrap();
            break;
        }
        s.fail_job(&job.job.id, &job.token, "Not needed for this test")
            .unwrap();
    }
    let kept = s.get_receipt_items(&id).unwrap();
    assert_eq!(kept.items[0].description, "Reviewed coffee");
    assert_eq!(kept.source, FieldSource::Manual);
    assert!(kept.reviewed);
    s.delete_expenses(vec![id.clone()]).unwrap();
    assert!(s.get_receipt_items(&id).is_err());
}
#[test]
fn unsupported_or_overflowing_values_cannot_be_confirmed() {
    let (_dir, s) = setup();
    let id = manual(&s, "MYR", 1000);
    let mut i = input(s.get_receipt_items(&id).unwrap());
    i.items = one_line(1000);
    i.currency = "USD".into();
    i.reviewed = true;
    assert!(s.save_receipt_items(i.clone()).is_err());
    i.currency = "MYR".into();
    i.items[0].amount_minor = 999_999_999_999;
    i.adjustments.service_charge_minor = 999_999_999_999;
    assert!(s.save_receipt_items(i).is_err());
}
#[test]
fn old_extraction_json_and_existing_v6_database_upgrade_without_data_loss() {
    let legacy = serde_json::json!({"merchantName":"Cafe","date":"2026-11-01","totalAmountMinor":1000,"taxAmountMinor":null,"currency":"MYR","suggestedCategory":"Meals","confidence":{}});
    let extraction: folio::domain::extraction::Extraction = serde_json::from_value(legacy).unwrap();
    assert!(extraction.line_items.is_empty());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_currency_conversion.sql"),
        include_str!("../migrations/003_premises.sql"),
        include_str!("../migrations/004_finance_ledger.sql"),
        include_str!("../migrations/005_business.sql"),
        include_str!("../migrations/006_sync.sql"),
    ] {
        db.execute_batch(sql).unwrap();
    }
    db.execute("INSERT INTO expenses(id,status,total_amount_minor,currency,created_at,updated_at) VALUES('keep','draft',1000,'MYR','now','now')",[]).unwrap();
    drop(db);
    let db = folio::db::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT total_amount_minor FROM expenses WHERE id='keep'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1000
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM receipt_item_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        folio::db::SCHEMA_VERSION
    );
}
