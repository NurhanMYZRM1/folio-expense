-- Binds each local workspace to one cloud account and keeps retry state across restarts.
ALTER TABLE cloud_control ADD COLUMN owner_id TEXT;
ALTER TABLE cloud_control ADD COLUMN providers TEXT;
ALTER TABLE cloud_control ADD COLUMN failure TEXT;
ALTER TABLE cloud_control ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cloud_control ADD COLUMN retry_at TEXT;
ALTER TABLE cloud_control ADD COLUMN request_id TEXT;
-- The change last sent for a record, kept so a lost reply is recognised on the next pull.
ALTER TABLE cloud_pending ADD COLUMN sent TEXT;
-- Schema 8 accepted sessions from the imported owner only, so a database that
-- ever connected belongs to that account. Untouched databases stay unbound.
UPDATE cloud_control SET owner_id='045ca64e-aa9e-4c29-8b9d-db45059beb29'
 WHERE email IS NOT NULL OR cursor>0 OR bootstrapped=1;
PRAGMA user_version=9;
