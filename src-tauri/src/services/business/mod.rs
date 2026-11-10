//! Local business documents. Every operation validates its workspace, and
//! invoice issue/payment/void writes are atomic with stock and ledger changes.
use super::AppService;
use crate::{
    domain::{
        business::*,
        expense::{currency_exponent, MAX_MONEY},
        id, now,
    },
    error::{AppError, Result},
};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

fn business(db: &Connection, workspace: &str) -> Result<()> {
    let kind: Option<String> = db
        .query_row(
            "SELECT kind FROM workspaces WHERE id=?1",
            [workspace],
            |r| r.get(0),
        )
        .optional()?;
    if kind.as_deref() != Some("business") {
        return Err(AppError::invalid(
            "Choose a business workspace for business records.",
        ));
    }
    Ok(())
}
fn text(value: &str, max: usize, required: bool, field: &str) -> Result<()> {
    if value.len() > max
        || (required && value.trim().is_empty())
        || value.chars().any(|c| c == '\0')
    {
        return Err(AppError::invalid(format!(
            "Check {field} (up to {max} characters)."
        )));
    }
    Ok(())
}
fn date(value: &str) -> Result<()> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::invalid("Enter a valid calendar date (YYYY-MM-DD)."))?;
    if parsed.format("%Y-%m-%d").to_string() != value {
        return Err(AppError::invalid("Enter a date as YYYY-MM-DD."));
    }
    Ok(())
}
fn money(value: i128) -> Result<i64> {
    i64::try_from(value)
        .ok()
        .filter(|v| (0..=MAX_MONEY).contains(v))
        .ok_or_else(|| AppError::invalid("The calculated amount is too large."))
}
fn version(got: Option<i32>, expected: i32) -> Result<()> {
    if got != Some(expected) {
        return Err(AppError::new(
            "Conflict",
            "This record changed. Reload before saving.",
        ));
    }
    Ok(())
}
fn uuid(value: &str) -> Result<()> {
    uuid::Uuid::parse_str(value).map(|_| ()).map_err(|_| {
        AppError::invalid("Use a UUID request id so retries can be identified safely.")
    })
}
fn profile_in(db: &Connection, workspace: &str) -> Result<Option<BusinessProfile>> {
    db.query_row(
        "SELECT data FROM business_profiles WHERE workspace_id=?1",
        [workspace],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|s| serde_json::from_str(&s).map_err(Into::into))
    .transpose()
}
fn customer_in(db: &Connection, workspace: &str, id: &str) -> Result<Customer> {
    let json: Option<String> = db
        .query_row(
            "SELECT data FROM customers WHERE workspace_id=?1 AND id=?2",
            params![workspace, id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&json.ok_or_else(AppError::not_found)?).map_err(Into::into)
}
fn item_in(db: &Connection, workspace: &str, id: &str) -> Result<InventoryItem> {
    let json: Option<String> = db
        .query_row(
            "SELECT data FROM inventory_items WHERE workspace_id=?1 AND id=?2",
            params![workspace, id],
            |r| r.get(0),
        )
        .optional()?;
    let mut item: InventoryItem = serde_json::from_str(&json.ok_or_else(AppError::not_found)?)?;
    item.stock_milli = db.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE workspace_id=?1 AND item_id=?2", params![workspace,id], |r| r.get(0))?;
    Ok(item)
}
fn invoice_in(db: &Connection, workspace: &str, id: &str) -> Result<Invoice> {
    let json: Option<String> = db
        .query_row(
            "SELECT data FROM invoices WHERE workspace_id=?1 AND id=?2",
            params![workspace, id],
            |r| r.get(0),
        )
        .optional()?;
    let mut invoice: Invoice = serde_json::from_str(&json.ok_or_else(AppError::not_found)?)?;
    let mut stmt = db.prepare("SELECT id,account_id,transaction_id,amount_minor,paid_on,note FROM invoice_payments WHERE workspace_id=?1 AND invoice_id=?2 ORDER BY paid_on,id")?;
    invoice.payments = stmt
        .query_map(params![workspace, id], |r| {
            Ok(InvoicePayment {
                id: r.get(0)?,
                account_id: r.get(1)?,
                transaction_id: r.get(2)?,
                amount_minor: r.get(3)?,
                paid_on: r.get(4)?,
                note: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    invoice.paid_minor = money(
        invoice
            .payments
            .iter()
            .map(|p| i128::from(p.amount_minor))
            .sum(),
    )?;
    Ok(invoice)
}
fn save_invoice_in(db: &Connection, invoice: &Invoice) -> Result<()> {
    let status = match invoice.status {
        InvoiceStatus::Draft => "draft",
        InvoiceStatus::Issued => "issued",
        InvoiceStatus::Paid => "paid",
        InvoiceStatus::Void => "void",
    };
    db.execute("INSERT INTO invoices(id,workspace_id,number,customer_id,status,data) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET number=excluded.number,customer_id=excluded.customer_id,status=excluded.status,data=excluded.data", params![invoice.id,invoice.workspace_id,invoice.number,invoice.customer_id,status,serde_json::to_string(invoice)?])?;
    Ok(())
}
fn lines(
    db: &Connection,
    workspace: &str,
    input: Vec<InvoiceLineInput>,
) -> Result<Vec<InvoiceLine>> {
    if input.is_empty() || input.len() > 200 {
        return Err(AppError::invalid("Add between 1 and 200 invoice lines."));
    }
    input
        .into_iter()
        .map(|line| {
            text(&line.description, 500, true, "the line description")?;
            if !(1..=999_999_999).contains(&line.quantity_milli)
                || !(0..=MAX_MONEY).contains(&line.unit_price_minor)
                || !(0..=MAX_MONEY).contains(&line.discount_minor)
                || !(0..=10000).contains(&line.tax_basis_points)
            {
                return Err(AppError::invalid(
                    "Check line quantity, price, discount and tax rate.",
                ));
            }
            if !line.classification_code.is_empty()
                && (line.classification_code.len() != 3
                    || !line.classification_code.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err(AppError::invalid(
                    "The classification code must contain three digits.",
                ));
            }
            if let Some(ref item_id) = line.inventory_item_id {
                if item_in(db, workspace, item_id)?.archived {
                    return Err(AppError::invalid("Choose an active inventory item."));
                }
            }
            let gross = money(
                (i128::from(line.quantity_milli) * i128::from(line.unit_price_minor) + 500) / 1000,
            )?;
            if line.discount_minor > gross {
                return Err(AppError::invalid(
                    "A line discount cannot exceed the line amount.",
                ));
            }
            let subtotal_minor = gross - line.discount_minor;
            let tax_minor = money(
                (i128::from(subtotal_minor) * i128::from(line.tax_basis_points) + 5000) / 10000,
            )?;
            let total_minor = money(i128::from(subtotal_minor) + i128::from(tax_minor))?;
            Ok(InvoiceLine {
                input: line,
                subtotal_minor,
                tax_minor,
                total_minor,
            })
        })
        .collect()
}
// A single row insert keeps all stock provenance explicit at each call site.
#[allow(clippy::too_many_arguments)]
fn movement(
    db: &Connection,
    workspace: &str,
    item_id: &str,
    quantity: i64,
    occurred: &str,
    reason: &str,
    invoice: Option<&str>,
    request: &str,
) -> Result<()> {
    db.execute("INSERT INTO stock_movements(id,workspace_id,item_id,request_id,quantity_milli,occurred_on,reason,invoice_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id(),workspace,item_id,request,quantity,occurred,reason,invoice,now()])?;
    Ok(())
}
impl AppService {
    pub fn get_business_profile(&self, workspace_id: &str) -> Result<Option<BusinessProfile>> {
        let db = self.conn()?;
        business(&db, workspace_id)?;
        profile_in(&db, workspace_id)
    }
    pub fn save_business_profile(&self, mut input: BusinessProfile) -> Result<BusinessProfile> {
        text(&input.legal_name, 200, true, "the legal name")?;
        text(&input.address, 1000, true, "the business address")?;
        for value in [
            &input.registration_number,
            &input.tin,
            &input.sst_registration_number,
            &input.email,
            &input.phone,
        ] {
            text(value, 200, false, "business details")?;
        }
        if input.invoice_prefix.is_empty()
            || input.invoice_prefix.len() > 12
            || !input
                .invoice_prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(AppError::invalid(
                "Use an invoice prefix of 1–12 letters, digits or hyphens.",
            ));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, &input.workspace_id)?;
        let old = profile_in(&tx, &input.workspace_id)?;
        version(Some(input.version), old.as_ref().map_or(0, |p| p.version))?;
        input.version += 1;
        tx.execute("INSERT INTO business_profiles(workspace_id,data) VALUES(?1,?2) ON CONFLICT(workspace_id) DO UPDATE SET data=excluded.data",params![input.workspace_id,serde_json::to_string(&input)?])?;
        tx.commit()?;
        Ok(input)
    }
    pub fn list_customers(
        &self,
        workspace_id: &str,
        include_archived: bool,
    ) -> Result<Vec<Customer>> {
        let db = self.conn()?;
        business(&db, workspace_id)?;
        let mut stmt=db.prepare("SELECT data FROM customers WHERE workspace_id=?1 ORDER BY json_extract(data,'$.name') COLLATE NOCASE,id")?;
        let raw = stmt
            .query_map([workspace_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|s| serde_json::from_str::<Customer>(&s).map_err(Into::into))
            .filter(|r| include_archived || r.as_ref().map_or(true, |c| !c.archived))
            .collect()
    }
    pub fn save_customer(&self, input: CustomerInput) -> Result<Customer> {
        text(&input.name, 200, true, "the customer name")?;
        text(&input.address, 1000, false, "the address")?;
        for value in [
            &input.registration_number,
            &input.tin,
            &input.email,
            &input.phone,
        ] {
            text(value, 200, false, "customer details")?;
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, &input.workspace_id)?;
        let old = input
            .id
            .as_deref()
            .map(|id| customer_in(&tx, &input.workspace_id, id))
            .transpose()?;
        if let Some(ref old) = old {
            version(input.version, old.version)?;
        }
        let customer = Customer {
            id: input.id.unwrap_or_else(id),
            workspace_id: input.workspace_id,
            name: input.name.trim().into(),
            registration_number: input.registration_number,
            tin: input.tin,
            address: input.address,
            email: input.email,
            phone: input.phone,
            archived: input.archived,
            version: old.map_or(1, |c| c.version + 1),
        };
        tx.execute("INSERT INTO customers(id,workspace_id,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![customer.id,customer.workspace_id,serde_json::to_string(&customer)?])?;
        tx.commit()?;
        Ok(customer)
    }
    pub fn list_inventory_items(
        &self,
        workspace_id: &str,
        include_archived: bool,
    ) -> Result<Vec<InventoryItem>> {
        let db = self.conn()?;
        business(&db, workspace_id)?;
        let mut stmt = db.prepare(
            "SELECT id FROM inventory_items WHERE workspace_id=?1 ORDER BY sku COLLATE NOCASE,id",
        )?;
        let ids = stmt
            .query_map([workspace_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| item_in(&db, workspace_id, &id))
            .filter(|r| include_archived || r.as_ref().map_or(true, |item| !item.archived))
            .collect()
    }
    pub fn save_inventory_item(&self, input: InventoryItemInput) -> Result<InventoryItem> {
        text(&input.name, 200, true, "the item name")?;
        text(&input.sku, 80, true, "the SKU")?;
        text(&input.unit, 30, true, "the unit")?;
        currency_exponent(&input.currency)?;
        money(i128::from(input.unit_price_minor))?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, &input.workspace_id)?;
        let old = input
            .id
            .as_deref()
            .map(|id| item_in(&tx, &input.workspace_id, id))
            .transpose()?;
        if let Some(ref old) = old {
            version(input.version, old.version)?;
            if old.stock_milli != 0 && input.archived {
                return Err(AppError::invalid(
                    "Adjust this item's stock to zero before archiving it.",
                ));
            }
        }
        let item = InventoryItem {
            id: input.id.unwrap_or_else(id),
            workspace_id: input.workspace_id,
            sku: input.sku.trim().into(),
            name: input.name.trim().into(),
            unit: input.unit.trim().into(),
            currency: input.currency,
            unit_price_minor: input.unit_price_minor,
            stock_milli: old.as_ref().map_or(0, |c| c.stock_milli),
            archived: input.archived,
            version: old.map_or(1, |c| c.version + 1),
        };
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM inventory_items WHERE workspace_id=?1 AND sku=?2 COLLATE NOCASE AND id<>?3)",params![item.workspace_id,item.sku,item.id],|r|r.get(0))?;
        if duplicate {
            return Err(AppError::invalid(
                "This business already has an item with that SKU.",
            ));
        }
        tx.execute("INSERT INTO inventory_items(id,workspace_id,sku,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET sku=excluded.sku,data=excluded.data",params![item.id,item.workspace_id,item.sku,serde_json::to_string(&item)?])?;
        tx.commit()?;
        Ok(item)
    }
    /// Deletes a customer that no invoice refers to; used customers are archived instead.
    pub fn delete_customer(&self, id: &str) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM invoices WHERE customer_id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if used {
            return Err(AppError::invalid(
                "This customer is on invoices. Archive the customer to keep invoice history.",
            ));
        }
        if tx.execute("DELETE FROM customers WHERE id=?1", [id])? == 0 {
            return Err(AppError::not_found());
        }
        tx.commit()?;
        Ok(())
    }
    /// Deletes an item with no stock history and on no invoice line; others are archived instead.
    pub fn delete_inventory_item(&self, id: &str) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM stock_movements WHERE item_id=?1)
              OR EXISTS(SELECT 1 FROM invoices i, json_tree(i.data) j
                         WHERE j.key='inventoryItemId' AND j.atom=?1)",
            [id],
            |r| r.get(0),
        )?;
        if used {
            return Err(AppError::invalid(
                "This item has stock history or is on an invoice. Archive it to keep that history.",
            ));
        }
        if tx.execute("DELETE FROM inventory_items WHERE id=?1", [id])? == 0 {
            return Err(AppError::not_found());
        }
        tx.commit()?;
        Ok(())
    }
    pub fn adjust_stock(&self, input: StockAdjustmentInput) -> Result<InventoryItem> {
        uuid(&input.request_id)?;
        date(&input.occurred_on)?;
        text(&input.reason, 500, true, "the adjustment reason")?;
        if input.quantity_milli == 0 || input.quantity_milli.unsigned_abs() > 999_999_999 {
            return Err(AppError::invalid(
                "Enter a non-zero stock adjustment within the supported range.",
            ));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, &input.workspace_id)?;
        let item = item_in(&tx, &input.workspace_id, &input.item_id)?;
        let existing:Option<(String,String,i64,String,String)>=tx.query_row("SELECT workspace_id,item_id,quantity_milli,occurred_on,reason FROM stock_movements WHERE request_id=?1",[&input.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        if let Some(old) = existing {
            if old
                != (
                    input.workspace_id.clone(),
                    input.item_id.clone(),
                    input.quantity_milli,
                    input.occurred_on.clone(),
                    input.reason.clone(),
                )
            {
                return Err(AppError::new(
                    "Conflict",
                    "This stock request id was already used for another adjustment.",
                ));
            }
            return Ok(item);
        }
        let next = i128::from(item.stock_milli) + i128::from(input.quantity_milli);
        if item.archived || !(0..=999_999_999).contains(&next) {
            return Err(AppError::invalid(
                "The item must be active and stock must remain between 0 and 999,999.999 units.",
            ));
        }
        movement(
            &tx,
            &input.workspace_id,
            &input.item_id,
            input.quantity_milli,
            &input.occurred_on,
            &input.reason,
            None,
            &input.request_id,
        )?;
        tx.commit()?;
        item_in(&db, &input.workspace_id, &input.item_id)
    }
    pub fn list_stock_movements(
        &self,
        workspace_id: &str,
        item_id: &str,
    ) -> Result<Vec<StockMovement>> {
        let db = self.conn()?;
        business(&db, workspace_id)?;
        item_in(&db, workspace_id, item_id)?;
        let mut stmt=db.prepare("SELECT id,workspace_id,item_id,quantity_milli,occurred_on,reason,invoice_id,created_at FROM stock_movements WHERE workspace_id=?1 AND item_id=?2 ORDER BY created_at DESC,id")?;
        let rows = stmt
            .query_map(params![workspace_id, item_id], |r| {
                Ok(StockMovement {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    item_id: r.get(2)?,
                    quantity_milli: r.get(3)?,
                    occurred_on: r.get(4)?,
                    reason: r.get(5)?,
                    invoice_id: r.get(6)?,
                    created_at: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    pub fn get_invoice(&self, workspace_id: &str, id: &str) -> Result<Invoice> {
        let db = self.conn()?;
        business(&db, workspace_id)?;
        invoice_in(&db, workspace_id, id)
    }
    pub fn list_invoices(&self, workspace_id: &str) -> Result<Vec<Invoice>> {
        let db = self.conn()?;
        business(&db, workspace_id)?;
        let mut stmt=db.prepare("SELECT id FROM invoices WHERE workspace_id=?1 ORDER BY json_extract(data,'$.issuedOn') DESC,id")?;
        let ids = stmt
            .query_map([workspace_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| invoice_in(&db, workspace_id, &id))
            .collect()
    }
    pub fn save_invoice(&self, input: InvoiceInput) -> Result<Invoice> {
        currency_exponent(&input.currency)?;
        date(&input.issued_on)?;
        date(&input.due_on)?;
        text(&input.note, 4000, false, "the invoice note")?;
        if input.due_on < input.issued_on {
            return Err(AppError::invalid(
                "The due date must be on or after the issue date.",
            ));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, &input.workspace_id)?;
        let old = input
            .id
            .as_deref()
            .map(|id| invoice_in(&tx, &input.workspace_id, id))
            .transpose()?;
        if let Some(ref old) = old {
            version(input.version, old.version)?;
            if old.status != InvoiceStatus::Draft {
                return Err(AppError::invalid("Only draft invoices can be edited. Void an unpaid issued invoice to correct it."));
            }
        }
        let customer = customer_in(&tx, &input.workspace_id, &input.customer_id)?;
        if customer.archived {
            return Err(AppError::invalid("Choose an active customer."));
        }
        let lines = lines(&tx, &input.workspace_id, input.lines)?;
        let subtotal_minor = money(lines.iter().map(|l| i128::from(l.subtotal_minor)).sum())?;
        let tax_minor = money(lines.iter().map(|l| i128::from(l.tax_minor)).sum())?;
        let total_minor = money(i128::from(subtotal_minor) + i128::from(tax_minor))?;
        if total_minor == 0 {
            return Err(AppError::invalid(
                "The invoice total must be greater than zero.",
            ));
        }
        let invoice = Invoice {
            id: input.id.unwrap_or_else(id),
            workspace_id: input.workspace_id,
            number: None,
            customer_id: input.customer_id,
            currency: input.currency,
            issued_on: input.issued_on,
            due_on: input.due_on,
            note: input.note,
            lines,
            subtotal_minor,
            tax_minor,
            total_minor,
            paid_minor: 0,
            status: InvoiceStatus::Draft,
            supplier: None,
            customer,
            version: old.as_ref().map_or(1, |i| i.version + 1),
            created_at: old.map_or_else(now, |i| i.created_at),
            updated_at: now(),
            payments: vec![],
        };
        save_invoice_in(&tx, &invoice)?;
        tx.commit()?;
        Ok(invoice)
    }
    pub fn issue_invoice(
        &self,
        workspace_id: &str,
        id: &str,
        expected_version: i32,
    ) -> Result<Invoice> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, workspace_id)?;
        let mut invoice = invoice_in(&tx, workspace_id, id)?;
        if matches!(invoice.status, InvoiceStatus::Issued | InvoiceStatus::Paid) {
            return Ok(invoice);
        }
        version(Some(expected_version), invoice.version)?;
        if invoice.status != InvoiceStatus::Draft {
            return Err(AppError::invalid("A void invoice cannot be issued again."));
        }
        let profile = profile_in(&tx, workspace_id)?.ok_or_else(|| {
            AppError::invalid("Save the business profile before issuing invoices.")
        })?;
        let customer = customer_in(&tx, workspace_id, &invoice.customer_id)?;
        if customer.archived {
            return Err(AppError::invalid(
                "Choose an active customer before issuing.",
            ));
        }
        let seq: i64 = tx.query_row(
            "SELECT next_invoice FROM business_profiles WHERE workspace_id=?1",
            [workspace_id],
            |r| r.get(0),
        )?;
        if seq > 999_999_999 {
            return Err(AppError::invalid(
                "The invoice numbering sequence is exhausted.",
            ));
        }
        invoice.number = Some(format!("{}-{seq:06}", profile.invoice_prefix));
        invoice.supplier = Some(profile);
        invoice.customer = customer;
        invoice.status = InvoiceStatus::Issued;
        invoice.version += 1;
        invoice.updated_at = now();
        let mut totals: BTreeMap<String, i64> = BTreeMap::new();
        for line in &invoice.lines {
            if let Some(ref item) = line.input.inventory_item_id {
                *totals.entry(item.clone()).or_default() += line.input.quantity_milli;
            }
        }
        for (item_id, qty) in &totals {
            let item = item_in(&tx, workspace_id, item_id)?;
            if item.archived || item.stock_milli < *qty {
                return Err(AppError::invalid(format!(
                    "There is not enough active stock for {}.",
                    item.name
                )));
            }
        }
        save_invoice_in(&tx, &invoice)?;
        for (item_id, qty) in totals {
            movement(
                &tx,
                workspace_id,
                &item_id,
                -qty,
                &invoice.issued_on,
                "Invoice issued",
                Some(id),
                &format!("issue:{id}:{item_id}"),
            )?;
        }
        tx.execute(
            "UPDATE business_profiles SET next_invoice=next_invoice+1 WHERE workspace_id=?1",
            [workspace_id],
        )?;
        tx.commit()?;
        Ok(invoice)
    }
    pub fn void_invoice(
        &self,
        workspace_id: &str,
        id: &str,
        expected_version: i32,
    ) -> Result<Invoice> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, workspace_id)?;
        let mut invoice = invoice_in(&tx, workspace_id, id)?;
        if invoice.status == InvoiceStatus::Void {
            return Ok(invoice);
        }
        version(Some(expected_version), invoice.version)?;
        if invoice.paid_minor > 0 {
            return Err(AppError::invalid("An invoice with payments cannot be voided. Payment reversal and credit notes are not supported yet."));
        }
        if invoice.status == InvoiceStatus::Issued {
            let mut totals: BTreeMap<String, i64> = BTreeMap::new();
            for line in &invoice.lines {
                if let Some(ref item) = line.input.inventory_item_id {
                    *totals.entry(item.clone()).or_default() += line.input.quantity_milli;
                }
            }
            for (item_id, qty) in totals {
                let stock = item_in(&tx, workspace_id, &item_id)?.stock_milli;
                if i128::from(stock) + i128::from(qty) > 999_999_999 {
                    return Err(AppError::invalid(
                        "Returning this stock would exceed the supported stock range.",
                    ));
                }
                movement(
                    &tx,
                    workspace_id,
                    &item_id,
                    qty,
                    &invoice.issued_on,
                    "Invoice voided",
                    Some(id),
                    &format!("void:{id}:{item_id}"),
                )?;
            }
        }
        invoice.status = InvoiceStatus::Void;
        invoice.version += 1;
        invoice.updated_at = now();
        save_invoice_in(&tx, &invoice)?;
        tx.commit()?;
        Ok(invoice)
    }
    pub fn record_invoice_payment(&self, input: InvoicePaymentInput) -> Result<Invoice> {
        uuid(&input.request_id)?;
        date(&input.paid_on)?;
        text(&input.note, 2000, false, "the payment note")?;
        if input.amount_minor <= 0 {
            return Err(AppError::invalid("Enter a positive payment amount."));
        }
        money(i128::from(input.amount_minor))?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        business(&tx, &input.workspace_id)?;
        let mut invoice = invoice_in(&tx, &input.workspace_id, &input.invoice_id)?;
        let existing:Option<(String,String,String,i64,String,String)>=tx.query_row("SELECT workspace_id,invoice_id,account_id,amount_minor,paid_on,note FROM invoice_payments WHERE request_id=?1",[&input.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        if let Some(old) = existing {
            if old
                != (
                    input.workspace_id.clone(),
                    input.invoice_id.clone(),
                    input.account_id.clone(),
                    input.amount_minor,
                    input.paid_on.clone(),
                    input.note.clone(),
                )
            {
                return Err(AppError::new(
                    "Conflict",
                    "This payment request id was already used for another payment.",
                ));
            }
            return Ok(invoice);
        }
        if invoice.status != InvoiceStatus::Issued
            || input.amount_minor > invoice.total_minor - invoice.paid_minor
        {
            return Err(AppError::invalid(
                "Pay an issued invoice without exceeding its outstanding balance.",
            ));
        }
        if input.paid_on < invoice.issued_on {
            return Err(AppError::invalid(
                "The payment date cannot precede the invoice issue date.",
            ));
        }
        let account: Option<(String, Option<String>, String)> = tx
            .query_row(
                "SELECT currency,archived_at,kind FROM accounts WHERE workspace_id=?1 AND id=?2",
                params![input.workspace_id, input.account_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (currency, archived, kind) = account.ok_or_else(AppError::not_found)?;
        if currency != invoice.currency || archived.is_some() || kind == "loan" {
            return Err(AppError::invalid("Choose an active, non-loan account in the invoice currency. Record currency exchange separately."));
        }
        let transaction = super::finance::save_transaction_in(
            &tx,
            &crate::domain::finance::TransactionInput {
                id: None,
                version: None,
                workspace_id: input.workspace_id.clone(),
                kind: crate::domain::finance::TransactionKind::Income,
                account_id: input.account_id.clone(),
                amount_minor: input.amount_minor,
                to_account_id: None,
                to_amount_minor: None,
                occurred_on: input.paid_on.clone(),
                category_id: None,
                payee: Some(invoice.customer.name.clone()),
                note: input.note.clone(),
                payment_method: None,
                source: crate::domain::finance::TransactionSource::Invoice,
            },
            crate::domain::finance::TransactionSource::Invoice,
        )?
        .id;
        let payment = id();
        let time = now();
        tx.execute("INSERT INTO invoice_payments(id,workspace_id,invoice_id,request_id,account_id,transaction_id,amount_minor,paid_on,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![payment,input.workspace_id,input.invoice_id,input.request_id,input.account_id,transaction,input.amount_minor,input.paid_on,input.note])?;
        invoice.paid_minor += input.amount_minor;
        if invoice.paid_minor == invoice.total_minor {
            invoice.status = InvoiceStatus::Paid;
        }
        invoice.version += 1;
        invoice.updated_at = time;
        save_invoice_in(&tx, &invoice)?;
        tx.commit()?;
        invoice_in(&db, &input.workspace_id, &input.invoice_id)
    }
    pub fn report_business(&self, workspace_id: &str, today: &str) -> Result<BusinessReport> {
        date(today)?;
        let invoices = self.list_invoices(workspace_id)?;
        let mut sums: BTreeMap<String, BusinessCurrencyTotal> = BTreeMap::new();
        let mut draft_count = 0;
        let mut issued_count = 0;
        for invoice in invoices {
            if invoice.status == InvoiceStatus::Draft {
                draft_count += 1;
                continue;
            }
            if invoice.status == InvoiceStatus::Void {
                continue;
            }
            issued_count += 1;
            let sum = sums
                .entry(invoice.currency.clone())
                .or_insert(BusinessCurrencyTotal {
                    currency: invoice.currency,
                    invoiced_minor: 0,
                    paid_minor: 0,
                    outstanding_minor: 0,
                    overdue_minor: 0,
                });
            sum.invoiced_minor =
                money(i128::from(sum.invoiced_minor) + i128::from(invoice.total_minor))?;
            sum.paid_minor = money(i128::from(sum.paid_minor) + i128::from(invoice.paid_minor))?;
            let outstanding = invoice.total_minor - invoice.paid_minor;
            sum.outstanding_minor =
                money(i128::from(sum.outstanding_minor) + i128::from(outstanding))?;
            if invoice.due_on.as_str() < today {
                sum.overdue_minor = money(i128::from(sum.overdue_minor) + i128::from(outstanding))?;
            }
        }
        Ok(BusinessReport {
            workspace_id: workspace_id.into(),
            as_of: today.into(),
            by_currency: sums.into_values().collect(),
            draft_count,
            issued_count,
        })
    }
}
