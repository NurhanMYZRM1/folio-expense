use super::*;
const BUDGET_SQL:&str="SELECT b.id,b.workspace_id,b.category_id,c.name AS category_name,b.amount_minor,b.alert_percent,b.archived_at,b.version,b.sync_state FROM budgets b LEFT JOIN categories c ON c.id=b.category_id";
fn budget(db: &Connection, id: &str) -> Result<Budget> {
    one(db, &format!("{BUDGET_SQL} WHERE b.id=?1"), [id])
}
fn progress(db: &Connection, w: &str, period: &str) -> Result<Vec<BudgetProgress>> {
    month(period)?;
    let workspace = workspace(db, w)?;
    let budgets: Vec<Budget> = query(
        db,
        &format!("{BUDGET_SQL} WHERE b.workspace_id=?1 AND b.archived_at IS NULL ORDER BY c.name"),
        [w],
    )?;
    let mut out = Vec::new();
    for b in budgets {
        let(spent,unconverted):(i64,u32)=db.query_row("SELECT COALESCE(SUM(base_amount_minor),0),SUM(CASE WHEN base_amount_minor IS NULL THEN 1 ELSE 0 END) FROM transactions WHERE workspace_id=?1 AND kind='expense' AND substr(occurred_on,1,7)=?2 AND (?3 IS NULL OR category_id=?3)",params![w,period,b.category_id],|r|Ok((r.get(0)?,r.get::<_,Option<u32>>(1)?.unwrap_or(0))))?;
        safe(spent)?;
        let percent = i32::try_from(
            (i128::from(spent) * 100 / i128::from(b.amount_minor)).min(i128::from(i32::MAX)),
        )
        .unwrap();
        let status = if spent > b.amount_minor {
            BudgetStatus::Over
        } else if percent >= b.alert_percent {
            BudgetStatus::NearLimit
        } else {
            BudgetStatus::OnTrack
        };
        out.push(BudgetProgress {
            remaining_minor: safe(b.amount_minor - spent)?,
            budget: b,
            period: period.into(),
            base_currency: workspace.base_currency.clone(),
            spent_minor: spent,
            percent,
            status,
            unconverted_count: unconverted,
        });
    }
    Ok(out)
}
pub(super) fn refresh_budget_alerts(db: &Connection, w: &str, period: &str) -> Result<()> {
    for p in progress(db, w, period)? {
        for threshold in [p.budget.alert_percent, 100] {
            if p.percent >= threshold {
                db.execute("INSERT OR IGNORE INTO budget_alerts(budget_id,period,threshold,spent_minor,created_at) VALUES(?1,?2,?3,?4,?5)",params![p.budget.id,period,threshold,p.spent_minor,now()])?;
            }
        }
    }
    Ok(())
}
impl AppService {
    pub fn budgets(&self, w: &str, period: &str) -> Result<Vec<BudgetProgress>> {
        progress(&*self.conn()?, w, period)
    }
    pub fn save_budget(&self, i: BudgetInput) -> Result<Budget> {
        money(i.amount_minor)?;
        if !(1..=100).contains(&i.alert_percent) {
            return Err(AppError::invalid(
                "Choose an alert threshold from 1 to 100 percent.",
            ));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        workspace(&tx, &i.workspace_id)?;
        if let Some(ref c) = i.category_id {
            category(&tx, c, &i.workspace_id, CategoryKind::Expense)?;
        }
        let bid = i.id.clone().unwrap_or_else(id);
        if i.id.is_some() {
            let b = budget(&tx, &bid)?;
            if b.workspace_id != i.workspace_id {
                return Err(AppError::invalid("A budget cannot move workspaces."));
            }
            conflict(i.version, b.version)?;
            tx.execute("UPDATE budgets SET category_id=?1,amount_minor=?2,alert_percent=?3,archived_at=?4,version=version+1,updated_at=?5 WHERE id=?6",params![i.category_id,i.amount_minor,i.alert_percent,i.archived.then(now),now(),bid])?;
        } else {
            tx.execute("INSERT INTO budgets(id,workspace_id,category_id,amount_minor,alert_percent,archived_at,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![bid,i.workspace_id,i.category_id,i.amount_minor,i.alert_percent,i.archived.then(now),now()])?;
        }
        let out = budget(&tx, &bid)?;
        tx.commit()?;
        Ok(out)
    }
    pub fn delete_budget(&self, id: &str) -> Result<()> {
        if self
            .conn()?
            .execute("DELETE FROM budgets WHERE id=?1", [id])?
            == 0
        {
            return Err(AppError::not_found());
        }
        Ok(())
    }
}
const BILL_SQL: &str =
    "SELECT b.*,a.currency,NULL AS next_due_on FROM bills b JOIN accounts a ON a.id=b.account_id";
fn raw_bill(db: &Connection, id: &str) -> Result<Bill> {
    one(db, &format!("{BILL_SQL} WHERE b.id=?1"), [id])
}
/// Calendar recurrence preserves its original anchor day across short months.
fn occurrence(b: &Bill, n: i32) -> Result<Option<NaiveDate>> {
    let anchor = date(&b.anchor_on)?;
    let step = n
        .checked_mul(b.interval_count)
        .ok_or_else(|| AppError::invalid("The schedule is too long."))?;
    let d = match b.frequency {
        Frequency::Once => {
            if n == 0 {
                Some(anchor)
            } else {
                None
            }
        }
        Frequency::Weekly => anchor.checked_add_signed(Duration::weeks(i64::from(step))),
        Frequency::Monthly | Frequency::Yearly => {
            let months = i64::from(step)
                * if b.frequency == Frequency::Yearly {
                    12
                } else {
                    1
                };
            let index = i64::from(anchor.year()) * 12 + i64::from(anchor.month0()) + months;
            let y = i32::try_from(index.div_euclid(12)).ok();
            let m = (index.rem_euclid(12) + 1) as u32;
            y.and_then(|y| NaiveDate::from_ymd_opt(y, m, 1))
                .map(|first| {
                    let last = (first.checked_add_months(chrono::Months::new(1)).unwrap()
                        - Duration::days(1))
                    .day();
                    NaiveDate::from_ymd_opt(first.year(), first.month(), anchor.day().min(last))
                        .unwrap()
                })
        }
    };
    let Some(d) = d else { return Ok(None) };
    if b.ends_on.as_ref().is_some_and(|end| d.to_string() > *end)
        || b.cancelled_on
            .as_ref()
            .is_some_and(|end| d.to_string() > *end)
    {
        return Ok(None);
    }
    Ok(Some(d))
}
fn all_occurrences(
    db: &Connection,
    b: &Bill,
    today: &str,
    end: NaiveDate,
) -> Result<Vec<BillOccurrence>> {
    let mut result = Vec::new();
    for n in 0..12000 {
        let Some(d) = occurrence(b, n)? else { break };
        if d > end {
            break;
        }
        let on = d.to_string();
        let payment: Option<(String, i64)> = db
            .query_row(
                "SELECT id,amount_minor FROM transactions WHERE bill_id=?1 AND bill_due_on=?2",
                params![b.id, on],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let status = if payment.is_some() {
            OccurrenceStatus::Paid
        } else if on.as_str() < today {
            OccurrenceStatus::Overdue
        } else if on == today {
            OccurrenceStatus::DueToday
        } else {
            OccurrenceStatus::Upcoming
        };
        let remind_on = if b.reminders_enabled {
            Some((d - Duration::days(i64::from(b.remind_days_before))).to_string())
        } else {
            None
        };
        result.push(BillOccurrence {
            bill_id: b.id.clone(),
            bill_name: b.name.clone(),
            kind: b.kind,
            due_on: on,
            amount_minor: b.amount_minor,
            currency: b.currency.clone(),
            status,
            transaction_id: payment.as_ref().map(|p| p.0.clone()),
            paid_amount_minor: payment.map(|p| p.1),
            remind_on,
        });
    }
    // Paid occurrences remain visible even after editing/re-anchoring a schedule.
    let paid: Vec<Transaction> = query(
        db,
        &format!("{TRANSACTION_SQL} WHERE t.bill_id=?1 AND t.bill_due_on<=?2"),
        params![b.id, end.to_string()],
    )?;
    for t in paid {
        let Some(due) = t.bill_due_on else { continue };
        if result.iter().any(|o| o.due_on == due) {
            continue;
        }
        result.push(BillOccurrence {
            bill_id: b.id.clone(),
            bill_name: b.name.clone(),
            kind: b.kind,
            due_on: due,
            amount_minor: b.amount_minor,
            currency: b.currency.clone(),
            status: OccurrenceStatus::Paid,
            transaction_id: Some(t.id),
            paid_amount_minor: Some(t.amount_minor),
            remind_on: None,
        });
    }
    result.sort_by(|a, b| a.due_on.cmp(&b.due_on));
    Ok(result)
}
fn with_next(db: &Connection, mut b: Bill) -> Result<Bill> {
    for n in 0..12000 {
        let Some(d) = occurrence(&b, n)? else { break };
        let paid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE bill_id=?1 AND bill_due_on=?2)",
            params![b.id, d.to_string()],
            |r| r.get(0),
        )?;
        if !paid {
            b.next_due_on = Some(d.to_string());
            break;
        }
    }
    Ok(b)
}
fn bill_list(db: &Connection, w: &str) -> Result<Vec<Bill>> {
    workspace(db, w)?;
    query::<Bill>(
        db,
        &format!("{BILL_SQL} WHERE b.workspace_id=?1 ORDER BY b.name"),
        [w],
    )?
    .into_iter()
    .map(|b| with_next(db, b))
    .collect()
}
fn upcoming(db: &Connection, w: &str, today: &str, days: i64) -> Result<Vec<BillOccurrence>> {
    let start = date(today)?;
    if !(0..=366).contains(&days) {
        return Err(AppError::invalid(
            "Choose a reminder horizon between 0 and 366 days.",
        ));
    }
    let mut all = Vec::new();
    for b in bill_list(db, w)? {
        all.extend(
            all_occurrences(db, &b, today, start + Duration::days(days))?
                .into_iter()
                .filter(|o| o.status != OccurrenceStatus::Paid),
        );
    }
    all.sort_by(|a, b| a.due_on.cmp(&b.due_on).then(a.bill_name.cmp(&b.bill_name)));
    Ok(all)
}
impl AppService {
    pub fn bills(&self, w: &str) -> Result<Vec<Bill>> {
        bill_list(&*self.conn()?, w)
    }
    pub fn bill(&self, id: &str, today: &str) -> Result<BillDetail> {
        let d = date(today)?;
        let db = self.conn()?;
        let b = with_next(&db, raw_bill(&db, id)?)?;
        let all = all_occurrences(&db, &b, today, d + Duration::days(1100))?;
        let mut past: Vec<_> = all
            .iter()
            .filter(|o| o.due_on.as_str() < today)
            .cloned()
            .rev()
            .take(12)
            .collect();
        past.reverse();
        past.extend(
            all.into_iter()
                .filter(|o| o.due_on.as_str() >= today)
                .take(3),
        );
        Ok(BillDetail {
            bill: b,
            occurrences: past,
        })
    }
    pub fn save_bill(&self, i: BillInput) -> Result<Bill> {
        text(&i.name, 120, "a bill name")?;
        money(i.amount_minor)?;
        let anchor = date(&i.anchor_on)?;
        for d in [&i.ends_on, &i.cancelled_on].into_iter().flatten() {
            date(d)?;
        }
        if i.ends_on.as_ref().is_some_and(|d| d < &i.anchor_on) {
            return Err(AppError::invalid(
                "The end date must not precede the first due date.",
            ));
        }
        if !(1..=52).contains(&i.interval_count) || !(0..=60).contains(&i.remind_days_before) {
            return Err(AppError::invalid("Choose a recurrence interval from 1 to 52 and reminder lead time from 0 to 60 days."));
        }
        note(&i.note, 2000)?;
        if let Some(ref p) = i.payee {
            text(p, 300, "a payee")?;
        }
        if anchor.year() < 1900 || anchor.year() > 2200 {
            return Err(AppError::invalid("Choose a bill date from 1900 to 2200."));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        check_account(&tx, &i.account_id, &i.workspace_id)?;
        if let Some(ref c) = i.category_id {
            category(&tx, c, &i.workspace_id, CategoryKind::Expense)?;
        }
        let bid = i.id.clone().unwrap_or_else(id);
        if i.id.is_some() {
            let b = raw_bill(&tx, &bid)?;
            if b.workspace_id != i.workspace_id {
                return Err(AppError::invalid("A bill cannot move workspaces."));
            }
            conflict(i.version, b.version)?;
            let a = account(&tx, &i.account_id)?;
            if a.currency != b.currency {
                return Err(AppError::invalid(
                    "Keep the bill's original currency to preserve payment history.",
                ));
            }
            tx.execute("UPDATE bills SET name=?1,kind=?2,amount_minor=?3,account_id=?4,category_id=?5,payee=?6,note=?7,frequency=?8,interval_count=?9,anchor_on=?10,ends_on=?11,remind_days_before=?12,reminders_enabled=?13,cancelled_on=?14,version=version+1,updated_at=?15 WHERE id=?16",params![i.name.trim(),key(i.kind),i.amount_minor,i.account_id,i.category_id,i.payee,i.note,key(i.frequency),i.interval_count,i.anchor_on,i.ends_on,i.remind_days_before,i.reminders_enabled,i.cancelled_on,now(),bid])?;
        } else {
            tx.execute("INSERT INTO bills(id,workspace_id,name,kind,amount_minor,account_id,category_id,payee,note,frequency,interval_count,anchor_on,ends_on,remind_days_before,reminders_enabled,cancelled_on,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)",params![bid,i.workspace_id,i.name.trim(),key(i.kind),i.amount_minor,i.account_id,i.category_id,i.payee,i.note,key(i.frequency),i.interval_count,i.anchor_on,i.ends_on,i.remind_days_before,i.reminders_enabled,i.cancelled_on,now()])?;
        }
        let out = with_next(&tx, raw_bill(&tx, &bid)?)?;
        tx.commit()?;
        Ok(out)
    }
    pub fn delete_bill(&self, id: &str) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        raw_bill(&tx, id)?;
        tx.execute("UPDATE transactions SET bill_id=NULL,bill_due_on=NULL,version=version+1,updated_at=?1 WHERE bill_id=?2",params![now(),id])?;
        tx.execute("DELETE FROM bills WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn bill_occurrences(&self, w: &str, today: &str, days: i64) -> Result<Vec<BillOccurrence>> {
        upcoming(&*self.conn()?, w, today, days)
    }
    pub fn pay_bill(&self, i: BillPaymentInput) -> Result<Transaction> {
        date(&i.paid_on)?;
        let due = date(&i.due_on)?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let b = raw_bill(&tx, &i.bill_id)?;
        if let Some(t) = query::<Transaction>(
            &tx,
            &format!("{TRANSACTION_SQL} WHERE t.bill_id=?1 AND t.bill_due_on=?2"),
            params![b.id, i.due_on],
        )?
        .into_iter()
        .next()
        {
            return Ok(t);
        }
        let mut valid = false;
        for n in 0..12000 {
            let Some(d) = occurrence(&b, n)? else { break };
            if d == due {
                valid = true;
                break;
            }
            if d > due {
                break;
            }
        }
        if !valid {
            return Err(AppError::invalid(
                "This due date is not in the bill's current schedule.",
            ));
        }
        let aid = i.account_id.unwrap_or(b.account_id);
        let a = check_account(&tx, &aid, &b.workspace_id)?;
        if a.currency != b.currency {
            return Err(AppError::invalid(
                "Choose a paying account in the bill's currency.",
            ));
        }
        let out = save_transaction_in(
            &tx,
            &TransactionInput {
                id: None,
                version: None,
                workspace_id: b.workspace_id,
                kind: TransactionKind::Expense,
                account_id: aid,
                amount_minor: i.amount_minor.unwrap_or(b.amount_minor),
                to_account_id: None,
                to_amount_minor: None,
                occurred_on: i.paid_on,
                category_id: b.category_id,
                payee: b.payee.or(Some(b.name)),
                note: i.note,
                payment_method: i.payment_method,
                source: TransactionSource::Bill,
            },
            TransactionSource::Bill,
        )?;
        tx.execute(
            "UPDATE transactions SET bill_id=?1,bill_due_on=?2 WHERE id=?3",
            params![b.id, i.due_on, out.id],
        )?;
        refresh_budget_alerts(&tx, &out.workspace_id, &out.occurred_on[..7])?;
        let out = read_transaction(&tx, &out.id)?;
        tx.commit()?;
        Ok(out)
    }
}
fn report(db: &Connection, w: &str, period: &str) -> Result<MonthlyReport> {
    let first = month(period)?;
    let workspace = workspace(db, w)?;
    let previous = (first - Duration::days(1)).format("%Y-%m").to_string();
    let ts:Vec<Transaction>=query(db,&format!("{TRANSACTION_SQL} WHERE t.workspace_id=?1 AND t.kind<>'transfer' AND substr(t.occurred_on,1,7) IN (?2,?3)"),params![w,period,previous])?;
    let mut income = 0_i64;
    let mut expense = 0_i64;
    let mut prev_income = 0_i64;
    let mut prev_expense = 0_i64;
    let mut unknown = 0;
    let mut totals: BTreeMap<(String, String), CategoryTotal> = BTreeMap::new();
    let mut daily = BTreeMap::new();
    let next = first.checked_add_months(chrono::Months::new(1)).unwrap();
    let mut d = first;
    while d < next {
        let s = d.to_string();
        daily.insert(
            s.clone(),
            DailyTotal {
                date: s,
                income_minor: 0,
                expense_minor: 0,
            },
        );
        d += Duration::days(1);
    }
    for t in ts {
        let current = t.occurred_on.starts_with(period);
        let Some(amount) = t.base_amount_minor else {
            if current {
                unknown += 1;
            }
            continue;
        };
        let income_kind = t.kind == TransactionKind::Income;
        if !current {
            if income_kind {
                prev_income = safe(prev_income + amount)?;
            } else {
                prev_expense = safe(prev_expense + amount)?;
            }
            continue;
        }
        if income_kind {
            income = safe(income + amount)?;
            daily.get_mut(&t.occurred_on).unwrap().income_minor += amount;
        } else {
            expense = safe(expense + amount)?;
            daily.get_mut(&t.occurred_on).unwrap().expense_minor += amount;
        }
        let k = (key(t.kind), t.category_id.clone().unwrap_or_default());
        let c = totals.entry(k).or_insert_with(|| CategoryTotal {
            category_id: t.category_id,
            name: t.category_name.unwrap_or("Uncategorised".into()),
            kind: if income_kind {
                CategoryKind::Income
            } else {
                CategoryKind::Expense
            },
            total_minor: 0,
            count: 0,
        });
        c.total_minor = safe(c.total_minor + amount)?;
        c.count += 1;
    }
    let mut by_category: Vec<_> = totals.into_values().collect();
    by_category.sort_by(|a, b| {
        key(a.kind)
            .cmp(&key(b.kind))
            .then(b.total_minor.cmp(&a.total_minor))
    });
    Ok(MonthlyReport {
        workspace_id: w.into(),
        period: period.into(),
        base_currency: workspace.base_currency,
        income_minor: income,
        expense_minor: expense,
        net_minor: safe(income - expense)?,
        previous_income_minor: prev_income,
        previous_expense_minor: prev_expense,
        by_category,
        daily: daily.into_values().collect(),
        unconverted_count: unknown,
    })
}
impl AppService {
    pub fn monthly_report(&self, w: &str, period: &str) -> Result<MonthlyReport> {
        report(&*self.conn()?, w, period)
    }
    pub fn overview(&self, w: &str, today: &str) -> Result<Overview> {
        date(today)?;
        let accounts = self.accounts(w)?;
        let db = self.conn()?;
        let workspace = workspace(&db, w)?;
        let r = report(&db, w, &today[..7])?;
        let budgets = progress(&db, w, &today[..7])?;
        let bills = upcoming(&db, w, today, 14)?;
        let recent = filtered(
            &db,
            &TransactionFilter {
                workspace_id: w.into(),
                limit: 8,
                ..Default::default()
            },
        )?
        .items;
        let pending_suggestions=db.query_row("SELECT count(*) FROM transaction_suggestions WHERE workspace_id=?1 AND status='pending'",[w],|r|r.get(0))?;
        let mut total = 0;
        let mut others: BTreeMap<String, i64> = BTreeMap::new();
        for a in &accounts {
            if a.archived_at.is_some() || a.kind == AccountKind::Loan {
                continue;
            }
            if a.currency == workspace.base_currency {
                total = safe(total + a.balance_minor)?;
            } else {
                let v = others.entry(a.currency.clone()).or_default();
                *v = safe(*v + a.balance_minor)?;
            }
        }
        Ok(Overview {
            workspace,
            today: today.into(),
            total_balance_minor: total,
            other_balances: others
                .into_iter()
                .map(|(currency, amount_minor)| CurrencyAmount {
                    currency,
                    amount_minor,
                })
                .collect(),
            month_income_minor: r.income_minor,
            month_expense_minor: r.expense_minor,
            accounts,
            budgets,
            bills,
            recent,
            pending_suggestions,
        })
    }
    pub fn reminders(&self, today: &str, days: i64) -> Result<Vec<Reminder>> {
        let today_date = date(today)?;
        if !(0..=366).contains(&days) {
            return Err(AppError::invalid(
                "Choose a reminder horizon between 0 and 366 days.",
            ));
        }
        let settings = self.settings()?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let ws: Vec<Workspace> = query(&tx, "SELECT * FROM workspaces", [])?;
        let mut out = Vec::new();
        for w in ws {
            if settings.notify_bills {
                for o in upcoming(&tx, &w.id, today, days + 60.min(366 - days))? {
                    if let Some(on) = o.remind_on {
                        if date(&on)? <= today_date + Duration::days(days) {
                            out.push(Reminder {
                                id: format!("bill:{}:{}", o.bill_id, o.due_on),
                                kind: ReminderKind::Bill,
                                workspace_id: w.id.clone(),
                                title: o.bill_name,
                                body: format!("Payment due {}", o.due_on),
                                fire_on: if on.as_str() < today {
                                    today.into()
                                } else {
                                    on
                                },
                            });
                        }
                    }
                }
            }
            if settings.notify_budgets {
                refresh_budget_alerts(&tx, &w.id, &today[..7])?;
                let rows: Vec<(String, String, i32)> = {
                    let mut stmt=tx.prepare("SELECT a.budget_id,a.period,a.threshold FROM budget_alerts a JOIN budgets b ON b.id=a.budget_id WHERE b.workspace_id=?1 AND b.archived_at IS NULL AND a.period=?2 AND a.delivered_at IS NULL")?;
                    let collected = stmt
                        .query_map(params![w.id, &today[..7]], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    collected
                };
                let current = progress(&tx, &w.id, &today[..7])?;
                for (id, p, threshold) in rows {
                    if !current
                        .iter()
                        .any(|p| p.budget.id == id && p.percent >= threshold)
                    {
                        continue;
                    }
                    out.push(Reminder {
                        id: format!("budget:{id}:{p}:{threshold}"),
                        kind: ReminderKind::Budget,
                        workspace_id: w.id.clone(),
                        title: "Budget alert".into(),
                        body: format!("Your monthly budget reached {threshold}%."),
                        fire_on: today.into(),
                    });
                }
            }
        }
        tx.commit()?;
        Ok(out)
    }
    pub fn ack_reminder(&self, id: &str) -> Result<()> {
        if let Some(rest) = id.strip_prefix("budget:") {
            let parts: Vec<_> = rest.rsplitn(3, ':').collect();
            if parts.len() != 3 {
                return Err(AppError::invalid("Invalid reminder."));
            }
            let threshold: i32 = parts[0]
                .parse()
                .map_err(|_| AppError::invalid("Invalid reminder."))?;
            self.conn()?.execute("UPDATE budget_alerts SET delivered_at=?1 WHERE budget_id=?2 AND period=?3 AND threshold=?4",params![now(),parts[2],parts[1],threshold])?;
        } else if !id.starts_with("bill:") {
            return Err(AppError::invalid("Invalid reminder."));
        }
        Ok(())
    }
}
