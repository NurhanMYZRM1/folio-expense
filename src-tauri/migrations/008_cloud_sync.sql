-- Durable automatic synchronization. Credentials remain in the native vault.
CREATE TABLE cloud_control(id INTEGER PRIMARY KEY CHECK(id=1),applying INTEGER NOT NULL DEFAULT 0,cursor INTEGER NOT NULL DEFAULT 0,bootstrapped INTEGER NOT NULL DEFAULT 0,email TEXT,last_sync TEXT,last_error TEXT,vault_id TEXT NOT NULL DEFAULT(lower(hex(randomblob(16)))));
INSERT INTO cloud_control(id) VALUES(1);
CREATE TABLE cloud_versions(entity TEXT NOT NULL,record_key TEXT NOT NULL,revision INTEGER NOT NULL,PRIMARY KEY(entity,record_key));
CREATE TABLE cloud_pending(entity TEXT NOT NULL,record_key TEXT NOT NULL,change_id TEXT NOT NULL,base_revision INTEGER NOT NULL,operation TEXT NOT NULL,data TEXT NOT NULL,PRIMARY KEY(entity,record_key));
CREATE TABLE cloud_conflicts(id TEXT PRIMARY KEY,entity TEXT NOT NULL,record_key TEXT NOT NULL,local_data TEXT NOT NULL,remote_record TEXT NOT NULL,choice TEXT,UNIQUE(entity,record_key));
CREATE TABLE cloud_uploaded(relative_path TEXT NOT NULL,sha256 TEXT NOT NULL,PRIMARY KEY(relative_path,sha256));
PRAGMA user_version=8;
