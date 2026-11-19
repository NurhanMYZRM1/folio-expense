use folio::{
    dispatch::dispatch, domain::finance::*, error::Result, security::secrets::SecretStore,
    services::AppService,
};
use serde_json::{json, Value};
use std::sync::Arc;
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
fn service() -> (tempfile::TempDir, AppService) {
    let t = tempfile::tempdir().unwrap();
    let s = AppService::open(t.path().join("app"), Arc::new(Secrets)).unwrap();
    (t, s)
}
fn call(s: &AppService, c: &str, a: Value) -> Value {
    dispatch(s, c, a).unwrap()
}
fn account(s: &AppService, w: &str, name: &str, opening: i64) -> Account {
    s.save_account(serde_json::from_value(json!({"id":null,"version":null,"workspaceId":w,"name":name,"kind":"bank","currency":"MYR","openingBalanceMinor":opening,"goalTargetMinor":null,"goalTargetDate":null,"loanDirection":null,"loanPrincipalMinor":null,"loanAnnualRate":null,"counterparty":null,"note":"","archived":false})).unwrap()).unwrap()
}
fn transaction(w: &str, a: &str, kind: &str, amount: i64) -> TransactionInput {
    serde_json::from_value(json!({"id":null,"version":null,"workspaceId":w,"kind":kind,"accountId":a,"amountMinor":amount,"toAccountId":null,"toAmountMinor":null,"occurredOn":"2026-11-05","categoryId":null,"payee":"Merchant","note":"","paymentMethod":"cash","source":"manual"})).unwrap()
}
#[test]
fn ledger_roundtrip_edits_transfers_reports_and_workspace_scope() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    assert_eq!(w.kind, WorkspaceKind::Personal);
    let a = account(&s, &w.id, "Bank", 10000);
    let b = account(&s, &w.id, "Savings", 0);
    s.save_transaction(transaction(&w.id, &a.id, "income", 5000))
        .unwrap();
    let expense = s
        .save_transaction(transaction(&w.id, &a.id, "expense", 1500))
        .unwrap();
    let mut transfer = transaction(&w.id, &a.id, "transfer", 2000);
    transfer.to_account_id = Some(b.id.clone());
    transfer.to_amount_minor = Some(2000);
    s.save_transaction(transfer).unwrap();
    assert_eq!(
        s.overview(&w.id, "2026-11-05").unwrap().total_balance_minor,
        13500
    );
    let report = s.monthly_report(&w.id, "2026-11").unwrap();
    assert_eq!(
        (report.income_minor, report.expense_minor, report.net_minor),
        (5000, 1500, 3500)
    );
    let mut edit = transaction(&w.id, &a.id, "expense", 1000);
    edit.id = Some(expense.id.clone());
    edit.version = Some(expense.version);
    s.save_transaction(edit.clone()).unwrap();
    assert_eq!(s.save_transaction(edit).unwrap_err().code, "Conflict");
    assert_eq!(
        s.monthly_report(&w.id, "2026-11").unwrap().expense_minor,
        1000
    );
    s.delete_transactions(vec![expense.id]).unwrap();
    assert_eq!(s.monthly_report(&w.id, "2026-11").unwrap().expense_minor, 0);
    let other = s
        .save_workspace(WorkspaceInput {
            id: None,
            version: None,
            name: "Work".into(),
            base_currency: "MYR".into(),
        })
        .unwrap();
    let invalid = transaction(&other.id, &a.id, "expense", 100);
    assert!(s.save_transaction(invalid).is_err());
    assert!(s.delete_account(&a.id).is_err());
    assert_eq!(
        s.accounts(&w.id)
            .unwrap()
            .iter()
            .find(|v| v.id == b.id)
            .unwrap()
            .balance_minor,
        2000
    );
}
#[test]
fn budget_recalculates_and_alert_is_acknowledged_once() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let a = account(&s, &w.id, "Cash", 0);
    let mut settings = s.settings().unwrap();
    settings.notify_budgets = true;
    s.save_settings(settings).unwrap();
    let budget = s
        .save_budget(BudgetInput {
            id: None,
            version: None,
            workspace_id: w.id.clone(),
            category_id: None,
            amount_minor: 1000,
            alert_percent: 80,
            archived: false,
        })
        .unwrap();
    let t = s
        .save_transaction(transaction(&w.id, &a.id, "expense", 900))
        .unwrap();
    let p = s.budgets(&w.id, "2026-11").unwrap().remove(0);
    assert_eq!(
        (p.spent_minor, p.remaining_minor, p.percent),
        (900, 100, 90)
    );
    let reminders = s.reminders("2026-11-05", 14).unwrap();
    assert_eq!(reminders.len(), 1);
    assert!(reminders[0].id.contains(&budget.id));
    s.ack_reminder(&reminders[0].id).unwrap();
    assert!(s.reminders("2026-11-05", 14).unwrap().is_empty());
    s.delete_transactions(vec![t.id]).unwrap();
    assert_eq!(s.budgets(&w.id, "2026-11").unwrap()[0].spent_minor, 0);
    s.save_transaction(transaction(&w.id, &a.id, "expense", 900))
        .unwrap();
    assert!(s.reminders("2026-11-05", 14).unwrap().is_empty());
}
fn bill(w: &str, a: &str, anchor: &str, frequency: &str) -> BillInput {
    serde_json::from_value(json!({"id":null,"version":null,"workspaceId":w,"name":"Phone","kind":"subscription","amountMinor":1000,"accountId":a,"categoryId":null,"payee":null,"note":"","frequency":frequency,"intervalCount":1,"anchorOn":anchor,"endsOn":null,"remindDaysBefore":3,"remindersEnabled":true,"cancelledOn":null})).unwrap()
}
#[test]
fn month_end_bill_payment_is_idempotent_and_deletion_reopens_occurrence() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let a = account(&s, &w.id, "Bank", 0);
    let b = s
        .save_bill(bill(&w.id, &a.id, "2028-01-31", "monthly"))
        .unwrap();
    let detail = s.bill(&b.id, "2028-02-01").unwrap();
    let dates: Vec<_> = detail
        .occurrences
        .iter()
        .map(|o| o.due_on.as_str())
        .collect();
    assert_eq!(
        dates,
        ["2028-01-31", "2028-02-29", "2028-03-31", "2028-04-30"]
    );
    let input = BillPaymentInput {
        bill_id: b.id.clone(),
        due_on: "2028-02-29".into(),
        amount_minor: None,
        paid_on: "2028-02-29".into(),
        account_id: None,
        payment_method: None,
        note: "".into(),
    };
    let first = s.pay_bill(input.clone()).unwrap();
    let retry = s.pay_bill(input).unwrap();
    assert_eq!(first.id, retry.id);
    assert_eq!(
        s.monthly_report(&w.id, "2028-02").unwrap().expense_minor,
        1000
    );
    s.delete_transactions(vec![first.id]).unwrap();
    let detail = s.bill(&b.id, "2028-03-01").unwrap();
    assert_eq!(
        detail
            .occurrences
            .iter()
            .find(|o| o.due_on == "2028-02-29")
            .unwrap()
            .status,
        OccurrenceStatus::Overdue
    );
}
#[test]
fn linked_receipt_owns_values_and_delete_cascades_only_ledger() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let a = account(&s, &w.id, "Bank", 0);
    let e = call(&s, "create_expense", json!({}));
    let eid = e["id"].as_str().unwrap();
    let edit = json!({"id":eid,"version":e["version"],"merchantName":"Lunch","premises":null,"occurredAt":"2026-11-05","currency":"MYR","totalAmountMinor":2500,"taxAmountMinor":0,"category":"Meals","description":"","markReady":true});
    call(&s, "edit_expense", json!({"edit":edit}));
    let input = RecordExpenseInput {
        expense_id: eid.into(),
        account_id: a.id.clone(),
        category_id: None,
        payment_method: None,
        note: "".into(),
    };
    let t = s.record_expense(input.clone()).unwrap();
    assert_eq!(s.record_expense(input).unwrap().id, t.id);
    let mut bad = transaction(&w.id, &a.id, "expense", 100);
    bad.id = Some(t.id.clone());
    bad.version = Some(t.version);
    assert!(s.save_transaction(bad).is_err());
    let mut edited = edit.clone();
    edited["totalAmountMinor"] = json!(3000);
    edited["version"] = call(&s, "get_expense", json!({"id":eid}))["version"].clone();
    call(&s, "edit_expense", json!({"edit":edited}));
    assert_eq!(s.transaction(&t.id).unwrap().amount_minor, 3000);
    s.delete_expenses(vec![eid.into()]).unwrap();
    assert!(s.transaction(&t.id).is_err());
}
#[test]
fn quick_entry_is_review_only_and_currency_precision_is_respected() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let d = s
        .parse_quick_add(&w.id, "paid RM12.35 lunch yesterday", "2026-11-05")
        .unwrap();
    assert_eq!(d.amount_minor, Some(1235));
    assert_eq!(d.occurred_on, "2026-11-04");
    assert!(d.uncertain.contains(&"account".into()));
    assert_eq!(
        s.transactions(TransactionFilter {
            workspace_id: w.id.clone(),
            limit: 10,
            ..Default::default()
        })
        .unwrap()
        .total,
        0
    );
    let d = s
        .parse_quick_add(&w.id, "paid JPY 1500 yesterday", "2026-11-05")
        .unwrap();
    assert_eq!(d.amount_minor, Some(1500));
    let d = s
        .parse_quick_add(&w.id, "paid BHD 1.234 yesterday", "2026-11-05")
        .unwrap();
    assert_eq!(d.amount_minor, Some(1234));
}

#[test]
fn archived_accounts_cannot_gain_hidden_money_after_a_history_change() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let a = account(&s, &w.id, "Closing", 1000);
    let b = account(&s, &w.id, "Active", 0);
    let mut input = transaction(&w.id, &a.id, "transfer", 1000);
    input.to_account_id = Some(b.id.clone());
    input.to_amount_minor = Some(1000);
    let t = s.save_transaction(input).unwrap();
    let archived:AccountInput=serde_json::from_value(json!({"id":a.id,"version":a.version,"workspaceId":w.id,"name":a.name,"kind":a.kind,"currency":a.currency,"openingBalanceMinor":a.opening_balance_minor,"goalTargetMinor":null,"goalTargetDate":null,"loanDirection":null,"loanPrincipalMinor":null,"loanAnnualRate":null,"counterparty":null,"note":"","archived":true})).unwrap();
    s.save_account(archived).unwrap();
    assert!(s.delete_transactions(vec![t.id.clone()]).is_err());
    assert!(s.transaction(&t.id).is_ok());
    assert_eq!(
        s.accounts(&w.id)
            .unwrap()
            .iter()
            .find(|v| v.id == a.id)
            .unwrap()
            .balance_minor,
        0
    );
}

#[test]
fn loans_and_savings_use_balanced_transfers_and_reject_overpayment() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let bank = account(&s, &w.id, "Bank", 10000);
    let loan:AccountInput=serde_json::from_value(json!({"id":null,"version":null,"workspaceId":w.id,"name":"Loan","kind":"loan","currency":"MYR","openingBalanceMinor":-5000,"goalTargetMinor":null,"goalTargetDate":null,"loanDirection":"borrowed","loanPrincipalMinor":5000,"loanAnnualRate":"4.50","counterparty":"Bank","note":"","archived":false})).unwrap();
    let loan = s.save_account(loan).unwrap();
    let goal:AccountInput=serde_json::from_value(json!({"id":null,"version":null,"workspaceId":w.id,"name":"Holiday","kind":"savings","currency":"MYR","openingBalanceMinor":0,"goalTargetMinor":20000,"goalTargetDate":"2027-05-01","loanDirection":null,"loanPrincipalMinor":null,"loanAnnualRate":null,"counterparty":null,"note":"","archived":false})).unwrap();
    let goal = s.save_account(goal).unwrap();
    let mut pay = transaction(&w.id, &bank.id, "transfer", 1000);
    pay.to_account_id = Some(loan.id.clone());
    pay.to_amount_minor = Some(1000);
    s.save_transaction(pay).unwrap();
    let mut save = transaction(&w.id, &bank.id, "transfer", 2000);
    save.to_account_id = Some(goal.id.clone());
    save.to_amount_minor = Some(2000);
    s.save_transaction(save).unwrap();
    let o = s.overview(&w.id, "2026-11-05").unwrap();
    assert_eq!(o.total_balance_minor, 9000);
    assert_eq!(o.month_income_minor, 0);
    assert_eq!(o.month_expense_minor, 0);
    assert_eq!(
        o.accounts
            .iter()
            .find(|a| a.id == loan.id)
            .unwrap()
            .balance_minor,
        -4000
    );
    assert_eq!(
        o.accounts
            .iter()
            .find(|a| a.id == goal.id)
            .unwrap()
            .balance_minor,
        2000
    );
    let mut excess = transaction(&w.id, &bank.id, "transfer", 5000);
    excess.to_account_id = Some(loan.id.clone());
    excess.to_amount_minor = Some(5000);
    assert!(s.save_transaction(excess).is_err());
}

struct OfflineRates;
impl folio::services::exchange_rates::RateSource for OfflineRates {
    fn fetch(&self, _: &str, _: &str, _: &str) -> Result<folio::domain::exchange::ExchangeRate> {
        Err(folio::error::AppError::new("RateOffline", "offline"))
    }
}
struct FixedRates;
impl folio::services::exchange_rates::RateSource for FixedRates {
    fn fetch(
        &self,
        base: &str,
        quote: &str,
        date: &str,
    ) -> Result<folio::domain::exchange::ExchangeRate> {
        Ok(folio::domain::exchange::ExchangeRate {
            base: base.into(),
            quote: quote.into(),
            rate_date: date.into(),
            rate: "0.029".into(),
            source: "test".into(),
        })
    }
}
#[test]
fn offline_foreign_amounts_remain_explicit_then_convert_with_integer_rounding() {
    let (t, s) = service();
    let s = s.with_rate_source(Arc::new(OfflineRates));
    let w = s.workspaces().unwrap().remove(0);
    let a = account(&s, &w.id, "Foreign", 0);
    let mut foreign:AccountInput=serde_json::from_value(json!({"id":a.id,"version":a.version,"workspaceId":w.id,"name":a.name,"kind":"bank","currency":"JPY","openingBalanceMinor":0,"goalTargetMinor":null,"goalTargetDate":null,"loanDirection":null,"loanPrincipalMinor":null,"loanAnnualRate":null,"counterparty":null,"note":"","archived":false})).unwrap();
    foreign.currency = "JPY".into();
    let a = s.save_account(foreign).unwrap();
    let record = s
        .save_transaction(transaction(&w.id, &a.id, "expense", 1500))
        .unwrap();
    assert_eq!(record.base_amount_minor, None);
    let r = s.monthly_report(&w.id, "2026-11").unwrap();
    assert_eq!((r.expense_minor, r.unconverted_count), (0, 1));
    drop(s);
    let s = AppService::open(t.path().join("app"), Arc::new(Secrets))
        .unwrap()
        .with_rate_source(Arc::new(FixedRates));
    assert_eq!(s.workspaces().unwrap().len(), 1);
    assert_eq!(s.convert_pending().unwrap(), 1);
    let r = s.monthly_report(&w.id, "2026-11").unwrap();
    assert_eq!((r.expense_minor, r.unconverted_count), (4350, 0));
    assert_eq!(
        s.transaction(&record.id).unwrap().exchange_rate.as_deref(),
        Some("0.029")
    );
}

#[test]
fn csv_export_can_be_opened_and_guards_spreadsheet_formulas() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0);
    let a = account(&s, &w.id, "Bank", 0);
    let mut input = transaction(&w.id, &a.id, "income", 1234);
    input.payee = Some("=HYPERLINK(\"https://example.com\")".into());
    input.note = "line1\nline2, quoted".into();
    s.save_transaction(input).unwrap();
    let e = s
        .export_transactions_csv(TransactionFilter {
            workspace_id: w.id,
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(e.rows, 1);
    assert_eq!(s.csv_export_path(&e.id).unwrap(), e.path);
    let csv = std::fs::read_to_string(e.path).unwrap();
    assert!(csv.contains("12.34"));
    assert!(csv.contains("'=HYPERLINK"));
    assert!(csv.contains("\"line1\nline2, quoted\""));
}
#[test]
fn category_names_are_unique_per_kind_and_used_categories_are_archived_not_deleted() {
    let (_t, s) = service();
    let w = s.workspaces().unwrap().remove(0).id;
    let save = |name: &str, kind: &str, existing: Option<&Category>, archived: bool| {
        s.save_category(serde_json::from_value(json!({"id":existing.map(|c| &c.id),"version":existing.map(|c| c.version),"workspaceId":w,"kind":kind,"name":name,"archived":archived})).unwrap())
    };
    let coffee = save("Coffee", "expense", None, false).unwrap();
    // Differing only in case and spacing is still the same name for the same kind.
    let duplicate = save("  coffee ", "expense", None, false).unwrap_err();
    assert!(
        duplicate.message.contains("already exists"),
        "{}",
        duplicate.message
    );
    save("Coffee", "income", None, false).unwrap();
    let tea = save("Tea", "expense", None, false).unwrap();
    assert!(save("COFFEE", "expense", Some(&tea), false).is_err());
    let coffee = save("Coffee", "expense", Some(&coffee), false).unwrap();
    // A category in use cannot be deleted; archiving keeps its history.
    let bank = account(&s, &w, "Bank", 0);
    let mut spend = transaction(&w, &bank.id, "expense", 500);
    spend.category_id = Some(coffee.id.clone());
    s.save_transaction(spend).unwrap();
    let in_use = s.delete_category(&coffee.id).unwrap_err();
    assert!(in_use.message.contains("Archive"), "{}", in_use.message);
    assert!(save("Coffee", "expense", Some(&coffee), true)
        .unwrap()
        .archived_at
        .is_some());
    s.delete_category(&tea.id).unwrap();
    let names: Vec<_> = s
        .categories(&w)
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&"Coffee".to_string()) && !names.contains(&"Tea".to_string()));
}
