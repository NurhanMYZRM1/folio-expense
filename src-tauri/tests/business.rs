use folio::{
    domain::{business::*, finance::*},
    error::Result,
    security::secrets::SecretStore,
    services::AppService,
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
fn setup() -> (tempfile::TempDir, AppService, String) {
    let dir = tempfile::tempdir().unwrap();
    let service = AppService::open(dir.path().join("app"), Arc::new(NoSecrets)).unwrap();
    let w = service
        .save_workspace(WorkspaceInput {
            id: None,
            version: None,
            name: "Shop".into(),
            base_currency: "MYR".into(),
        })
        .unwrap();
    service
        .save_business_profile(BusinessProfile {
            workspace_id: w.id.clone(),
            legal_name: "Shop Sdn Bhd".into(),
            registration_number: "123".into(),
            tin: "C123".into(),
            sst_registration_number: "".into(),
            address: "Kuala Lumpur".into(),
            email: "shop@example.com".into(),
            phone: "123".into(),
            invoice_prefix: "INV".into(),
            version: 0,
        })
        .unwrap();
    (dir, service, w.id)
}
fn customer(s: &AppService, w: &str) -> Customer {
    s.save_customer(CustomerInput {
        id: None,
        version: None,
        workspace_id: w.into(),
        name: "Customer".into(),
        registration_number: "456".into(),
        tin: "C456".into(),
        address: "Penang".into(),
        email: "customer@example.com".into(),
        phone: "456".into(),
        archived: false,
    })
    .unwrap()
}
fn item(s: &AppService, w: &str) -> InventoryItem {
    s.save_inventory_item(InventoryItemInput {
        id: None,
        version: None,
        workspace_id: w.into(),
        sku: "SKU".into(),
        name: "Widget".into(),
        unit: "unit".into(),
        currency: "MYR".into(),
        unit_price_minor: 1001,
        archived: false,
    })
    .unwrap()
}
fn invoice(s: &AppService, w: &str, c: &str, item: Option<String>) -> Invoice {
    s.save_invoice(InvoiceInput {
        id: None,
        version: None,
        workspace_id: w.into(),
        customer_id: c.into(),
        currency: "MYR".into(),
        issued_on: "2026-11-01".into(),
        due_on: "2026-11-10".into(),
        note: "".into(),
        lines: vec![InvoiceLineInput {
            inventory_item_id: item,
            description: "Widget".into(),
            quantity_milli: 1500,
            unit_price_minor: 1001,
            discount_minor: 1,
            tax_basis_points: 600,
            classification_code: "004".into(),
        }],
    })
    .unwrap()
}
fn account(s: &AppService, w: &str) -> Account {
    s.save_account(AccountInput {
        id: None,
        version: None,
        workspace_id: w.into(),
        name: "Business bank".into(),
        kind: AccountKind::Bank,
        currency: "MYR".into(),
        opening_balance_minor: 0,
        goal_target_minor: None,
        goal_target_date: None,
        loan_direction: None,
        loan_principal_minor: None,
        loan_annual_rate: None,
        counterparty: None,
        note: "".into(),
        archived: false,
    })
    .unwrap()
}
#[test]
fn issue_payment_is_atomic_idempotent_and_does_not_double_count_income() {
    let (_dir, s, w) = setup();
    let c = customer(&s, &w);
    let item = item(&s, &w);
    let adjustment = StockAdjustmentInput {
        request_id: uuid::Uuid::new_v4().to_string(),
        workspace_id: w.clone(),
        item_id: item.id.clone(),
        quantity_milli: 3000,
        occurred_on: "2026-11-01".into(),
        reason: "Opening stock".into(),
    };
    s.adjust_stock(adjustment.clone()).unwrap();
    s.adjust_stock(adjustment).unwrap();
    let draft = invoice(&s, &w, &c.id, Some(item.id.clone()));
    assert_eq!(draft.subtotal_minor, 1501);
    assert_eq!(draft.tax_minor, 90);
    assert_eq!(draft.total_minor, 1591);
    let issued = s.issue_invoice(&w, &draft.id, draft.version).unwrap();
    assert_eq!(issued.number.as_deref(), Some("INV-000001"));
    assert_eq!(
        s.list_inventory_items(&w, false).unwrap()[0].stock_milli,
        1500
    );
    s.issue_invoice(&w, &draft.id, draft.version).unwrap();
    assert_eq!(s.list_stock_movements(&w, &item.id).unwrap().len(), 2);
    let a = account(&s, &w);
    assert_eq!(s.accounts(&w).unwrap()[0].balance_minor, 0);
    let payment = InvoicePaymentInput {
        request_id: uuid::Uuid::new_v4().to_string(),
        workspace_id: w.clone(),
        invoice_id: issued.id.clone(),
        account_id: a.id.clone(),
        amount_minor: 1591,
        paid_on: "2026-11-03".into(),
        note: "Transfer".into(),
    };
    let paid = s.record_invoice_payment(payment.clone()).unwrap();
    s.record_invoice_payment(payment.clone()).unwrap();
    assert_eq!(paid.status, InvoiceStatus::Paid);
    assert_eq!(paid.payments.len(), 1);
    assert_eq!(s.accounts(&w).unwrap()[0].balance_minor, 1591);
    let mut changed = payment;
    changed.amount_minor = 10;
    assert_eq!(
        s.record_invoice_payment(changed).unwrap_err().code,
        "Conflict"
    );
    assert!(s.void_invoice(&w, &paid.id, paid.version).is_err());
    assert!(s
        .delete_transactions(vec![paid.payments[0].transaction_id.clone()])
        .is_err());
    let report = s.report_business(&w, "2026-11-20").unwrap();
    assert_eq!(report.by_currency[0].invoiced_minor, 1591);
    assert_eq!(report.by_currency[0].paid_minor, 1591);
    assert_eq!(report.by_currency[0].outstanding_minor, 0);
}
#[test]
fn insufficient_stock_rolls_back_number_and_void_returns_stock_once() {
    let (_dir, s, w) = setup();
    let c = customer(&s, &w);
    let item = item(&s, &w);
    let draft = invoice(&s, &w, &c.id, Some(item.id.clone()));
    assert!(s.issue_invoice(&w, &draft.id, draft.version).is_err());
    assert_eq!(
        s.get_invoice(&w, &draft.id).unwrap().status,
        InvoiceStatus::Draft
    );
    s.adjust_stock(StockAdjustmentInput {
        request_id: uuid::Uuid::new_v4().to_string(),
        workspace_id: w.clone(),
        item_id: item.id.clone(),
        quantity_milli: 2000,
        occurred_on: "2026-11-01".into(),
        reason: "Opening".into(),
    })
    .unwrap();
    let issued = s.issue_invoice(&w, &draft.id, draft.version).unwrap();
    assert_eq!(issued.number.as_deref(), Some("INV-000001"));
    let void = s.void_invoice(&w, &issued.id, issued.version).unwrap();
    s.void_invoice(&w, &void.id, void.version).unwrap();
    assert_eq!(
        s.list_inventory_items(&w, false).unwrap()[0].stock_milli,
        2000
    );
    assert_eq!(s.list_stock_movements(&w, &item.id).unwrap().len(), 3);
    assert!(s.issue_invoice(&w, &void.id, void.version).is_err());
}
#[test]
fn business_workspace_and_cross_workspace_references_are_enforced() {
    let (_dir, s, w) = setup();
    let c = customer(&s, &w);
    let item = item(&s, &w);
    let other = s
        .save_workspace(WorkspaceInput {
            id: None,
            version: None,
            name: "Other".into(),
            base_currency: "MYR".into(),
        })
        .unwrap();
    assert!(s.get_invoice(&other.id, "missing").is_err());
    let personal = s
        .workspaces()
        .unwrap()
        .into_iter()
        .find(|w| w.kind == WorkspaceKind::Personal)
        .unwrap();
    assert!(s.list_customers(&personal.id, false).is_err());
    assert!(s
        .save_customer(CustomerInput {
            id: Some(c.id),
            version: Some(c.version),
            workspace_id: other.id.clone(),
            name: "Hacked".into(),
            registration_number: "".into(),
            tin: "".into(),
            address: "".into(),
            email: "".into(),
            phone: "".into(),
            archived: false
        })
        .is_err());
    let c2 = customer(&s, &other.id);
    assert!(s
        .save_invoice(InvoiceInput {
            id: None,
            version: None,
            workspace_id: other.id,
            customer_id: c2.id,
            currency: "MYR".into(),
            issued_on: "2026-11-01".into(),
            due_on: "2026-11-10".into(),
            note: "".into(),
            lines: vec![InvoiceLineInput {
                inventory_item_id: Some(item.id),
                description: "Bad".into(),
                quantity_milli: 1000,
                unit_price_minor: 1,
                discount_minor: 0,
                tax_basis_points: 0,
                classification_code: "".into()
            }]
        })
        .is_err());
}
#[test]
fn customer_versions_and_snapshot_immutability() {
    let (_dir, s, w) = setup();
    let c = customer(&s, &w);
    let draft = invoice(&s, &w, &c.id, None);
    let issued = s.issue_invoice(&w, &draft.id, draft.version).unwrap();
    let changed = s
        .save_customer(CustomerInput {
            id: Some(c.id.clone()),
            version: Some(c.version),
            workspace_id: w.clone(),
            name: "New name".into(),
            registration_number: c.registration_number.clone(),
            tin: c.tin.clone(),
            address: c.address.clone(),
            email: c.email.clone(),
            phone: c.phone.clone(),
            archived: false,
        })
        .unwrap();
    assert_eq!(
        s.get_invoice(&w, &issued.id).unwrap().customer.name,
        "Customer"
    );
    assert_eq!(changed.version, c.version + 1);
    assert_eq!(
        s.validate_einvoice(&w, &issued.id)
            .unwrap()
            .preparation_status,
        "local_preflight_only"
    );
}

#[test]
fn paid_bill_history_survives_schedule_reanchoring() {
    let (_dir, s, w) = setup();
    let account = account(&s, &w);
    let input = BillInput {
        id: None,
        version: None,
        workspace_id: w,
        name: "Rent".into(),
        kind: BillKind::Bill,
        amount_minor: 10000,
        account_id: account.id,
        category_id: None,
        payee: None,
        note: String::new(),
        frequency: Frequency::Monthly,
        interval_count: 1,
        anchor_on: "2026-01-31".into(),
        ends_on: None,
        remind_days_before: 3,
        reminders_enabled: true,
        cancelled_on: None,
    };
    let bill = s.save_bill(input.clone()).unwrap();
    let paid = s
        .pay_bill(BillPaymentInput {
            bill_id: bill.id.clone(),
            due_on: "2026-01-31".into(),
            amount_minor: None,
            paid_on: "2026-01-31".into(),
            account_id: None,
            payment_method: None,
            note: String::new(),
        })
        .unwrap();
    s.save_bill(BillInput {
        id: Some(bill.id.clone()),
        version: Some(bill.version),
        anchor_on: "2026-03-15".into(),
        ..input
    })
    .unwrap();
    let detail = s.bill(&bill.id, "2026-03-01").unwrap();
    assert!(
        detail
            .occurrences
            .iter()
            .any(|o| o.due_on == "2026-01-31"
                && o.transaction_id.as_deref() == Some(paid.id.as_str())),
        "Paid history must remain visible after schedule changes."
    );
    assert!(
        !detail.occurrences.iter().any(|o| o.due_on == "2026-02-28"),
        "Earlier unpaid occurrences of the old schedule must be removed."
    );
}
#[test]
fn only_unused_customers_and_items_can_be_deleted() {
    let (_dir, s, w) = setup();
    let new_item = |sku: &str| {
        s.save_inventory_item(InventoryItemInput {
            id: None,
            version: None,
            workspace_id: w.clone(),
            sku: sku.into(),
            name: sku.into(),
            unit: "unit".into(),
            currency: "MYR".into(),
            unit_price_minor: 500,
            archived: false,
        })
        .unwrap()
    };
    let (invoiced, spare) = (customer(&s, &w), customer(&s, &w));
    let (on_invoice, stocked, unused) = (new_item("A"), new_item("B"), new_item("C"));
    invoice(&s, &w, &invoiced.id, Some(on_invoice.id.clone()));
    s.adjust_stock(StockAdjustmentInput {
        request_id: uuid::Uuid::new_v4().to_string(),
        workspace_id: w.clone(),
        item_id: stocked.id.clone(),
        quantity_milli: 2000,
        occurred_on: "2026-11-05".into(),
        reason: "Opening stock".into(),
    })
    .unwrap();

    let refused = s.delete_customer(&invoiced.id).unwrap_err();
    assert!(refused.message.contains("Archive"), "{}", refused.message);
    s.delete_customer(&spare.id).unwrap();
    assert!(
        s.delete_customer(&spare.id).is_err(),
        "deleting twice reports not found"
    );
    for used in [&on_invoice, &stocked] {
        let refused = s.delete_inventory_item(&used.id).unwrap_err();
        assert!(refused.message.contains("Archive"), "{}", refused.message);
    }
    s.delete_inventory_item(&unused.id).unwrap();

    let customers: Vec<_> = s
        .list_customers(&w, true)
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(customers, vec![invoiced.id]);
    let mut items: Vec<_> = s
        .list_inventory_items(&w, true)
        .unwrap()
        .into_iter()
        .map(|i| i.sku)
        .collect();
    items.sort();
    assert_eq!(items, vec!["A", "B"]);
}
