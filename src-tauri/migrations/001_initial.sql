CREATE TABLE receipt_files (
 id TEXT PRIMARY KEY, sha256 TEXT NOT NULL UNIQUE, original_filename TEXT NOT NULL,
 mime_type TEXT NOT NULL, relative_path TEXT NOT NULL UNIQUE, size_bytes INTEGER NOT NULL CHECK(size_bytes>0),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict'))
);
CREATE TABLE expenses (
 id TEXT PRIMARY KEY, receipt_id TEXT UNIQUE REFERENCES receipt_files(id), occurred_at TEXT, merchant_name TEXT,
 total_amount_minor INTEGER CHECK(total_amount_minor BETWEEN 0 AND 999999999999),
 tax_amount_minor INTEGER CHECK(tax_amount_minor BETWEEN 0 AND 999999999999), currency TEXT, category TEXT NOT NULL DEFAULT 'Other',
 description TEXT NOT NULL DEFAULT '', status TEXT NOT NULL CHECK(status IN ('draft','extracting','needs_review','ready','submitted','archived')),
 extraction_confidence REAL, field_meta TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 version INTEGER NOT NULL DEFAULT 1, sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict')),
 CHECK(tax_amount_minor IS NULL OR total_amount_minor IS NULL OR tax_amount_minor<=total_amount_minor)
);
CREATE INDEX idx_expenses_date ON expenses(occurred_at DESC);
CREATE INDEX idx_expenses_status ON expenses(status);
CREATE INDEX idx_expenses_category ON expenses(category);
CREATE TABLE extraction_runs (
 id TEXT PRIMARY KEY, expense_id TEXT NOT NULL REFERENCES expenses(id), provider TEXT NOT NULL,
 raw_text TEXT, result_json TEXT, error TEXT, created_at TEXT NOT NULL
);
CREATE TABLE expense_claims (
 id TEXT PRIMARY KEY, claim_number TEXT NOT NULL UNIQUE, title TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
 status TEXT NOT NULL CHECK(status IN ('draft','submitted','archived')), currency TEXT NOT NULL,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_state IN ('local_only','pending','synced','conflict'))
);
CREATE TABLE claim_expenses (
 claim_id TEXT NOT NULL REFERENCES expense_claims(id), expense_id TEXT NOT NULL UNIQUE REFERENCES expenses(id),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 sync_state TEXT NOT NULL DEFAULT 'local_only', PRIMARY KEY(claim_id,expense_id)
);
CREATE TABLE audit_events (id TEXT PRIMARY KEY,event_type TEXT NOT NULL,entity_id TEXT NOT NULL,details TEXT NOT NULL,created_at TEXT NOT NULL);
CREATE INDEX idx_audit_entity ON audit_events(entity_id,created_at);
CREATE TABLE jobs (
 id TEXT PRIMARY KEY,job_type TEXT NOT NULL CHECK(job_type IN ('extract_receipt','generate_thumbnail','generate_pdf','future_sync')),
 entity_id TEXT NOT NULL,payload TEXT NOT NULL DEFAULT '{}',status TEXT NOT NULL CHECK(status IN ('pending','running','completed','failed')),
 attempts INTEGER NOT NULL DEFAULT 0,last_error TEXT,lease_token TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL
);
CREATE INDEX idx_jobs_status ON jobs(status,created_at);
CREATE UNIQUE INDEX idx_jobs_active ON jobs(job_type,entity_id) WHERE status IN ('pending','running');
CREATE TABLE settings (key TEXT PRIMARY KEY,value TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 1,updated_at TEXT NOT NULL,sync_state TEXT NOT NULL DEFAULT 'local_only');
PRAGMA user_version=1;
