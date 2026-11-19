-- Read-only owner-isolated mirror of the latest desktop SQLite v7 snapshot.
-- Writes remain disabled until domain validation and the sync client are implemented.

create table public."receipt_files" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "sha256" text not null,
 "original_filename" text not null,
 "mime_type" text not null,
 "relative_path" text not null,
 "size_bytes" bigint not null,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 unique (owner_id,"relative_path"),
 unique (owner_id,"sha256"),
 check (size_bytes>0),
 check (sync_state IN ('local_only','pending','synced','conflict'))
);

alter table public."receipt_files" enable row level security;

alter table public."receipt_files" force row level security;

revoke all on public."receipt_files" from public, anon, authenticated;

grant select on public."receipt_files" to authenticated;

create policy owner_read on public."receipt_files" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."expenses" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "receipt_id" text,
 "occurred_at" text,
 "merchant_name" text,
 "total_amount_minor" bigint,
 "tax_amount_minor" bigint,
 "currency" text,
 "category" text not null default 'Other',
 "description" text not null default '',
 "status" text not null,
 "extraction_confidence" double precision,
 "field_meta" text not null default '{}',
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 "original_currency" text,
 "original_total_amount_minor" bigint,
 "original_tax_amount_minor" bigint,
 "exchange_rate" text,
 "exchange_rate_date" text,
 "premises" text,
 primary key (owner_id,"id"),
 unique (owner_id,"receipt_id"),
 check (total_amount_minor BETWEEN 0 AND 999999999999),
 check (tax_amount_minor BETWEEN 0 AND 999999999999),
 check (status IN ('draft','extracting','needs_review','ready','submitted','archived')),
 check (sync_state IN ('local_only','pending','synced','conflict')),
 check (original_total_amount_minor BETWEEN 0 AND 999999999999),
 check (original_tax_amount_minor BETWEEN 0 AND 999999999999),
 check (premises IS NULL OR length(premises) <= 500),
 check (tax_amount_minor IS NULL OR total_amount_minor IS NULL OR tax_amount_minor<=total_amount_minor)
);

alter table public."expenses" enable row level security;

alter table public."expenses" force row level security;

revoke all on public."expenses" from public, anon, authenticated;

grant select on public."expenses" to authenticated;

create policy owner_read on public."expenses" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."extraction_runs" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "expense_id" text not null,
 "provider" text not null,
 "raw_text" text,
 "result_json" text,
 "error" text,
 "created_at" text not null,
 primary key (owner_id,"id")
);

alter table public."extraction_runs" enable row level security;

alter table public."extraction_runs" force row level security;

revoke all on public."extraction_runs" from public, anon, authenticated;

grant select on public."extraction_runs" to authenticated;

create policy owner_read on public."extraction_runs" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."expense_claims" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "claim_number" text not null,
 "title" text not null,
 "description" text not null default '',
 "status" text not null,
 "currency" text not null,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 unique (owner_id,"claim_number"),
 check (status IN ('draft','submitted','archived')),
 check (sync_state IN ('local_only','pending','synced','conflict'))
);

alter table public."expense_claims" enable row level security;

alter table public."expense_claims" force row level security;

revoke all on public."expense_claims" from public, anon, authenticated;

grant select on public."expense_claims" to authenticated;

create policy owner_read on public."expense_claims" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."claim_expenses" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "claim_id" text not null,
 "expense_id" text not null,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"claim_id","expense_id"),
 unique (owner_id,"expense_id")
);

alter table public."claim_expenses" enable row level security;

alter table public."claim_expenses" force row level security;

revoke all on public."claim_expenses" from public, anon, authenticated;

grant select on public."claim_expenses" to authenticated;

create policy owner_read on public."claim_expenses" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."audit_events" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "event_type" text not null,
 "entity_id" text not null,
 "details" text not null,
 "created_at" text not null,
 primary key (owner_id,"id")
);

alter table public."audit_events" enable row level security;

alter table public."audit_events" force row level security;

revoke all on public."audit_events" from public, anon, authenticated;

grant select on public."audit_events" to authenticated;

create policy owner_read on public."audit_events" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."jobs" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "job_type" text not null,
 "entity_id" text not null,
 "payload" text not null default '{}',
 "status" text not null,
 "attempts" bigint not null default 0,
 "last_error" text,
 "lease_token" text,
 "created_at" text not null,
 "updated_at" text not null,
 primary key (owner_id,"id"),
 check (job_type IN ('extract_receipt','generate_thumbnail','generate_pdf','future_sync')),
 check (status IN ('pending','running','completed','failed'))
);

alter table public."jobs" enable row level security;

alter table public."jobs" force row level security;

revoke all on public."jobs" from public, anon, authenticated;

grant select on public."jobs" to authenticated;

create policy owner_read on public."jobs" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."settings" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "key" text not null,
 "value" text not null,
 "version" bigint not null default 1,
 "updated_at" text not null,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"key")
);

alter table public."settings" enable row level security;

alter table public."settings" force row level security;

revoke all on public."settings" from public, anon, authenticated;

grant select on public."settings" to authenticated;

create policy owner_read on public."settings" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."exchange_rates" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "base" text not null,
 "quote" text not null,
 "requested_date" text not null,
 "rate_date" text not null,
 "rate" text not null,
 "source" text not null,
 "fetched_at" text not null,
 primary key (owner_id,"base","quote","requested_date")
);

alter table public."exchange_rates" enable row level security;

alter table public."exchange_rates" force row level security;

revoke all on public."exchange_rates" from public, anon, authenticated;

grant select on public."exchange_rates" to authenticated;

create policy owner_read on public."exchange_rates" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."workspaces" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "kind" text not null,
 "name" text not null,
 "base_currency" text not null,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 check (kind IN ('personal','business')),
 check (length(name) BETWEEN 1 AND 120),
 check (sync_state IN ('local_only','pending','synced','conflict'))
);

alter table public."workspaces" enable row level security;

alter table public."workspaces" force row level security;

revoke all on public."workspaces" from public, anon, authenticated;

grant select on public."workspaces" to authenticated;

create policy owner_read on public."workspaces" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."categories" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "kind" text not null,
 "name" text not null,
 "archived_at" text,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 unique (owner_id,"workspace_id","kind","name"),
 check (kind IN ('income','expense')),
 check (length(name) BETWEEN 1 AND 60),
 check (sync_state IN ('local_only','pending','synced','conflict'))
);

alter table public."categories" enable row level security;

alter table public."categories" force row level security;

revoke all on public."categories" from public, anon, authenticated;

grant select on public."categories" to authenticated;

create policy owner_read on public."categories" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."accounts" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "name" text not null,
 "kind" text not null,
 "currency" text not null,
 "opening_balance_minor" bigint not null default 0,
 "goal_target_minor" bigint,
 "goal_target_date" text,
 "loan_direction" text,
 "loan_principal_minor" bigint,
 "loan_annual_rate" text,
 "counterparty" text,
 "note" text not null default '',
 "archived_at" text,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 check (length(name) BETWEEN 1 AND 120),
 check (kind IN ('cash','bank','ewallet','card','savings','loan','other')),
 check (opening_balance_minor BETWEEN -999999999999 AND 999999999999),
 check (goal_target_minor IS NULL OR goal_target_minor BETWEEN 1 AND 999999999999),
 check (loan_direction IS NULL OR loan_direction IN ('borrowed','lent')),
 check (loan_principal_minor IS NULL OR loan_principal_minor BETWEEN 1 AND 999999999999),
 check (counterparty IS NULL OR length(counterparty) <= 120),
 check (length(note) <= 2000),
 check (sync_state IN ('local_only','pending','synced','conflict')),
 check ((kind='loan') = (loan_direction IS NOT NULL AND loan_principal_minor IS NOT NULL)),
 check (kind='savings' OR (goal_target_minor IS NULL AND goal_target_date IS NULL))
);

alter table public."accounts" enable row level security;

alter table public."accounts" force row level security;

revoke all on public."accounts" from public, anon, authenticated;

grant select on public."accounts" to authenticated;

create policy owner_read on public."accounts" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."bills" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "name" text not null,
 "kind" text not null default 'bill',
 "amount_minor" bigint not null,
 "account_id" text not null,
 "category_id" text,
 "payee" text,
 "note" text not null default '',
 "frequency" text not null,
 "interval_count" bigint not null default 1,
 "anchor_on" text not null,
 "ends_on" text,
 "remind_days_before" bigint not null default 3,
 "reminders_enabled" bigint not null default 1,
 "cancelled_on" text,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 check (length(name) BETWEEN 1 AND 120),
 check (kind IN ('bill','subscription')),
 check (amount_minor BETWEEN 1 AND 999999999999),
 check (payee IS NULL OR length(payee) <= 300),
 check (length(note) <= 2000),
 check (frequency IN ('once','weekly','monthly','yearly')),
 check (interval_count BETWEEN 1 AND 52),
 check (length(anchor_on)=10),
 check (ends_on IS NULL OR length(ends_on)=10),
 check (remind_days_before BETWEEN 0 AND 60),
 check (reminders_enabled IN (0,1)),
 check (cancelled_on IS NULL OR length(cancelled_on)=10),
 check (sync_state IN ('local_only','pending','synced','conflict'))
);

alter table public."bills" enable row level security;

alter table public."bills" force row level security;

revoke all on public."bills" from public, anon, authenticated;

grant select on public."bills" to authenticated;

create policy owner_read on public."bills" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."transactions" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "kind" text not null,
 "account_id" text not null,
 "amount_minor" bigint not null,
 "to_account_id" text,
 "to_amount_minor" bigint,
 "base_amount_minor" bigint,
 "exchange_rate" text,
 "exchange_rate_date" text,
 "occurred_on" text not null,
 "category_id" text,
 "payee" text,
 "note" text not null default '',
 "payment_method" text,
 "source" text not null default 'manual',
 "expense_id" text,
 "bill_id" text,
 "bill_due_on" text,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 unique (owner_id,"expense_id"),
 check (kind IN ('income','expense','transfer')),
 check (amount_minor BETWEEN 1 AND 999999999999),
 check (to_amount_minor IS NULL OR to_amount_minor BETWEEN 1 AND 999999999999),
 check (base_amount_minor IS NULL OR base_amount_minor BETWEEN 0 AND 999999999999),
 check (length(occurred_on)=10),
 check (payee IS NULL OR length(payee) <= 300),
 check (length(note) <= 4000),
 check (payment_method IS NULL OR payment_method IN ('cash','debit_card','credit_card','ewallet','bank_transfer','duitnow','fpx','cheque','other')),
 check (source IN ('manual','receipt','quick_add','voice','bill','invoice','notification')),
 check (sync_state IN ('local_only','pending','synced','conflict')),
 check ((kind='transfer') = (to_account_id IS NOT NULL)),
 check ((to_account_id IS NULL) = (to_amount_minor IS NULL)),
 check (to_account_id IS NULL OR to_account_id <> account_id),
 check (kind <> 'transfer' OR category_id IS NULL),
 check ((bill_id IS NULL) = (bill_due_on IS NULL))
);

alter table public."transactions" enable row level security;

alter table public."transactions" force row level security;

revoke all on public."transactions" from public, anon, authenticated;

grant select on public."transactions" to authenticated;

create policy owner_read on public."transactions" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."budgets" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "category_id" text,
 "amount_minor" bigint not null,
 "alert_percent" bigint not null default 80,
 "archived_at" text,
 "created_at" text not null,
 "updated_at" text not null,
 "version" bigint not null default 1,
 "sync_state" text not null default 'local_only',
 primary key (owner_id,"id"),
 check (amount_minor BETWEEN 1 AND 999999999999),
 check (alert_percent BETWEEN 1 AND 100),
 check (sync_state IN ('local_only','pending','synced','conflict'))
);

alter table public."budgets" enable row level security;

alter table public."budgets" force row level security;

revoke all on public."budgets" from public, anon, authenticated;

grant select on public."budgets" to authenticated;

create policy owner_read on public."budgets" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."budget_alerts" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "budget_id" text not null,
 "period" text not null,
 "threshold" bigint not null,
 "spent_minor" bigint not null,
 "created_at" text not null,
 "delivered_at" text,
 primary key (owner_id,"budget_id","period","threshold"),
 check (length(period)=7),
 check (threshold BETWEEN 1 AND 100)
);

alter table public."budget_alerts" enable row level security;

alter table public."budget_alerts" force row level security;

revoke all on public."budget_alerts" from public, anon, authenticated;

grant select on public."budget_alerts" to authenticated;

create policy owner_read on public."budget_alerts" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."transaction_suggestions" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "source" text not null,
 "fingerprint" text not null,
 "kind" text not null,
 "amount_minor" bigint not null,
 "currency" text not null,
 "occurred_on" text not null,
 "payee" text,
 "app_label" text not null,
 "status" text not null default 'pending',
 "transaction_id" text,
 "created_at" text not null,
 "updated_at" text not null,
 primary key (owner_id,"id"),
 unique (owner_id,"fingerprint"),
 check (source IN ('notification')),
 check (kind IN ('income','expense')),
 check (amount_minor BETWEEN 1 AND 999999999999),
 check (length(occurred_on)=10),
 check (payee IS NULL OR length(payee) <= 300),
 check (length(app_label) BETWEEN 1 AND 60),
 check (status IN ('pending','accepted','dismissed'))
);

alter table public."transaction_suggestions" enable row level security;

alter table public."transaction_suggestions" force row level security;

revoke all on public."transaction_suggestions" from public, anon, authenticated;

grant select on public."transaction_suggestions" to authenticated;

create policy owner_read on public."transaction_suggestions" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."business_profiles" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "workspace_id" text not null,
 "data" text not null,
 "next_invoice" bigint not null default 1,
 primary key (owner_id,"workspace_id"),
 check ((data::jsonb is not null)),
 check (next_invoice > 0)
);

alter table public."business_profiles" enable row level security;

alter table public."business_profiles" force row level security;

revoke all on public."business_profiles" from public, anon, authenticated;

grant select on public."business_profiles" to authenticated;

create policy owner_read on public."business_profiles" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."customers" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "data" text not null,
 primary key (owner_id,"id"),
 check ((data::jsonb is not null))
);

alter table public."customers" enable row level security;

alter table public."customers" force row level security;

revoke all on public."customers" from public, anon, authenticated;

grant select on public."customers" to authenticated;

create policy owner_read on public."customers" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."inventory_items" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "sku" text not null,
 "data" text not null,
 primary key (owner_id,"id"),
 unique (owner_id,"workspace_id","sku"),
 check ((data::jsonb is not null))
);

alter table public."inventory_items" enable row level security;

alter table public."inventory_items" force row level security;

revoke all on public."inventory_items" from public, anon, authenticated;

grant select on public."inventory_items" to authenticated;

create policy owner_read on public."inventory_items" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."invoices" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "number" text,
 "customer_id" text not null,
 "status" text not null,
 "data" text not null,
 primary key (owner_id,"id"),
 unique (owner_id,"workspace_id","number"),
 check (status IN ('draft','issued','paid','void')),
 check ((data::jsonb is not null))
);

alter table public."invoices" enable row level security;

alter table public."invoices" force row level security;

revoke all on public."invoices" from public, anon, authenticated;

grant select on public."invoices" to authenticated;

create policy owner_read on public."invoices" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."stock_movements" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "item_id" text not null,
 "request_id" text not null,
 "quantity_milli" bigint not null,
 "occurred_on" text not null,
 "reason" text not null,
 "invoice_id" text,
 "created_at" text not null,
 primary key (owner_id,"id"),
 unique (owner_id,"request_id"),
 check (quantity_milli BETWEEN -999999999 AND 999999999 AND quantity_milli <> 0)
);

alter table public."stock_movements" enable row level security;

alter table public."stock_movements" force row level security;

revoke all on public."stock_movements" from public, anon, authenticated;

grant select on public."stock_movements" to authenticated;

create policy owner_read on public."stock_movements" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."invoice_payments" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "invoice_id" text not null,
 "request_id" text not null,
 "account_id" text not null,
 "transaction_id" text not null,
 "amount_minor" bigint not null,
 "paid_on" text not null,
 "note" text not null,
 primary key (owner_id,"id"),
 unique (owner_id,"transaction_id"),
 unique (owner_id,"request_id"),
 check (amount_minor BETWEEN 1 AND 999999999999)
);

alter table public."invoice_payments" enable row level security;

alter table public."invoice_payments" force row level security;

revoke all on public."invoice_payments" from public, anon, authenticated;

grant select on public."invoice_payments" to authenticated;

create policy owner_read on public."invoice_payments" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."sync_connections" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "workspace_id" text not null,
 "account_id" text not null,
 "enabled" bigint not null default 0,
 "cursor" bigint not null default 0,
 "attempts" bigint not null default 0,
 "next_retry_at" text,
 "last_error" text,
 "updated_at" text not null,
 primary key (owner_id,"workspace_id"),
 check (enabled IN (0,1))
);

alter table public."sync_connections" enable row level security;

alter table public."sync_connections" force row level security;

revoke all on public."sync_connections" from public, anon, authenticated;

grant select on public."sync_connections" to authenticated;

create policy owner_read on public."sync_connections" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."sync_runtime" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" bigint not null,
 "applying" bigint not null default 0,
 primary key (owner_id,"id"),
 check (id=1),
 check (applying IN (0,1))
);

alter table public."sync_runtime" enable row level security;

alter table public."sync_runtime" force row level security;

revoke all on public."sync_runtime" from public, anon, authenticated;

grant select on public."sync_runtime" to authenticated;

create policy owner_read on public."sync_runtime" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."sync_versions" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "workspace_id" text not null,
 "entity" text not null,
 "record_id" text not null,
 "server_version" bigint not null default 0,
 "server_seq" bigint not null default 0,
 primary key (owner_id,"workspace_id","entity","record_id")
);

alter table public."sync_versions" enable row level security;

alter table public."sync_versions" force row level security;

revoke all on public."sync_versions" from public, anon, authenticated;

grant select on public."sync_versions" to authenticated;

create policy owner_read on public."sync_versions" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."sync_outbox" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "workspace_id" text not null,
 "entity" text not null,
 "record_id" text not null,
 "change_id" text not null,
 "base_version" bigint not null default 0,
 "operation" text not null,
 "data" text not null,
 "created_at" text not null,
 primary key (owner_id,"workspace_id","entity","record_id"),
 unique (owner_id,"change_id"),
 check (operation IN ('upsert','delete'))
);

alter table public."sync_outbox" enable row level security;

alter table public."sync_outbox" force row level security;

revoke all on public."sync_outbox" from public, anon, authenticated;

grant select on public."sync_outbox" to authenticated;

create policy owner_read on public."sync_outbox" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."sync_conflicts" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "id" text not null,
 "workspace_id" text not null,
 "entity" text not null,
 "record_id" text not null,
 "local_change" text not null,
 "server_record" text not null,
 "created_at" text not null,
 primary key (owner_id,"id")
);

alter table public."sync_conflicts" enable row level security;

alter table public."sync_conflicts" force row level security;

revoke all on public."sync_conflicts" from public, anon, authenticated;

grant select on public."sync_conflicts" to authenticated;

create policy owner_read on public."sync_conflicts" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."sync_quarantine" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "workspace_id" text not null,
 "server_seq" bigint not null,
 "record" text not null,
 "reason" text not null,
 "created_at" text not null,
 primary key (owner_id,"workspace_id","server_seq")
);

alter table public."sync_quarantine" enable row level security;

alter table public."sync_quarantine" force row level security;

revoke all on public."sync_quarantine" from public, anon, authenticated;

grant select on public."sync_quarantine" to authenticated;

create policy owner_read on public."sync_quarantine" for select to authenticated using ((select auth.uid()) = owner_id);

create table public."receipt_item_reviews" (
 owner_id uuid not null references auth.users(id) on delete cascade,
 "expense_id" text not null,
 "data" text not null,
 primary key (owner_id,"expense_id"),
 check ((data::jsonb is not null))
);

alter table public."receipt_item_reviews" enable row level security;

alter table public."receipt_item_reviews" force row level security;

revoke all on public."receipt_item_reviews" from public, anon, authenticated;

grant select on public."receipt_item_reviews" to authenticated;

create policy owner_read on public."receipt_item_reviews" for select to authenticated using ((select auth.uid()) = owner_id);

alter table public."expenses" add foreign key(owner_id,"receipt_id") references public."receipt_files"(owner_id,"id");

create index "fk_expenses_receipt_id" on public."expenses"(owner_id,"receipt_id");

alter table public."extraction_runs" add foreign key(owner_id,"expense_id") references public."expenses"(owner_id,"id");

create index "fk_extraction_runs_expense_id" on public."extraction_runs"(owner_id,"expense_id");

alter table public."claim_expenses" add foreign key(owner_id,"expense_id") references public."expenses"(owner_id,"id");

create index "fk_claim_expenses_expense_id" on public."claim_expenses"(owner_id,"expense_id");

alter table public."claim_expenses" add foreign key(owner_id,"claim_id") references public."expense_claims"(owner_id,"id");

create index "fk_claim_expenses_claim_id" on public."claim_expenses"(owner_id,"claim_id");

alter table public."categories" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_categories_workspace_id" on public."categories"(owner_id,"workspace_id");

alter table public."accounts" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_accounts_workspace_id" on public."accounts"(owner_id,"workspace_id");

alter table public."bills" add foreign key(owner_id,"category_id") references public."categories"(owner_id,"id");

create index "fk_bills_category_id" on public."bills"(owner_id,"category_id");

alter table public."bills" add foreign key(owner_id,"account_id") references public."accounts"(owner_id,"id");

create index "fk_bills_account_id" on public."bills"(owner_id,"account_id");

alter table public."bills" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_bills_workspace_id" on public."bills"(owner_id,"workspace_id");

alter table public."transactions" add foreign key(owner_id,"bill_id") references public."bills"(owner_id,"id");

create index "fk_transactions_bill_id" on public."transactions"(owner_id,"bill_id");

alter table public."transactions" add foreign key(owner_id,"expense_id") references public."expenses"(owner_id,"id");

create index "fk_transactions_expense_id" on public."transactions"(owner_id,"expense_id");

alter table public."transactions" add foreign key(owner_id,"category_id") references public."categories"(owner_id,"id");

create index "fk_transactions_category_id" on public."transactions"(owner_id,"category_id");

alter table public."transactions" add foreign key(owner_id,"to_account_id") references public."accounts"(owner_id,"id");

create index "fk_transactions_to_account_id" on public."transactions"(owner_id,"to_account_id");

alter table public."transactions" add foreign key(owner_id,"account_id") references public."accounts"(owner_id,"id");

create index "fk_transactions_account_id" on public."transactions"(owner_id,"account_id");

alter table public."transactions" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_transactions_workspace_id" on public."transactions"(owner_id,"workspace_id");

alter table public."budgets" add foreign key(owner_id,"category_id") references public."categories"(owner_id,"id");

create index "fk_budgets_category_id" on public."budgets"(owner_id,"category_id");

alter table public."budgets" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_budgets_workspace_id" on public."budgets"(owner_id,"workspace_id");

alter table public."budget_alerts" add foreign key(owner_id,"budget_id") references public."budgets"(owner_id,"id");

create index "fk_budget_alerts_budget_id" on public."budget_alerts"(owner_id,"budget_id");

alter table public."transaction_suggestions" add foreign key(owner_id,"transaction_id") references public."transactions"(owner_id,"id");

create index "fk_transaction_suggestions_transaction_id" on public."transaction_suggestions"(owner_id,"transaction_id");

alter table public."transaction_suggestions" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_transaction_suggestions_workspace_id" on public."transaction_suggestions"(owner_id,"workspace_id");

alter table public."business_profiles" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_business_profiles_workspace_id" on public."business_profiles"(owner_id,"workspace_id");

alter table public."customers" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_customers_workspace_id" on public."customers"(owner_id,"workspace_id");

alter table public."inventory_items" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_inventory_items_workspace_id" on public."inventory_items"(owner_id,"workspace_id");

alter table public."invoices" add foreign key(owner_id,"customer_id") references public."customers"(owner_id,"id");

create index "fk_invoices_customer_id" on public."invoices"(owner_id,"customer_id");

alter table public."invoices" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_invoices_workspace_id" on public."invoices"(owner_id,"workspace_id");

alter table public."stock_movements" add foreign key(owner_id,"invoice_id") references public."invoices"(owner_id,"id");

create index "fk_stock_movements_invoice_id" on public."stock_movements"(owner_id,"invoice_id");

alter table public."stock_movements" add foreign key(owner_id,"item_id") references public."inventory_items"(owner_id,"id");

create index "fk_stock_movements_item_id" on public."stock_movements"(owner_id,"item_id");

alter table public."stock_movements" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_stock_movements_workspace_id" on public."stock_movements"(owner_id,"workspace_id");

alter table public."invoice_payments" add foreign key(owner_id,"transaction_id") references public."transactions"(owner_id,"id");

create index "fk_invoice_payments_transaction_id" on public."invoice_payments"(owner_id,"transaction_id");

alter table public."invoice_payments" add foreign key(owner_id,"account_id") references public."accounts"(owner_id,"id");

create index "fk_invoice_payments_account_id" on public."invoice_payments"(owner_id,"account_id");

alter table public."invoice_payments" add foreign key(owner_id,"invoice_id") references public."invoices"(owner_id,"id");

create index "fk_invoice_payments_invoice_id" on public."invoice_payments"(owner_id,"invoice_id");

alter table public."invoice_payments" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_invoice_payments_workspace_id" on public."invoice_payments"(owner_id,"workspace_id");

alter table public."sync_connections" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_sync_connections_workspace_id" on public."sync_connections"(owner_id,"workspace_id");

alter table public."sync_versions" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_sync_versions_workspace_id" on public."sync_versions"(owner_id,"workspace_id");

alter table public."sync_outbox" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_sync_outbox_workspace_id" on public."sync_outbox"(owner_id,"workspace_id");

alter table public."sync_conflicts" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_sync_conflicts_workspace_id" on public."sync_conflicts"(owner_id,"workspace_id");

alter table public."sync_quarantine" add foreign key(owner_id,"workspace_id") references public."workspaces"(owner_id,"id");

create index "fk_sync_quarantine_workspace_id" on public."sync_quarantine"(owner_id,"workspace_id");

alter table public."receipt_item_reviews" add foreign key(owner_id,"expense_id") references public."expenses"(owner_id,"id");

create index "fk_receipt_item_reviews_expense_id" on public."receipt_item_reviews"(owner_id,"expense_id");

insert into storage.buckets(id,name,public,file_size_limit) values('folio-import','folio-import',false,104857600);

create table public.receipt_assets (
 owner_id uuid not null references auth.users(id) on delete cascade,
 receipt_id text not null,
 bucket_id text not null check(bucket_id='folio-import'),
 object_path text not null,
 source_relative_path text not null,
 sha256 text not null check(sha256 ~ '^[a-f0-9]{64}$'),
 size_bytes bigint not null check(size_bytes>0),
 primary key(owner_id,object_path),
 foreign key(owner_id,receipt_id) references public.receipt_files(owner_id,id)
);
create index receipt_assets_receipt on public.receipt_assets(owner_id,receipt_id);
alter table public.receipt_assets enable row level security;
alter table public.receipt_assets force row level security;
revoke all on public.receipt_assets from public,anon,authenticated;
grant select on public.receipt_assets to authenticated;
create policy owner_read on public.receipt_assets for select to authenticated using((select auth.uid())=owner_id);
create policy folio_import_owner_read on storage.objects for select to authenticated using (
 bucket_id='folio-import' and exists(
 select 1 from public.receipt_assets a where a.bucket_id=storage.objects.bucket_id
 and a.object_path=storage.objects.name and a.owner_id=(select auth.uid())));
