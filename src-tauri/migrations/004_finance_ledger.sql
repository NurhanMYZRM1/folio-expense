-- Personal and business finance ledger. See docs/DECISIONS.md (D1-D4).
-- Additive only: existing receipts, expenses and claims are untouched.
-- Every row below belongs to exactly one workspace.
CREATE TABLE workspaces (
 id TEXT PRIMARY KEY,
 kind TEXT NOT NULL CHECK(kind IN ('personal','business')),
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 120),
 base_currency TEXT NOT NULL,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict'))
);
CREATE UNIQUE INDEX idx_workspaces_one_personal ON workspaces(kind) WHERE kind='personal';

CREATE TABLE categories (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 kind TEXT NOT NULL CHECK(kind IN ('income','expense')),
 name TEXT NOT NULL COLLATE NOCASE CHECK(length(name) BETWEEN 1 AND 60),
 archived_at TEXT,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict')),
 UNIQUE(workspace_id, kind, name)
);

CREATE TABLE accounts (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 120),
 kind TEXT NOT NULL CHECK(kind IN ('cash','bank','ewallet','card','savings','loan','other')),
 currency TEXT NOT NULL,
 -- Signed: negative when money is owed (a card balance, a loan you borrowed).
 opening_balance_minor INTEGER NOT NULL DEFAULT 0 CHECK(opening_balance_minor BETWEEN -999999999999 AND 999999999999),
 -- Savings goal: progress is this account's balance.
 goal_target_minor INTEGER CHECK(goal_target_minor IS NULL OR goal_target_minor BETWEEN 1 AND 999999999999),
 goal_target_date TEXT,
 -- Loans: 'borrowed' carries a negative balance, 'lent' a positive one.
 loan_direction TEXT CHECK(loan_direction IS NULL OR loan_direction IN ('borrowed','lent')),
 loan_principal_minor INTEGER CHECK(loan_principal_minor IS NULL OR loan_principal_minor BETWEEN 1 AND 999999999999),
 loan_annual_rate TEXT,
 counterparty TEXT CHECK(counterparty IS NULL OR length(counterparty) <= 120),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note) <= 2000),
 archived_at TEXT,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict')),
 CHECK((kind='loan') = (loan_direction IS NOT NULL AND loan_principal_minor IS NOT NULL)),
 CHECK(kind='savings' OR (goal_target_minor IS NULL AND goal_target_date IS NULL))
);
CREATE INDEX idx_accounts_workspace ON accounts(workspace_id);

CREATE TABLE bills (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 120),
 kind TEXT NOT NULL DEFAULT 'bill' CHECK(kind IN ('bill','subscription')),
 -- Expected amount in the paying account's currency; a payment may differ.
 amount_minor INTEGER NOT NULL CHECK(amount_minor BETWEEN 1 AND 999999999999),
 account_id TEXT NOT NULL REFERENCES accounts(id),
 category_id TEXT REFERENCES categories(id),
 payee TEXT CHECK(payee IS NULL OR length(payee) <= 300),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note) <= 2000),
 frequency TEXT NOT NULL CHECK(frequency IN ('once','weekly','monthly','yearly')),
 interval_count INTEGER NOT NULL DEFAULT 1 CHECK(interval_count BETWEEN 1 AND 52),
 -- First due date of the current schedule. Monthly and yearly schedules keep
 -- its day of month, clamped to the last day of shorter months.
 anchor_on TEXT NOT NULL CHECK(length(anchor_on)=10),
 ends_on TEXT CHECK(ends_on IS NULL OR length(ends_on)=10),
 remind_days_before INTEGER NOT NULL DEFAULT 3 CHECK(remind_days_before BETWEEN 0 AND 60),
 reminders_enabled INTEGER NOT NULL DEFAULT 1 CHECK(reminders_enabled IN (0,1)),
 -- No occurrences after this date; paid history is kept.
 cancelled_on TEXT CHECK(cancelled_on IS NULL OR length(cancelled_on)=10),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict'))
);
CREATE INDEX idx_bills_workspace ON bills(workspace_id);

CREATE TABLE transactions (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 kind TEXT NOT NULL CHECK(kind IN ('income','expense','transfer')),
 account_id TEXT NOT NULL REFERENCES accounts(id),
 -- Always positive, in the account's currency; the kind gives the direction.
 amount_minor INTEGER NOT NULL CHECK(amount_minor BETWEEN 1 AND 999999999999),
 -- Transfers: the receiving account and the amount it received, in its currency.
 to_account_id TEXT REFERENCES accounts(id),
 to_amount_minor INTEGER CHECK(to_amount_minor IS NULL OR to_amount_minor BETWEEN 1 AND 999999999999),
 -- amount_minor in the workspace base currency, for budgets and reports.
 -- NULL until an exchange rate for occurred_on is available.
 base_amount_minor INTEGER CHECK(base_amount_minor IS NULL OR base_amount_minor BETWEEN 0 AND 999999999999),
 exchange_rate TEXT,
 exchange_rate_date TEXT,
 occurred_on TEXT NOT NULL CHECK(length(occurred_on)=10),
 category_id TEXT REFERENCES categories(id),
 payee TEXT CHECK(payee IS NULL OR length(payee) <= 300),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note) <= 4000),
 payment_method TEXT CHECK(payment_method IS NULL OR payment_method IN ('cash','debit_card','credit_card','ewallet','bank_transfer','duitnow','fpx','cheque','other')),
 source TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('manual','receipt','quick_add','voice','bill','invoice','notification')),
 -- Links: each source is recorded at most once.
 expense_id TEXT UNIQUE REFERENCES expenses(id),
 bill_id TEXT REFERENCES bills(id),
 bill_due_on TEXT,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict')),
 CHECK((kind='transfer') = (to_account_id IS NOT NULL)),
 CHECK((to_account_id IS NULL) = (to_amount_minor IS NULL)),
 CHECK(to_account_id IS NULL OR to_account_id <> account_id),
 CHECK(kind <> 'transfer' OR category_id IS NULL),
 CHECK((bill_id IS NULL) = (bill_due_on IS NULL))
);
CREATE UNIQUE INDEX idx_transactions_bill_occurrence ON transactions(bill_id, bill_due_on) WHERE bill_id IS NOT NULL;
CREATE INDEX idx_transactions_workspace_date ON transactions(workspace_id, occurred_on DESC, created_at DESC);
CREATE INDEX idx_transactions_account ON transactions(account_id);
CREATE INDEX idx_transactions_to_account ON transactions(to_account_id) WHERE to_account_id IS NOT NULL;
CREATE INDEX idx_transactions_category ON transactions(category_id);

CREATE TABLE budgets (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 -- NULL: all spending in the workspace.
 category_id TEXT REFERENCES categories(id),
 -- Per calendar month, in the workspace base currency.
 amount_minor INTEGER NOT NULL CHECK(amount_minor BETWEEN 1 AND 999999999999),
 alert_percent INTEGER NOT NULL DEFAULT 80 CHECK(alert_percent BETWEEN 1 AND 100),
 archived_at TEXT,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict'))
);
CREATE UNIQUE INDEX idx_budgets_scope ON budgets(workspace_id, IFNULL(category_id,'')) WHERE archived_at IS NULL;

-- One row per budget, month and threshold crossed, so an alert fires once.
-- Device-local: never synced.
CREATE TABLE budget_alerts (
 budget_id TEXT NOT NULL REFERENCES budgets(id) ON DELETE CASCADE,
 period TEXT NOT NULL CHECK(length(period)=7),
 threshold INTEGER NOT NULL CHECK(threshold BETWEEN 1 AND 100),
 spent_minor INTEGER NOT NULL,
 created_at TEXT NOT NULL,
 delivered_at TEXT,
 PRIMARY KEY(budget_id, period, threshold)
);

-- Transactions the app proposes (Android payment notifications) that the
-- user has not confirmed. Only parsed fields are kept, never notification
-- text. The fingerprint makes a repeated notification a no-op. Device-local.
CREATE TABLE transaction_suggestions (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 source TEXT NOT NULL CHECK(source IN ('notification')),
 fingerprint TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL CHECK(kind IN ('income','expense')),
 amount_minor INTEGER NOT NULL CHECK(amount_minor BETWEEN 1 AND 999999999999),
 currency TEXT NOT NULL,
 occurred_on TEXT NOT NULL CHECK(length(occurred_on)=10),
 payee TEXT CHECK(payee IS NULL OR length(payee) <= 300),
 app_label TEXT NOT NULL CHECK(length(app_label) BETWEEN 1 AND 60),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','dismissed')),
 transaction_id TEXT REFERENCES transactions(id) ON DELETE SET NULL,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
PRAGMA user_version=4;
