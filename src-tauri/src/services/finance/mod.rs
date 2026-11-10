//! Workspace-scoped integer ledger. All multi-record changes commit atomically.
use super::AppService;
use crate::{
    domain::{
        exchange::convert_minor,
        expense::{currency_exponent, Expense, ExpenseStatus, MAX_MONEY},
        finance::*,
        id, now,
    },
    error::{AppError, Result},
    repository::expense_repository,
};
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
mod entry;
mod schedules;
use schedules::*;

pub(crate) fn key<T: Serialize>(v: T) -> String {
    serde_json::to_value(v).unwrap().as_str().unwrap().into()
}
pub(crate) fn query<T: DeserializeOwned>(
    db: &Connection,
    sql: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<T>> {
    let mut stmt = db.prepare(sql)?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|n| {
            let mut out = String::new();
            let mut upper = false;
            for c in n.chars() {
                if c == '_' {
                    upper = true;
                } else if upper {
                    out.extend(c.to_uppercase());
                    upper = false;
                } else {
                    out.push(c);
                }
            }
            out
        })
        .collect();
    let mut rows = stmt.query(p)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut m = Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(i) if n == "remindersEnabled" => Value::Bool(i != 0),
                ValueRef::Integer(i) => Value::from(i),
                ValueRef::Real(_) => {
                    return Err(AppError::invalid("A ledger amount was not an integer."))
                }
                ValueRef::Text(s) => Value::from(String::from_utf8_lossy(s).into_owned()),
                ValueRef::Blob(_) => return Err(AppError::invalid("Invalid ledger value.")),
            };
            m.insert(n.clone(), v);
        }
        out.push(serde_json::from_value(Value::Object(m))?);
    }
    Ok(out)
}
fn one<T: DeserializeOwned>(db: &Connection, sql: &str, p: impl rusqlite::Params) -> Result<T> {
    query(db, sql, p)?
        .into_iter()
        .next()
        .ok_or_else(AppError::not_found)
}
pub(crate) fn safe(v: i64) -> Result<i64> {
    if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&v) {
        Err(AppError::invalid(
            "The total is too large to display exactly.",
        ))
    } else {
        Ok(v)
    }
}
fn money(v: i64) -> Result<()> {
    if !(1..=MAX_MONEY).contains(&v) {
        Err(AppError::invalid(
            "Enter a positive amount within the supported limit.",
        ))
    } else {
        Ok(())
    }
}
fn text(v: &str, max: usize, label: &str) -> Result<()> {
    if v.trim().is_empty() || v.chars().count() > max {
        Err(AppError::invalid(format!(
            "Enter {label} (up to {max} characters)."
        )))
    } else {
        Ok(())
    }
}
fn note(v: &str, max: usize) -> Result<()> {
    if v.chars().count() > max {
        Err(AppError::invalid("The note is too long."))
    } else {
        Ok(())
    }
}
pub(crate) fn date(v: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(v, "%Y-%m-%d")
        .ok()
        .filter(|d| d.format("%Y-%m-%d").to_string() == v)
        .ok_or_else(|| AppError::invalid("Enter a valid date as YYYY-MM-DD."))
}
fn month(v: &str) -> Result<NaiveDate> {
    date(&format!("{v}-01"))
}
fn conflict(version: Option<i32>, actual: i32) -> Result<()> {
    if version != Some(actual) {
        Err(AppError::new(
            "Conflict",
            "This record changed. Reload before saving.",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn workspace(db: &Connection, id: &str) -> Result<Workspace> {
    one(db, "SELECT * FROM workspaces WHERE id=?1", [id])
}
fn seed(db: &Connection, w: &str) -> Result<()> {
    for (kind, names) in [
        (
            "income",
            &["Salary", "Business", "Reimbursement", "Other Income"][..],
        ),
        (
            "expense",
            &[
                "Meals",
                "Transport",
                "Accommodation",
                "Fuel",
                "Parking",
                "Office Supplies",
                "Travel",
                "Entertainment",
                "Software",
                "Bills",
                "Other",
            ][..],
        ),
    ] {
        for n in names {
            db.execute("INSERT OR IGNORE INTO categories(id,workspace_id,kind,name,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![id(),w,kind,n,now()])?;
        }
    }
    Ok(())
}
const ACCOUNT_SQL:&str="SELECT a.*,a.opening_balance_minor+COALESCE((SELECT SUM(CASE WHEN t.account_id=a.id THEN CASE WHEN t.kind='income' THEN t.amount_minor ELSE -t.amount_minor END ELSE t.to_amount_minor END) FROM transactions t WHERE t.account_id=a.id OR t.to_account_id=a.id),0) AS balance_minor FROM accounts a";
pub(crate) fn account(db: &Connection, id: &str) -> Result<Account> {
    let a: Account = one(db, &format!("{ACCOUNT_SQL} WHERE a.id=?1"), [id])?;
    safe(a.balance_minor)?;
    Ok(a)
}
fn check_account(db: &Connection, id: &str, w: &str) -> Result<Account> {
    let a = account(db, id)?;
    if a.workspace_id != w || a.archived_at.is_some() {
        return Err(AppError::invalid(
            "Choose an active account in this workspace.",
        ));
    }
    Ok(a)
}
fn category(db: &Connection, id: &str, w: &str, kind: CategoryKind) -> Result<Category> {
    let c:Category=one(db,"SELECT id,workspace_id,kind,name,archived_at,version,sync_state FROM categories WHERE id=?1",[id])?;
    if c.workspace_id != w || c.kind != kind || c.archived_at.is_some() {
        return Err(AppError::invalid(
            "Choose an active category of the matching kind in this workspace.",
        ));
    }
    Ok(c)
}

impl AppService {
    pub(crate) fn initialize_finance(&self) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE kind='personal')",
            [],
            |r| r.get(0),
        )?;
        if !exists {
            let w = id();
            let s = Self::settings_in(&tx)?;
            tx.execute("INSERT INTO workspaces(id,kind,name,base_currency,created_at,updated_at) VALUES(?1,'personal','Personal',?2,?3,?3)",params![w,s.default_currency,now()])?;
            seed(&tx, &w)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        query(
            &*self.conn()?,
            "SELECT * FROM workspaces ORDER BY kind DESC,created_at",
            [],
        )
    }
    pub fn save_workspace(&self, input: WorkspaceInput) -> Result<Workspace> {
        text(&input.name, 120, "a workspace name")?;
        currency_exponent(&input.base_currency)?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let w = if let Some(ref w) = input.id {
            let before = workspace(&tx, w)?;
            conflict(input.version, before.version)?;
            tx.execute("UPDATE workspaces SET name=?1,base_currency=?2,version=version+1,updated_at=?3 WHERE id=?4",params![input.name.trim(),input.base_currency,now(),w])?;
            if before.base_currency != input.base_currency {
                reconvert_all(&tx, w)?;
            }
            w.clone()
        } else {
            let w = id();
            tx.execute("INSERT INTO workspaces(id,kind,name,base_currency,created_at,updated_at) VALUES(?1,'business',?2,?3,?4,?4)",params![w,input.name.trim(),input.base_currency,now()])?;
            seed(&tx, &w)?;
            w
        };
        let out = workspace(&tx, &w)?;
        tx.commit()?;
        Ok(out)
    }
    pub fn categories(&self, w: &str) -> Result<Vec<Category>> {
        let db = self.conn()?;
        workspace(&db, w)?;
        query(&db,"SELECT id,workspace_id,kind,name,archived_at,version,sync_state FROM categories WHERE workspace_id=?1 ORDER BY kind,name",[w])
    }
    pub fn save_category(&self, input: CategoryInput) -> Result<Category> {
        text(&input.name, 60, "a category name")?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        workspace(&tx, &input.workspace_id)?;
        let c = input.id.clone().unwrap_or_else(id);
        // The column is COLLATE NOCASE, so this matches the UNIQUE constraint exactly.
        let taken: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE workspace_id=?1 AND kind=?2 AND name=?3 AND id<>?4)",
            params![input.workspace_id, key(input.kind), input.name.trim(), c],
            |r| r.get(0),
        )?;
        if taken {
            return Err(AppError::invalid(
                "A category with this name already exists. Choose another name.",
            ));
        }
        if input.id.is_some() {
            let before:Category=one(&tx,"SELECT id,workspace_id,kind,name,archived_at,version,sync_state FROM categories WHERE id=?1",[&c])?;
            if before.workspace_id != input.workspace_id || before.kind != input.kind {
                return Err(AppError::invalid(
                    "A category cannot change workspace or kind.",
                ));
            }
            conflict(input.version, before.version)?;
            tx.execute("UPDATE categories SET name=?1,archived_at=?2,version=version+1,updated_at=?3 WHERE id=?4",params![input.name.trim(),input.archived.then(now),now(),c])?;
        } else {
            tx.execute("INSERT INTO categories(id,workspace_id,kind,name,archived_at,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",params![c,input.workspace_id,key(input.kind),input.name.trim(),input.archived.then(now),now()])?;
        }
        let out=one(&tx,"SELECT id,workspace_id,kind,name,archived_at,version,sync_state FROM categories WHERE id=?1",[c])?;
        tx.commit()?;
        Ok(out)
    }
    pub fn delete_category(&self, id: &str) -> Result<()> {
        let db = self.conn()?;
        let used:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE category_id=?1 UNION ALL SELECT 1 FROM bills WHERE category_id=?1 UNION ALL SELECT 1 FROM budgets WHERE category_id=?1)",[id],|r|r.get(0))?;
        if used {
            return Err(AppError::invalid(
                "Archive this category to preserve existing records.",
            ));
        }
        if db.execute("DELETE FROM categories WHERE id=?1", [id])? == 0 {
            return Err(AppError::not_found());
        }
        Ok(())
    }
    pub fn accounts(&self, w: &str) -> Result<Vec<Account>> {
        let db = self.conn()?;
        workspace(&db, w)?;
        let out: Vec<Account> = query(
            &db,
            &format!("{ACCOUNT_SQL} WHERE a.workspace_id=?1 ORDER BY a.created_at,a.name"),
            [w],
        )?;
        for a in &out {
            safe(a.balance_minor)?;
        }
        Ok(out)
    }
    pub fn save_account(&self, input: AccountInput) -> Result<Account> {
        text(&input.name, 120, "an account name")?;
        currency_exponent(&input.currency)?;
        if !(-MAX_MONEY..=MAX_MONEY).contains(&input.opening_balance_minor) {
            return Err(AppError::invalid("The opening balance is too large."));
        }
        note(&input.note, 2000)?;
        if let Some(ref d) = input.goal_target_date {
            date(d)?;
        }
        if let Some(v) = input.goal_target_minor {
            money(v)?;
        }
        if input.kind == AccountKind::Loan {
            let principal = input
                .loan_principal_minor
                .ok_or_else(|| AppError::invalid("Enter the loan principal."))?;
            money(principal)?;
            let d = input
                .loan_direction
                .ok_or_else(|| AppError::invalid("Choose borrowed or lent."))?;
            if (d == LoanDirection::Borrowed && input.opening_balance_minor > 0)
                || (d == LoanDirection::Lent && input.opening_balance_minor < 0)
            {
                return Err(AppError::invalid("Borrowed loans need a negative opening balance; lent loans a positive balance."));
            }
        } else if input.loan_direction.is_some() || input.loan_principal_minor.is_some() {
            return Err(AppError::invalid(
                "Only loan accounts can have loan details.",
            ));
        }
        if input.kind != AccountKind::Savings
            && (input.goal_target_minor.is_some() || input.goal_target_date.is_some())
        {
            return Err(AppError::invalid("Only savings accounts can have a goal."));
        }
        if let Some(ref r) = input.loan_annual_rate {
            if r.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0. && *v <= 1000.)
                .is_none()
            {
                return Err(AppError::invalid(
                    "Enter a valid informational annual rate.",
                ));
            }
        }
        if let Some(ref c) = input.counterparty {
            text(c, 120, "a counterparty")?;
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        workspace(&tx, &input.workspace_id)?;
        let aid = input.id.clone().unwrap_or_else(id);
        if input.id.is_some() {
            let before = account(&tx, &aid)?;
            if before.workspace_id != input.workspace_id {
                return Err(AppError::invalid("An account cannot move workspaces."));
            }
            conflict(input.version, before.version)?;
            let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE account_id=?1 OR to_account_id=?1 UNION ALL SELECT 1 FROM bills WHERE account_id=?1)",[&aid],|r|r.get(0))?;
            if used && (before.currency != input.currency || before.kind != input.kind) {
                return Err(AppError::invalid(
                    "The currency and kind of an account with records cannot change.",
                ));
            }
            let new_balance =
                before.balance_minor - before.opening_balance_minor + input.opening_balance_minor;
            if input.archived&&(new_balance!=0||tx.query_row("SELECT EXISTS(SELECT 1 FROM bills WHERE account_id=?1 AND cancelled_on IS NULL)",[&aid],|r|r.get::<_,bool>(0))?){return Err(AppError::invalid("An account must have zero balance and no active bills before archiving."));}
            tx.execute("UPDATE accounts SET name=?1,kind=?2,currency=?3,opening_balance_minor=?4,goal_target_minor=?5,goal_target_date=?6,loan_direction=?7,loan_principal_minor=?8,loan_annual_rate=?9,counterparty=?10,note=?11,archived_at=?12,version=version+1,updated_at=?13 WHERE id=?14",params![input.name.trim(),key(input.kind),input.currency,input.opening_balance_minor,input.goal_target_minor,input.goal_target_date,input.loan_direction.map(key),input.loan_principal_minor,input.loan_annual_rate,input.counterparty,input.note,input.archived.then(now),now(),aid])?;
        } else {
            if input.archived && input.opening_balance_minor != 0 {
                return Err(AppError::invalid(
                    "Only a zero-balance account can be archived.",
                ));
            }
            tx.execute("INSERT INTO accounts(id,workspace_id,name,kind,currency,opening_balance_minor,goal_target_minor,goal_target_date,loan_direction,loan_principal_minor,loan_annual_rate,counterparty,note,archived_at,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15)",params![aid,input.workspace_id,input.name.trim(),key(input.kind),input.currency,input.opening_balance_minor,input.goal_target_minor,input.goal_target_date,input.loan_direction.map(key),input.loan_principal_minor,input.loan_annual_rate,input.counterparty,input.note,input.archived.then(now),now()])?;
        }
        let out = account(&tx, &aid)?;
        tx.commit()?;
        Ok(out)
    }
    pub fn delete_account(&self, id: &str) -> Result<()> {
        let db = self.conn()?;
        let a = account(&db, id)?;
        let used:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM transactions WHERE account_id=?1 OR to_account_id=?1 UNION ALL SELECT 1 FROM bills WHERE account_id=?1)",[id],|r|r.get(0))?;
        if used || a.balance_minor != 0 {
            return Err(AppError::invalid(
                "Only an empty zero-balance account can be deleted. Archive it instead.",
            ));
        }
        db.execute("DELETE FROM accounts WHERE id=?1", [id])?;
        Ok(())
    }
}

const TRANSACTION_SQL:&str="SELECT t.*,a.name AS account_name,a.currency,b.name AS to_account_name,b.currency AS to_currency,c.name AS category_name,e.receipt_id FROM transactions t JOIN accounts a ON a.id=t.account_id LEFT JOIN accounts b ON b.id=t.to_account_id LEFT JOIN categories c ON c.id=t.category_id LEFT JOIN expenses e ON e.id=t.expense_id";
pub(crate) fn read_transaction(db: &Connection, id: &str) -> Result<Transaction> {
    one(db, &format!("{TRANSACTION_SQL} WHERE t.id=?1"), [id])
}
fn conversion(
    db: &Connection,
    w: &str,
    currency: &str,
    amount: i64,
    on: &str,
) -> Result<(Option<i64>, Option<String>, Option<String>)> {
    let w = workspace(db, w)?;
    if currency == w.base_currency {
        return Ok((Some(amount), Some("1".into()), Some(on.into())));
    }
    let rate:Option<(String,String)>=db.query_row("SELECT rate,rate_date FROM exchange_rates WHERE base=?1 AND quote=?2 AND requested_date=?3",params![currency,w.base_currency,on],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    match rate {
        Some((r, d)) => Ok((
            Some(convert_minor(amount, currency, &w.base_currency, &r)?),
            Some(r),
            Some(d),
        )),
        None => Ok((None, None, None)),
    }
}
fn reconvert_all(db: &Connection, w: &str) -> Result<()> {
    let ts: Vec<Transaction> = query(
        db,
        &format!("{TRANSACTION_SQL} WHERE t.workspace_id=?1"),
        [w],
    )?;
    for t in ts {
        let (base, rate, day) = conversion(db, w, &t.currency, t.amount_minor, &t.occurred_on)?;
        db.execute("UPDATE transactions SET base_amount_minor=?1,exchange_rate=?2,exchange_rate_date=?3,version=version+1,updated_at=?4 WHERE id=?5",params![base,rate,day,now(),t.id])?;
    }
    Ok(())
}
/// Used by business payments inside their enclosing SQLite transaction.
pub(crate) fn save_transaction_in(
    db: &Connection,
    input: &TransactionInput,
    source: TransactionSource,
) -> Result<Transaction> {
    workspace(db, &input.workspace_id)?;
    let a = check_account(db, &input.account_id, &input.workspace_id)?;
    money(input.amount_minor)?;
    date(&input.occurred_on)?;
    note(&input.note, 4000)?;
    if let Some(ref p) = input.payee {
        text(p, 300, "a payee")?;
    }
    if input.kind == TransactionKind::Transfer {
        let to = input
            .to_account_id
            .as_deref()
            .ok_or_else(|| AppError::invalid("Choose a receiving account."))?;
        let b = check_account(db, to, &input.workspace_id)?;
        if a.id == b.id || input.category_id.is_some() {
            return Err(AppError::invalid(
                "A transfer needs different accounts and no category.",
            ));
        }
        let amount = input
            .to_amount_minor
            .ok_or_else(|| AppError::invalid("Enter the amount received."))?;
        money(amount)?;
        if a.currency == b.currency && amount != input.amount_minor {
            return Err(AppError::invalid(
                "Same-currency transfers must send and receive the same amount.",
            ));
        }
    } else {
        if input.to_account_id.is_some() || input.to_amount_minor.is_some() {
            return Err(AppError::invalid(
                "Only transfers can have a receiving account.",
            ));
        }
        if let Some(ref c) = input.category_id {
            category(
                db,
                c,
                &input.workspace_id,
                if input.kind == TransactionKind::Income {
                    CategoryKind::Income
                } else {
                    CategoryKind::Expense
                },
            )?;
        }
    }
    let tid = input.id.clone().unwrap_or_else(id);
    let mut src = source;
    if input.id.is_some() {
        let old = read_transaction(db, &tid)?;
        if old.workspace_id != input.workspace_id {
            return Err(AppError::invalid("A transaction cannot move workspaces."));
        }
        conflict(input.version, old.version)?;
        if old.source == TransactionSource::Invoice {
            return Err(AppError::invalid("Edit this payment from its invoice."));
        }
        if old.expense_id.is_some()
            && (input.amount_minor != old.amount_minor
                || input.occurred_on != old.occurred_on
                || input.payee != old.payee
                || a.currency != old.currency
                || input.kind != old.kind)
        {
            return Err(AppError::invalid(
                "Edit amount, date and merchant on the linked receipt.",
            ));
        }
        if old.bill_id.is_some()
            && (input.kind != TransactionKind::Expense || a.currency != old.currency)
        {
            return Err(AppError::invalid(
                "A bill payment must stay an expense in its original currency.",
            ));
        }
        src = old.source;
    }
    let (base, rate, day) = conversion(
        db,
        &input.workspace_id,
        &a.currency,
        input.amount_minor,
        &input.occurred_on,
    )?;
    if input.id.is_some() {
        db.execute("UPDATE transactions SET kind=?1,account_id=?2,amount_minor=?3,to_account_id=?4,to_amount_minor=?5,occurred_on=?6,category_id=?7,payee=?8,note=?9,payment_method=?10,base_amount_minor=?11,exchange_rate=?12,exchange_rate_date=?13,version=version+1,updated_at=?14 WHERE id=?15",params![key(input.kind),input.account_id,input.amount_minor,input.to_account_id,input.to_amount_minor,input.occurred_on,input.category_id,input.payee,input.note,input.payment_method.map(key),base,rate,day,now(),tid])?;
    } else {
        db.execute("INSERT INTO transactions(id,workspace_id,kind,account_id,amount_minor,to_account_id,to_amount_minor,occurred_on,category_id,payee,note,payment_method,source,base_amount_minor,exchange_rate,exchange_rate_date,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)",params![tid,input.workspace_id,key(input.kind),input.account_id,input.amount_minor,input.to_account_id,input.to_amount_minor,input.occurred_on,input.category_id,input.payee,input.note,input.payment_method.map(key),key(src),base,rate,day,now()])?;
    }
    validate_balances(db)?;
    read_transaction(db, &tid)
}

impl AppService {
    pub fn transaction(&self, id: &str) -> Result<Transaction> {
        read_transaction(&*self.conn()?, id)
    }
    pub fn save_transaction(&self, input: TransactionInput) -> Result<Transaction> {
        if input.id.is_none()
            && !matches!(
                input.source,
                TransactionSource::Manual | TransactionSource::QuickAdd | TransactionSource::Voice
            )
        {
            return Err(AppError::invalid(
                "Use the original receipt, bill, invoice or suggestion to record this transaction.",
            ));
        }
        date(&input.occurred_on)?;
        let a = check_account(&*self.conn()?, &input.account_id, &input.workspace_id)?;
        let w = workspace(&*self.conn()?, &input.workspace_id)?;
        let _ = self.prefetch_rate(&a.currency, &w.base_currency, &input.occurred_on);
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let out = save_transaction_in(&tx, &input, input.source)?;
        refresh_budget_alerts(&tx, &input.workspace_id, &input.occurred_on[..7])?;
        tx.commit()?;
        Ok(out)
    }
    pub fn transactions(&self, f: TransactionFilter) -> Result<TransactionPage> {
        let db = self.conn()?;
        filtered(&db, &f)
    }
    pub fn delete_transactions(&self, ids: Vec<String>) -> Result<u32> {
        if ids.is_empty() || ids.len() > 500 {
            return Err(AppError::invalid("Choose 1 to 500 transactions."));
        }
        let unique: std::collections::BTreeSet<_> = ids.into_iter().collect();
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        for id in &unique {
            let t = read_transaction(&tx, id)?;
            if t.source == TransactionSource::Invoice {
                return Err(AppError::invalid(
                    "Remove invoice payments from their invoice.",
                ));
            }
            tx.execute("DELETE FROM transactions WHERE id=?1", [id])?;
        }
        validate_balances(&tx)?;
        tx.commit()?;
        Ok(unique.len() as u32)
    }
    pub fn record_expense(&self, input: RecordExpenseInput) -> Result<Transaction> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        if let Some(t) = query::<Transaction>(
            &tx,
            &format!("{TRANSACTION_SQL} WHERE t.expense_id=?1"),
            [&input.expense_id],
        )?
        .into_iter()
        .next()
        {
            return Ok(t);
        }
        let e = expense_repository::get(&tx, &input.expense_id)?;
        if !matches!(
            e.status,
            ExpenseStatus::Ready | ExpenseStatus::Submitted | ExpenseStatus::Archived
        ) {
            return Err(AppError::invalid(
                "Review and mark this receipt ready first.",
            ));
        }
        let a = account(&tx, &input.account_id)?;
        if e.currency.as_deref() != Some(&a.currency) {
            return Err(AppError::invalid(
                "Choose an account in the receipt's claimed currency.",
            ));
        }
        let on = e
            .occurred_at
            .clone()
            .ok_or_else(|| AppError::invalid("Add a date to the receipt first."))?;
        let amount = e
            .total_amount_minor
            .ok_or_else(|| AppError::invalid("Add a total to the receipt first."))?;
        let out = save_transaction_in(
            &tx,
            &TransactionInput {
                id: None,
                version: None,
                workspace_id: a.workspace_id,
                kind: TransactionKind::Expense,
                account_id: a.id,
                amount_minor: amount,
                to_account_id: None,
                to_amount_minor: None,
                occurred_on: on,
                category_id: input.category_id,
                payee: e.merchant_name,
                note: input.note,
                payment_method: input.payment_method,
                source: TransactionSource::Receipt,
            },
            TransactionSource::Receipt,
        )?;
        tx.execute(
            "UPDATE transactions SET expense_id=?1 WHERE id=?2",
            params![input.expense_id, out.id],
        )?;
        let out = read_transaction(&tx, &out.id)?;
        refresh_budget_alerts(&tx, &out.workspace_id, &out.occurred_on[..7])?;
        tx.commit()?;
        Ok(out)
    }
}
fn filtered(db: &Connection, f: &TransactionFilter) -> Result<TransactionPage> {
    workspace(db, &f.workspace_id)?;
    if !(1..=500).contains(&f.limit) {
        return Err(AppError::invalid("Choose a page size between 1 and 500."));
    }
    if let Some(ref v) = f.from {
        date(v)?;
    }
    if let Some(ref v) = f.to {
        date(v)?;
    }
    if f.from
        .as_ref()
        .zip(f.to.as_ref())
        .is_some_and(|(a, b)| a > b)
    {
        return Err(AppError::invalid(
            "The start date must precede the end date.",
        ));
    }
    let mut all:Vec<Transaction>=query(db,&format!("{TRANSACTION_SQL} WHERE t.workspace_id=?1 ORDER BY t.occurred_on DESC,t.created_at DESC,t.id"),[&f.workspace_id])?;
    let search = f.search.as_deref().unwrap_or("").to_lowercase();
    all.retain(|t| {
        f.from.as_ref().is_none_or(|v| t.occurred_on >= *v)
            && f.to.as_ref().is_none_or(|v| t.occurred_on <= *v)
            && f.account_id
                .as_ref()
                .is_none_or(|v| t.account_id == *v || t.to_account_id.as_ref() == Some(v))
            && f.category_id
                .as_ref()
                .is_none_or(|v| t.category_id.as_ref() == Some(v))
            && f.kind.is_none_or(|v| t.kind == v)
            && f.min_amount_minor.is_none_or(|v| t.amount_minor >= v)
            && f.max_amount_minor.is_none_or(|v| t.amount_minor <= v)
            && (search.is_empty()
                || format!("{} {}", t.payee.as_deref().unwrap_or(""), t.note)
                    .to_lowercase()
                    .contains(&search))
    });
    let total =
        u32::try_from(all.len()).map_err(|_| AppError::invalid("Too many transactions."))?;
    Ok(TransactionPage {
        items: all
            .into_iter()
            .skip(f.offset as usize)
            .take(f.limit as usize)
            .collect(),
        total,
    })
}

pub(crate) fn sync_receipt_in(db: &Connection, e: &Expense) -> Result<()> {
    let linked: Option<Transaction> = query(
        db,
        &format!("{TRANSACTION_SQL} WHERE t.expense_id=?1"),
        [&e.id],
    )?
    .into_iter()
    .next();
    if let Some(t) = linked {
        if e.currency.as_deref() != Some(&t.currency) {
            return Err(AppError::invalid("The linked ledger account uses another currency. Remove the ledger entry before changing receipt currency."));
        }
        let amount = e
            .total_amount_minor
            .ok_or_else(|| AppError::invalid("A recorded receipt must keep its total."))?;
        money(amount)?;
        let on = e
            .occurred_at
            .as_deref()
            .ok_or_else(|| AppError::invalid("A recorded receipt must keep its date."))?;
        date(on)?;
        let (base, rate, day) = conversion(db, &t.workspace_id, &t.currency, amount, on)?;
        db.execute("UPDATE transactions SET amount_minor=?1,occurred_on=?2,payee=?3,base_amount_minor=?4,exchange_rate=?5,exchange_rate_date=?6,version=version+1,updated_at=?7 WHERE id=?8",params![amount,on,e.merchant_name,base,rate,day,now(),t.id])?;
        validate_balances(db)?;
    }
    Ok(())
}
impl AppService {
    pub(crate) fn convert_pending_ledger(&self) -> Result<u32> {
        let waiting: Vec<Transaction> = query(
            &*self.conn()?,
            &format!("{TRANSACTION_SQL} WHERE t.base_amount_minor IS NULL LIMIT 50"),
            [],
        )?;
        let mut count = 0;
        for t in waiting {
            let w = workspace(&*self.conn()?, &t.workspace_id)?;
            if self
                .prefetch_rate(&t.currency, &w.base_currency, &t.occurred_on)
                .is_err()
            {
                continue;
            }
            let mut db = self.conn()?;
            let tx = db.transaction()?;
            let current = read_transaction(&tx, &t.id)?;
            if current.version != t.version {
                continue;
            }
            let (base, rate, day) = conversion(
                &tx,
                &t.workspace_id,
                &t.currency,
                t.amount_minor,
                &t.occurred_on,
            )?;
            if base.is_some() {
                tx.execute("UPDATE transactions SET base_amount_minor=?1,exchange_rate=?2,exchange_rate_date=?3,version=version+1,updated_at=?4 WHERE id=?5",params![base,rate,day,now(),t.id])?;
                refresh_budget_alerts(&tx, &t.workspace_id, &t.occurred_on[..7])?;
                tx.commit()?;
                count += 1;
            }
        }
        Ok(count)
    }
}

pub(crate) fn validate_balances(db: &Connection) -> Result<()> {
    let accounts: Vec<Account> = query(db, ACCOUNT_SQL, [])?;
    for a in accounts {
        safe(a.balance_minor)?;
        if (a.loan_direction == Some(LoanDirection::Borrowed) && a.balance_minor > 0)
            || (a.loan_direction == Some(LoanDirection::Lent) && a.balance_minor < 0)
        {
            return Err(AppError::invalid(
                "This payment exceeds the loan's outstanding balance.",
            ));
        }
        if a.archived_at.is_some() && a.balance_minor != 0 {
            return Err(AppError::invalid("This change would give an archived account a nonzero balance. Restore the account first."));
        }
    }
    Ok(())
}
