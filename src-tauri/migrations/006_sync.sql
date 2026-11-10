-- Opt-in synchronization metadata. No credentials or financial uploads occur in migration.
CREATE TABLE sync_connections (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id),
 account_id TEXT NOT NULL,
 enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
 cursor INTEGER NOT NULL DEFAULT 0,
 attempts INTEGER NOT NULL DEFAULT 0,
 next_retry_at TEXT,
 last_error TEXT,
 updated_at TEXT NOT NULL
);
CREATE TABLE sync_runtime (id INTEGER PRIMARY KEY CHECK(id=1), applying INTEGER NOT NULL DEFAULT 0 CHECK(applying IN (0,1)));
INSERT INTO sync_runtime(id,applying) VALUES(1,0);
CREATE TABLE sync_versions (
 workspace_id TEXT NOT NULL REFERENCES workspaces(id), entity TEXT NOT NULL, record_id TEXT NOT NULL,
 server_version INTEGER NOT NULL DEFAULT 0, server_seq INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(workspace_id,entity,record_id)
);
CREATE TABLE sync_outbox (
 workspace_id TEXT NOT NULL REFERENCES workspaces(id), entity TEXT NOT NULL, record_id TEXT NOT NULL,
 change_id TEXT NOT NULL UNIQUE, base_version INTEGER NOT NULL DEFAULT 0,
 operation TEXT NOT NULL CHECK(operation IN ('upsert','delete')), data TEXT NOT NULL,
 created_at TEXT NOT NULL,
 PRIMARY KEY(workspace_id,entity,record_id)
);
CREATE TABLE sync_conflicts (
 id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), entity TEXT NOT NULL, record_id TEXT NOT NULL,
 local_change TEXT NOT NULL, server_record TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE sync_quarantine (
 workspace_id TEXT NOT NULL REFERENCES workspaces(id), server_seq INTEGER NOT NULL,
 record TEXT NOT NULL, reason TEXT NOT NULL, created_at TEXT NOT NULL,
 PRIMARY KEY(workspace_id,server_seq)
);
-- Capture triggers are installed from introspected, allowlisted columns in sync::initialize_sync.
-- A disabled connection leaves existing and new records local-only.
PRAGMA user_version=6;
