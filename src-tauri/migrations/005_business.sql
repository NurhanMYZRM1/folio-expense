-- Additive business documents. Issued documents are immutable snapshots;
-- only payments enter the finance ledger. Quantities use thousandths.
CREATE TABLE business_profiles (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
 data TEXT NOT NULL CHECK(json_valid(data)),
 next_invoice INTEGER NOT NULL DEFAULT 1 CHECK(next_invoice > 0)
);
CREATE TABLE customers (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 data TEXT NOT NULL CHECK(json_valid(data))
);
CREATE INDEX idx_customers_workspace ON customers(workspace_id);
CREATE TABLE inventory_items (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 sku TEXT NOT NULL COLLATE NOCASE,
 data TEXT NOT NULL CHECK(json_valid(data)),
 UNIQUE(workspace_id, sku)
);
CREATE TABLE invoices (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 number TEXT,
 customer_id TEXT NOT NULL REFERENCES customers(id),
 status TEXT NOT NULL CHECK(status IN ('draft','issued','paid','void')),
 data TEXT NOT NULL CHECK(json_valid(data)),
 UNIQUE(workspace_id, number)
);
CREATE INDEX idx_invoices_workspace ON invoices(workspace_id);
CREATE TABLE stock_movements (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 item_id TEXT NOT NULL REFERENCES inventory_items(id),
 request_id TEXT NOT NULL UNIQUE,
 quantity_milli INTEGER NOT NULL CHECK(quantity_milli BETWEEN -999999999 AND 999999999 AND quantity_milli <> 0),
 occurred_on TEXT NOT NULL,
 reason TEXT NOT NULL,
 invoice_id TEXT REFERENCES invoices(id),
 created_at TEXT NOT NULL
);
CREATE INDEX idx_stock_item ON stock_movements(item_id);
CREATE TABLE invoice_payments (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 invoice_id TEXT NOT NULL REFERENCES invoices(id),
 request_id TEXT NOT NULL UNIQUE,
 account_id TEXT NOT NULL REFERENCES accounts(id),
 transaction_id TEXT NOT NULL UNIQUE REFERENCES transactions(id),
 amount_minor INTEGER NOT NULL CHECK(amount_minor BETWEEN 1 AND 999999999999),
 paid_on TEXT NOT NULL,
 note TEXT NOT NULL
);
CREATE INDEX idx_invoice_payments_invoice ON invoice_payments(invoice_id);
PRAGMA user_version=5;
