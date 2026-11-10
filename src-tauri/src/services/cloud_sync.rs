//! Native, owner-bound Supabase synchronization. SQLite remains usable offline.
use super::AppService;
use crate::error::{AppError, Result};
use reqwest::blocking::{Client, Response};
use rusqlite::{params, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
const URL: &str = "https://jvzaufwijwdqqhmhroet.supabase.co";
const KEY: &str = "sb_publishable_fYEtE4iFTkI-_Nq_bETdAg_p2LlfRHO";
const OWNER: &str = "045ca64e-aa9e-4c29-8b9d-db45059beb29";
const EMAIL: &str = "paragonxplays@gmail.com";
const TABLES: &[&str] = &[
    "receipt_files",
    "expenses",
    "expense_claims",
    "claim_expenses",
    "extraction_runs",
    "audit_events",
    "exchange_rates",
    "workspaces",
    "categories",
    "accounts",
    "bills",
    "transactions",
    "budgets",
    "budget_alerts",
    "transaction_suggestions",
    "business_profiles",
    "customers",
    "inventory_items",
    "invoices",
    "stock_movements",
    "invoice_payments",
    "receipt_item_reviews",
];
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    signed_in: bool,
    email: Option<String>,
    pending: i64,
    conflicts: i64,
    last_sync: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct Session {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: u64,
    user: User,
}
/// The current access token, reused until shortly before it expires.
pub(crate) struct Access {
    token: Zeroizing<String>,
    until: Instant,
}
impl Session {
    fn access(&self) -> Access {
        Access {
            token: Zeroizing::new(self.access_token.clone()),
            until: Instant::now() + Duration::from_secs(self.expires_in.saturating_sub(120)),
        }
    }
}
#[derive(Deserialize)]
struct User {
    id: String,
}
fn fail(message: &str) -> AppError {
    AppError::new("CloudSync", message)
}
fn client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| fail("Cannot initialize the secure cloud connection."))
}
fn response(r: std::result::Result<Response, reqwest::Error>) -> Result<Response> {
    let r = r.map_err(|_| {
        fail("Cloud connection unavailable. Local changes are saved and will retry.")
    })?;
    if !r.status().is_success() {
        let refused = r.status().as_u16() == 401 || r.status().as_u16() == 403;
        let base = if refused {
            "Cloud access expired or was refused. Sign in again."
        } else {
            "Cloud request failed. Pending changes remain saved; retry later."
        };
        let mut body = Vec::new();
        let _ = r.take(4096).read_to_end(&mut body);
        let message = match server_reason(&body) {
            Some(why) => format!("{base} Supabase said: {why}"),
            None => base.to_string(),
        };
        return Err(AppError::new(
            if refused { "CloudAuth" } else { "CloudSync" },
            message,
        ));
    }
    Ok(r)
}
/// Supabase Auth reports `msg`, PostgREST `message`; either tells the user what to fix.
fn server_reason(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    ["msg", "message", "error_description"]
        .iter()
        .find_map(|k| v[*k].as_str())
        .map(|s| s.chars().take(300).collect())
}
/// Opening the email link signs the browser in instead; the address it lands on keeps that session.
fn redirect_session(link: &str) -> Result<Option<Zeroizing<String>>> {
    let Some(fragment) = reqwest::Url::parse(link)
        .ok()
        .and_then(|u| u.fragment().map(str::to_owned))
    else {
        return Ok(None);
    };
    let mut pairs =
        reqwest::Url::parse("http://fragment.invalid/").map_err(|_| fail("Invalid link."))?;
    pairs.set_query(Some(&fragment));
    let mut refresh = None;
    for (key, value) in pairs.query_pairs() {
        if key == "error_description" {
            return Err(fail(&format!(
                "Supabase rejected that sign-in link: {value}. Request a new sign-in email."
            )));
        }
        if key == "refresh_token" {
            refresh = Some(Zeroizing::new(value.into_owned()));
        }
    }
    Ok(refresh)
}
fn decode(r: Response) -> Result<Value> {
    let mut bytes = Vec::new();
    r.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(fail("Cloud response exceeded its safe size limit."));
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn columns(c: &Connection, t: &str) -> Result<Vec<(String, bool)>> {
    let mut s = c.prepare(&format!("PRAGMA table_info(\"{t}\")"))?;
    let rows = s.query_map([], |r| Ok((r.get(1)?, r.get::<_, i64>(5)? > 0)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
fn table(t: &str) -> Result<()> {
    if TABLES.contains(&t) {
        Ok(())
    } else {
        Err(fail("Unsupported cloud record type."))
    }
}
fn key_expr(cols: &[(String, bool)], prefix: &str) -> String {
    let mut cols = cols.to_vec();
    cols.sort_by(|a, b| a.0.cmp(&b.0));
    format!(
        "json_object({})",
        cols.iter()
            .filter(|c| c.1)
            .map(|c| format!("'{}',{prefix}.\"{}\"", c.0, c.0))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn row_expr(cols: &[(String, bool)], prefix: &str) -> String {
    format!(
        "json_object({})",
        cols.iter()
            .map(|c| format!("'{}',{prefix}.\"{}\"", c.0, c.0))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn canonical_key(c: &Connection, t: &str, k: &Value) -> Result<String> {
    let mut m = Map::new();
    for (name, pk) in columns(c, t)? {
        if pk {
            m.insert(
                name.clone(),
                k.get(&name)
                    .cloned()
                    .ok_or_else(|| fail("Cloud key is incomplete."))?,
            );
        }
    }
    Ok(serde_json::to_string(&m)?)
}
fn sql_value(v: &Value) -> Result<SqlValue> {
    Ok(match v {
        Value::Null => SqlValue::Null,
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else {
                SqlValue::Real(n.as_f64().ok_or_else(|| fail("Invalid cloud number."))?)
            }
        }
        _ => return Err(fail("Cloud row contains an unsupported column value.")),
    })
}
fn row(c: &Connection, t: &str, key: &Value) -> Result<Option<Value>> {
    table(t)?;
    let cols = columns(c, t)?;
    let keys: Vec<_> = cols.iter().filter(|x| x.1).collect();
    let where_sql = keys
        .iter()
        .map(|x| format!("\"{}\"=?", x.0))
        .collect::<Vec<_>>()
        .join(" AND ");
    let values = keys
        .iter()
        .map(|x| sql_value(&key[&x.0]))
        .collect::<Result<Vec<_>>>()?;
    let sql = format!(
        "SELECT {} FROM \"{t}\" r WHERE {where_sql}",
        row_expr(&cols, "r")
    );
    let data: Option<String> = c
        .query_row(&sql, rusqlite::params_from_iter(values), |r| r.get(0))
        .optional()?;
    data.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
fn apply(c: &Connection, record: &Value) -> Result<()> {
    let t = record["entity"]
        .as_str()
        .ok_or_else(|| fail("Missing cloud entity."))?;
    table(t)?;
    let cols = columns(c, t)?;
    let key = &record["key"];
    let keys: Vec<_> = cols.iter().filter(|x| x.1).collect();
    if record["deleted"].as_bool() == Some(true) {
        let predicates = keys
            .iter()
            .map(|x| format!("\"{}\"=?", x.0))
            .collect::<Vec<_>>()
            .join(" AND ");
        let values = keys
            .iter()
            .map(|x| sql_value(&key[&x.0]))
            .collect::<Result<Vec<_>>>()?;
        c.execute(
            &format!("DELETE FROM \"{t}\" WHERE {predicates}"),
            rusqlite::params_from_iter(values),
        )?;
    } else {
        let data = record["data"]
            .as_object()
            .ok_or_else(|| fail("Missing cloud payload."))?;
        let names = cols
            .iter()
            .map(|x| format!("\"{}\"", x.0))
            .collect::<Vec<_>>()
            .join(",");
        let values = cols
            .iter()
            .map(|x| {
                sql_value(
                    data.get(&x.0)
                        .ok_or_else(|| fail("Cloud payload is missing a column."))?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let updates = cols
            .iter()
            .filter(|x| !x.1)
            .map(|x| format!("\"{0}\"=excluded.\"{0}\"", x.0))
            .collect::<Vec<_>>()
            .join(",");
        let action = if updates.is_empty() {
            "DO NOTHING".into()
        } else {
            format!("DO UPDATE SET {updates}")
        };
        c.execute(
            &format!(
                "INSERT INTO \"{t}\"({names}) VALUES({}) ON CONFLICT {}",
                vec!["?"; cols.len()].join(","),
                action
            ),
            rusqlite::params_from_iter(values),
        )?;
    }
    Ok(())
}
fn set_version(c: &Connection, t: &str, k: &str, r: i64) -> Result<()> {
    c.execute("INSERT INTO cloud_versions VALUES(?1,?2,?3) ON CONFLICT(entity,record_key) DO UPDATE SET revision=excluded.revision",params![t,k,r])?;
    Ok(())
}
impl AppService {
    // Fresh databases have independent slots. A full database copy retains this identity.
    fn vault_account(&self) -> Result<String> {
        let id: String =
            self.conn()?
                .query_row("SELECT vault_id FROM cloud_control WHERE id=1", [], |r| {
                    r.get(0)
                })?;
        Ok(format!("supabase-refresh-v1-{id}"))
    }
    fn vault(&self) -> Result<keyring::Entry> {
        keyring::Entry::new("com.folio.expenses.cloud", &self.vault_account()?)
            .map_err(|_| fail("Secure cloud credential storage is unavailable."))
    }
    fn refresh_secret(&self) -> Result<Option<Zeroizing<String>>> {
        match self.vault()?.get_password() {
            Ok(s) => Ok(Some(Zeroizing::new(s))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(fail("Cannot unlock the secure cloud session.")),
        }
    }
    pub(crate) fn initialize_cloud(&self) -> Result<()> {
        let c = self.conn()?;
        for t in TABLES {
            let cols = columns(&c, t)?;
            for (kind, prefix, op) in [
                ("INSERT", "NEW", "upsert"),
                ("UPDATE", "NEW", "upsert"),
                ("DELETE", "OLD", "delete"),
            ] {
                let key = key_expr(&cols, prefix);
                let data = if op == "delete" {
                    "'{}'".into()
                } else {
                    row_expr(&cols, prefix)
                };
                c.execute_batch(&format!("DROP TRIGGER IF EXISTS cloud_{t}_{kind}; CREATE TRIGGER cloud_{t}_{kind} AFTER {kind} ON \"{t}\" WHEN (SELECT applying FROM cloud_control WHERE id=1)=0 BEGIN INSERT INTO cloud_pending(entity,record_key,change_id,base_revision,operation,data) VALUES('{t}',{key},(lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(6)))),coalesce((SELECT revision FROM cloud_versions WHERE entity='{t}' AND record_key={key}),0),'{op}',{data}) ON CONFLICT(entity,record_key) DO UPDATE SET change_id=excluded.change_id,base_revision=excluded.base_revision,operation=excluded.operation,data=excluded.data; END;"))?;
            }
        }
        Ok(())
    }
    pub fn cloud_status(&self) -> Result<CloudStatus> {
        let signed_in = self.refresh_secret()?.is_some();
        let c = self.conn()?;
        let (email, last_sync, error) = c.query_row(
            "SELECT email,last_sync,last_error FROM cloud_control WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(CloudStatus {
            signed_in,
            email,
            pending: c.query_row("SELECT count(*) FROM cloud_pending", [], |r| r.get(0))?,
            conflicts: c.query_row("SELECT count(*) FROM cloud_conflicts", [], |r| r.get(0))?,
            last_sync,
            error,
        })
    }
    fn store_session(&self, value: Value, email: &str, cache: &mut Option<Access>) -> Result<()> {
        let session: Session = serde_json::from_value(value)?;
        if session.user.id != OWNER {
            return Err(fail("This local workspace belongs to a different cloud account. Use its original owner account."));
        }
        self.vault()?
            .set_password(&session.refresh_token)
            .map_err(|_| fail("Unable to save the cloud session in the native vault."))?;
        *cache = Some(session.access());
        self.conn()?.execute(
            "UPDATE cloud_control SET email=?1,last_error=NULL WHERE id=1",
            [email],
        )?;
        Ok(())
    }
    pub fn cloud_sign_in(&self, email: &str, password: &str) -> Result<CloudStatus> {
        let mut guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        if !email.eq_ignore_ascii_case(EMAIL) {
            return Err(fail("Sign in with the owner of this imported workspace."));
        }
        let v = decode(response(
            client()?
                .post(format!("{URL}/auth/v1/token?grant_type=password"))
                .header("apikey", KEY)
                .json(&json!({"email":email,"password":password}))
                .send(),
        )?)?;
        self.store_session(v, email, &mut guard)?;
        self.cloud_status()
    }
    pub fn cloud_request_code(&self, email: &str) -> Result<CloudStatus> {
        if !email.eq_ignore_ascii_case(EMAIL) {
            return Err(fail("Use the email that owns this workspace."));
        }
        response(
            client()?
                .post(format!("{URL}/auth/v1/otp"))
                .header("apikey", KEY)
                .json(&json!({"email":email,"create_user":false}))
                .send(),
        )?;
        self.cloud_status()
    }
    pub fn cloud_verify_code(&self, email: &str, code: &str) -> Result<CloudStatus> {
        let mut guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        if !email.eq_ignore_ascii_case(EMAIL) {
            return Err(fail("Use the email that owns this workspace."));
        }
        if let Some(refresh) = redirect_session(code)? {
            let v = decode(response(
                client()?
                    .post(format!("{URL}/auth/v1/token?grant_type=refresh_token"))
                    .header("apikey", KEY)
                    .json(&json!({"refresh_token":refresh.as_str()}))
                    .send(),
            )?)?;
            self.store_session(v, email, &mut guard)?;
            return self.cloud_status();
        }
        let body = if code.starts_with("https://") {
            let u = reqwest::Url::parse(code).map_err(|_| fail("Invalid email link."))?;
            if u.host_str() != Some("jvzaufwijwdqqhmhroet.supabase.co")
                || u.path() != "/auth/v1/verify"
                || u.port().is_some()
                || !u.username().is_empty()
            {
                return Err(fail("Paste the sign-in link from this Folio project."));
            }
            let token_hash = u
                .query_pairs()
                .find(|(k, _)| k == "token")
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| fail("Email link has no verification token."))?;
            json!({"token_hash":token_hash,"type":"email"})
        } else {
            json!({"email":email,"token":code,"type":"email"})
        };
        let v = decode(response(
            client()?
                .post(format!("{URL}/auth/v1/verify"))
                .header("apikey", KEY)
                .json(&body)
                .send(),
        )?)?;
        self.store_session(v, email, &mut guard)?;
        self.cloud_status()
    }
    pub fn cloud_sign_out(&self) -> Result<CloudStatus> {
        let mut guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        if let Ok(token) = self.cloud_access(&mut guard) {
            let _ = client()?
                .post(format!("{URL}/auth/v1/logout?scope=local"))
                .header("apikey", KEY)
                .bearer_auth(token.as_str())
                .send();
        }
        *guard = None;
        match self.vault()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err(fail("Unable to clear the secure cloud session.")),
        }
        self.cloud_status()
    }
    fn cloud_access(&self, cache: &mut Option<Access>) -> Result<Zeroizing<String>> {
        if let Some(a) = cache.as_ref().filter(|a| Instant::now() < a.until) {
            return Ok(a.token.clone());
        }
        let refresh = self
            .refresh_secret()?
            .ok_or_else(|| fail("Sign in to enable cloud synchronization."))?;
        // Supabase revokes the session if a spent refresh token returns after 10 s, so a
        // lost reply is retried at once with a short timeout instead of at the next sync.
        let request = || -> Result<std::result::Result<Response, reqwest::Error>> {
            Ok(client()?
                .post(format!("{URL}/auth/v1/token?grant_type=refresh_token"))
                .header("apikey", KEY)
                .timeout(Duration::from_secs(6))
                .json(&json!({"refresh_token":refresh.as_str()}))
                .send())
        };
        let mut reply = request()?;
        if reply.is_err() {
            reply = request()?;
        }
        if reply.as_ref().is_ok_and(|r| r.status().as_u16() == 400) {
            // The refresh token was revoked or already used, so it can never work again.
            *cache = None;
            let _ = self.vault()?.delete_credential();
            return Err(fail(
                "Your cloud session ended. Sign in again to resume sync; local changes are kept.",
            ));
        }
        let value = decode(response(reply)?)?;
        let s: Session = serde_json::from_value(value)?;
        if s.user.id != OWNER {
            return Err(fail("Cloud owner verification failed."));
        }
        self.vault()?
            .set_password(&s.refresh_token)
            .map_err(|_| fail("Unable to refresh the secure session."))?;
        *cache = Some(s.access());
        Ok(Zeroizing::new(s.access_token))
    }
    fn rpc(&self, token: &str, name: &str, args: Value) -> Result<Value> {
        decode(response(
            client()?
                .post(format!("{URL}/rest/v1/rpc/{name}"))
                .header("apikey", KEY)
                .bearer_auth(token)
                .json(&args)
                .send(),
        )?)
    }
    pub fn cloud_sync(&self) -> Result<CloudStatus> {
        let mut guard = match self.cloud_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return self.cloud_status(),
        };
        let result = self.sync_cycle(&mut guard);
        if let Err(e) = result {
            if e.code == "CloudAuth" {
                *guard = None; // A refused access token is replaced on the next cycle.
            }
            self.conn()?.execute(
                "UPDATE cloud_control SET last_error=?1 WHERE id=1",
                [&e.message],
            )?;
        }
        self.cloud_status()
    }
    fn sync_cycle(&self, cache: &mut Option<Access>) -> Result<()> {
        let token = self.cloud_access(cache)?;
        self.pull(&token)?;
        self.seed()?;
        let pending = self.pending()?;
        if !pending.is_empty() {
            let replies = self.rpc(&token, "folio_sync_push", json!({"changes":pending}))?;
            let mut c = self.conn()?;
            let tx = c.transaction()?;
            for reply in replies
                .as_array()
                .ok_or_else(|| fail("Invalid push response."))?
            {
                let id = reply["changeId"]
                    .as_str()
                    .ok_or_else(|| fail("Missing push acknowledgement."))?;
                let sent = pending
                    .iter()
                    .find(|p| p["changeId"] == id)
                    .ok_or_else(|| fail("Unknown push acknowledgement."))?;
                let t = sent["entity"].as_str().unwrap();
                let key = canonical_key(&tx, t, &sent["key"])?;
                match reply["status"].as_str() {
                    Some("applied") => {
                        let revision = reply["revision"]
                            .as_i64()
                            .ok_or_else(|| fail("Missing revision."))?;
                        set_version(&tx, t, &key, revision)?;
                        tx.execute("DELETE FROM cloud_pending WHERE change_id=?1", [id])?;
                        tx.execute("UPDATE cloud_pending SET base_revision=?1 WHERE entity=?2 AND record_key=?3",params![revision,t,key])?;
                    }
                    Some("conflict") => {
                        record_conflict(&tx, t, &key, &sent["data"], &reply["record"])?;
                    }
                    Some("blocked") => {}
                    Some("rejected") if reply["reason"]=="duplicate_receipt" => return Err(fail("The same receipt was imported independently on another device. Your local edits are preserved. Compare both expense records and remove the duplicate local import before retrying sync.")),
                    _ => return Err(fail("Unknown cloud push result.")),
                }
            }
            tx.commit()?;
        }
        self.pull(&token)?;
        self.transfer_assets(&token)?;
        self.conn()?.execute(
            "UPDATE cloud_control SET last_sync=?1,last_error=NULL WHERE id=1",
            [chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
    fn pending(&self) -> Result<Vec<Value>> {
        let c = self.conn()?;
        let mut s=c.prepare("SELECT entity,record_key,change_id,base_revision,operation,data FROM cloud_pending WHERE NOT EXISTS(SELECT 1 FROM cloud_conflicts f WHERE f.entity=cloud_pending.entity AND f.record_key=cloud_pending.record_key) ORDER BY rowid LIMIT 2001")?;
        let rows = s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut result = Vec::new();
        for r in rows {
            let (t, k, id, v, op, d) = r?;
            result.push(json!({"entity":t,"key":serde_json::from_str::<Value>(&k)?,"changeId":id,"baseRevision":v,"operation":op,"data":serde_json::from_str::<Value>(&d)?}));
        }
        if result.len() > 2000 || serde_json::to_vec(&result)?.len() > 4 * 1024 * 1024 {
            return Err(fail("The pending change set exceeds one safe cloud transaction (2,000 records or 4 MB). Export your data and contact support to split it safely; no changes were discarded."));
        }
        Ok(result)
    }
    fn seed(&self) -> Result<()> {
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        let done: i64 = tx.query_row(
            "SELECT bootstrapped FROM cloud_control WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if done == 0 {
            for t in TABLES {
                let cols = columns(&tx, t)?;
                let key = key_expr(&cols, "r");
                let data = row_expr(&cols, "r");
                tx.execute_batch(&format!("INSERT OR IGNORE INTO cloud_pending SELECT '{t}',{key},(lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(6)))),0,'upsert',{data} FROM \"{t}\" r WHERE NOT EXISTS(SELECT 1 FROM cloud_versions v WHERE v.entity='{t}' AND v.record_key={key});"))?;
            }
            tx.execute("UPDATE cloud_control SET bootstrapped=1 WHERE id=1", [])?;
        }
        tx.commit()?;
        Ok(())
    }
    fn pull(&self, token: &str) -> Result<()> {
        for _ in 0..100 {
            let since: i64 =
                self.conn()?
                    .query_row("SELECT cursor FROM cloud_control WHERE id=1", [], |r| {
                        r.get(0)
                    })?;
            let page = self.rpc(
                token,
                "folio_sync_pull",
                json!({"since":since,"page_limit":500}),
            )?;
            let all_records = page["records"]
                .as_array()
                .ok_or_else(|| fail("Invalid pull response."))?;
            let mut latest = std::collections::BTreeMap::new();
            for record in all_records {
                latest.insert(
                    format!("{}:{}", record["entity"], record["key"]),
                    record.clone(),
                );
            }
            let records: Vec<Value> = latest.into_values().collect();
            let cursor = page["cursor"]
                .as_i64()
                .ok_or_else(|| fail("Missing cloud cursor."))?;
            if cursor < since {
                return Err(fail("Cloud cursor moved backwards."));
            }
            let mut c = self.conn()?;
            let tx = c.transaction()?;
            tx.execute_batch(
                "PRAGMA defer_foreign_keys=ON; UPDATE cloud_control SET applying=1 WHERE id=1;",
            )?;
            self.prune_default_workspace(&tx, &records)?;
            let mut conflict = false;
            for remote in &records {
                if let Some(local) = self.remote_conflict(&tx, remote)? {
                    let t = remote["entity"].as_str().unwrap();
                    let key = canonical_key(&tx, t, &remote["key"])?;
                    record_conflict(&tx, t, &key, &local, remote)?;
                    conflict = true;
                }
            }
            if conflict {
                tx.execute("UPDATE cloud_control SET applying=0 WHERE id=1", [])?;
                tx.commit()?;
                return Err(fail("Cloud changes need conflict review. The complete remote transaction is held until you choose which version to keep."));
            }
            for remote in &records {
                self.merge_remote(&tx, remote)?;
            }
            tx.execute(
                "UPDATE cloud_control SET applying=0,cursor=?1 WHERE id=1",
                [cursor],
            )?;
            tx.commit()?;
            if page["hasMore"].as_bool() != Some(true) {
                return Ok(());
            }
            if cursor == since {
                return Err(fail("Cloud pagination made no progress."));
            }
        }
        Err(fail("Cloud catch-up continues on the next sync."))
    }
    fn remote_conflict(&self, c: &Connection, remote: &Value) -> Result<Option<Value>> {
        let t = remote["entity"]
            .as_str()
            .ok_or_else(|| fail("Missing entity."))?;
        table(t)?;
        let key = canonical_key(c, t, &remote["key"])?;
        let known: Option<i64> = c
            .query_row(
                "SELECT revision FROM cloud_versions WHERE entity=?1 AND record_key=?2",
                params![t, key],
                |r| r.get(0),
            )
            .optional()?;
        let rev = remote["revision"]
            .as_i64()
            .ok_or_else(|| fail("Missing revision."))?;
        if known.is_some_and(|v| v >= rev) {
            return Ok(None);
        }
        let selected:Option<(Option<String>,String,String)>=c.query_row("SELECT choice,remote_record,local_data FROM cloud_conflicts WHERE entity=?1 AND record_key=?2",params![t,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let current_local = row(c, t, &remote["key"])?;
        let forced = selected.is_some_and(|(choice, saved, selected_local)| {
            choice.as_deref() == Some("remote")
                && serde_json::from_str::<Value>(&selected_local)
                    .is_ok_and(|v| v == current_local.clone().unwrap_or(Value::Null))
                && serde_json::from_str::<Value>(&saved)
                    .is_ok_and(|r| r["revision"] == remote["revision"])
        });
        if forced {
            return Ok(None);
        }
        let local = row(c, t, &remote["key"])?;
        let same = same_row(local.as_ref(), remote);
        let pending: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM cloud_pending WHERE entity=?1 AND record_key=?2)",
            params![t, key],
            |r| r.get(0),
        )?;
        if !same && (pending || (known.is_none() && local.is_some())) {
            Ok(Some(local.unwrap_or(Value::Null)))
        } else {
            Ok(None)
        }
    }
    fn prune_default_workspace(&self, c: &Connection, records: &[Value]) -> Result<()> {
        let initial: i64 = c.query_row(
            "SELECT bootstrapped FROM cloud_control WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if initial != 0 {
            return Ok(());
        }
        let remote_personal = records.iter().find(|r| {
            r["entity"] == "workspaces" && r["data"]["kind"] == "personal" && r["deleted"] != true
        });
        let Some(remote) = remote_personal else {
            return Ok(());
        };
        let id:Option<String>=c.query_row("SELECT id FROM workspaces WHERE kind='personal' AND name='Personal' AND version=1 AND created_at=updated_at AND (SELECT count(*) FROM workspaces)=1",[],|r|r.get(0)).optional()?;
        let Some(id) = id else {
            return Ok(());
        };
        if remote["key"]["id"] == id {
            return Ok(());
        }
        // Legacy expense/claim records do not reference finance workspaces or categories.
        // Only finance data prevents replacing this untouched, generated workspace.
        for table in [
            "accounts",
            "transactions",
            "bills",
            "budgets",
            "customers",
            "inventory_items",
            "invoices",
            "business_profiles",
            "stock_movements",
            "invoice_payments",
            "transaction_suggestions",
        ] {
            let count: i64 =
                c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            if count != 0 {
                return Ok(());
            }
        }
        let total: i64 = c.query_row(
            "SELECT count(*) FROM categories WHERE workspace_id=?1",
            [&id],
            |r| r.get(0),
        )?;
        let untouched:i64=c.query_row("SELECT count(*) FROM categories WHERE workspace_id=?1 AND version=1 AND archived_at IS NULL AND created_at=updated_at AND ((kind='income' AND name IN ('Salary','Business','Reimbursement','Other Income')) OR (kind='expense' AND name IN ('Meals','Transport','Accommodation','Fuel','Parking','Office Supplies','Travel','Entertainment','Software','Bills','Other')))",[&id],|r|r.get(0))?;
        if total != 15 || untouched != 15 {
            return Ok(());
        }
        c.execute("DELETE FROM cloud_pending WHERE entity='categories' AND json_extract(record_key,'$.id') IN (SELECT id FROM categories WHERE workspace_id=?1)",[&id])?;
        c.execute("DELETE FROM cloud_pending WHERE entity='workspaces' AND json_extract(record_key,'$.id')=?1",[&id])?;
        c.execute("DELETE FROM categories WHERE workspace_id=?1", [&id])?;
        c.execute("DELETE FROM workspaces WHERE id=?1", [&id])?;
        Ok(())
    }
    fn merge_remote(&self, c: &Connection, remote: &Value) -> Result<()> {
        let t = remote["entity"]
            .as_str()
            .ok_or_else(|| fail("Missing entity."))?;
        table(t)?;
        let key = canonical_key(c, t, &remote["key"])?;
        let revision = remote["revision"]
            .as_i64()
            .ok_or_else(|| fail("Missing revision."))?;
        let known: Option<i64> = c
            .query_row(
                "SELECT revision FROM cloud_versions WHERE entity=?1 AND record_key=?2",
                params![t, key],
                |r| r.get(0),
            )
            .optional()?;
        if known.is_some_and(|r| r >= revision) {
            return Ok(());
        }
        let selected:Option<(Option<String>,String,String)>=c.query_row("SELECT choice,remote_record,local_data FROM cloud_conflicts WHERE entity=?1 AND record_key=?2",params![t,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let current_local = row(c, t, &remote["key"])?;
        let forced = selected.is_some_and(|(choice, saved, selected_local)| {
            choice.as_deref() == Some("remote")
                && serde_json::from_str::<Value>(&selected_local)
                    .is_ok_and(|v| v == current_local.clone().unwrap_or(Value::Null))
                && serde_json::from_str::<Value>(&saved)
                    .is_ok_and(|r| r["revision"] == remote["revision"])
        });
        if forced {
            apply(c, remote)?;
            set_version(c, t, &key, revision)?;
            c.execute(
                "DELETE FROM cloud_pending WHERE entity=?1 AND record_key=?2",
                params![t, key],
            )?;
            c.execute(
                "DELETE FROM cloud_conflicts WHERE entity=?1 AND record_key=?2",
                params![t, key],
            )?;
            return Ok(());
        }
        let local = row(c, t, &remote["key"])?;
        let pending: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM cloud_pending WHERE entity=?1 AND record_key=?2)",
            params![t, key],
            |r| r.get(0),
        )?;
        let same = same_row(local.as_ref(), remote);
        if same {
            set_version(c, t, &key, revision)?;
            c.execute(
                "DELETE FROM cloud_pending WHERE entity=?1 AND record_key=?2",
                params![t, key],
            )?;
            return Ok(());
        }
        if pending || (known.is_none() && local.is_some()) {
            record_conflict(c, t, &key, &local.unwrap_or(Value::Null), remote)?;
            return Ok(());
        }
        apply(c, remote)?;
        set_version(c, t, &key, revision)
    }
    pub fn cloud_conflicts(&self) -> Result<Value> {
        let c = self.conn()?;
        let mut s = c.prepare(
            "SELECT id,entity,local_data,remote_record FROM cloud_conflicts ORDER BY rowid",
        )?;
        let rows = s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut list = Vec::new();
        for r in rows {
            let (id, entity, local, remote) = r?;
            list.push(json!({"id":id,"entity":entity,"local":serde_json::from_str::<Value>(&local)?,"remote":serde_json::from_str::<Value>(&remote)?}));
        }
        Ok(Value::Array(list))
    }
    pub fn cloud_resolve_conflict(&self, id: &str, choice: &str) -> Result<CloudStatus> {
        let _guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        let (t, key, remote): (String, String, String) = tx.query_row(
            "SELECT entity,record_key,remote_record FROM cloud_conflicts WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let record: Value = serde_json::from_str(&remote)?;
        let revision = record["revision"]
            .as_i64()
            .ok_or_else(|| fail("Invalid conflict revision."))?;
        if choice == "remote" {
            // Stage the decision: apply alongside the entire pulled aggregate,
            // which may include newly created parent rows not present locally yet.
            let current = row(&tx, &t, &record["key"])?;
            tx.execute(
                "UPDATE cloud_conflicts SET choice='remote',local_data=?2 WHERE id=?1",
                params![id, serde_json::to_string(&current.unwrap_or(Value::Null))?],
            )?;
            tx.commit()?;
            drop(c);
            return self.cloud_status();
        } else if choice == "local" {
            let current = row(&tx, &t, &record["key"])?;
            tx.execute("INSERT INTO cloud_pending VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(entity,record_key) DO UPDATE SET change_id=excluded.change_id,base_revision=excluded.base_revision,operation=excluded.operation,data=excluded.data",params![t,key,uuid::Uuid::new_v4().to_string(),revision,if current.is_some(){"upsert"}else{"delete"},serde_json::to_string(&current.unwrap_or(json!({})))?])?;
        } else {
            return Err(fail("Choose local or remote to resolve this conflict."));
        }
        set_version(&tx, &t, &key, revision)?;
        tx.execute("DELETE FROM cloud_conflicts WHERE id=?1", [id])?;
        tx.commit()?;
        drop(c);
        self.cloud_status()
    }
    fn transfer_assets(&self, token: &str) -> Result<()> {
        let assets = self.rpc(token, "folio_assets", json!({}))?;
        let list = assets
            .as_array()
            .ok_or_else(|| fail("Invalid receipt asset manifest."))?;
        for a in list {
            let relative = a["relative_path"]
                .as_str()
                .ok_or_else(|| fail("Missing receipt path."))?;
            let receipt = a["receipt_id"]
                .as_str()
                .ok_or_else(|| fail("Missing receipt identity."))?;
            if uuid::Uuid::parse_str(receipt).is_err()
                || relative.rsplit('/').next().is_none_or(|name| {
                    !safe_asset_name(name) || relative != format!("receipts/{receipt}/{name}")
                })
            {
                return Err(fail("Cloud receipt path does not match its identity."));
            }
            let exists: bool = self.conn()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM receipt_files WHERE id=?1)",
                [receipt],
                |r| r.get(0),
            )?;
            if !exists {
                continue;
            }
            let target = self.paths.resolve(relative)?;
            let hash = a["sha256"]
                .as_str()
                .ok_or_else(|| fail("Missing receipt checksum."))?;
            let size = a["size_bytes"]
                .as_u64()
                .ok_or_else(|| fail("Missing receipt size."))?;
            if size > 26_214_400 {
                return Err(fail("Cloud receipt exceeds the supported file limit."));
            }
            if target.exists() {
                if digest(&fs::read(&target)?) == hash {
                    self.mark_uploaded(relative, hash)?;
                    continue;
                }
                if relative
                    .rsplit('/')
                    .next()
                    .is_some_and(|n| n.starts_with("original."))
                {
                    return Err(fail("A local original receipt differs from its verified cloud copy. Preserve both files and repair this receipt before syncing."));
                }
                continue;
            }
            let bucket = a["bucket_id"]
                .as_str()
                .ok_or_else(|| fail("Missing receipt bucket."))?;
            let object = a["object_path"]
                .as_str()
                .ok_or_else(|| fail("Missing receipt object."))?;
            if !["folio-receipts", "folio-import"].contains(&bucket) {
                return Err(fail("Unexpected receipt bucket."));
            }
            let url = object_url(bucket, object)?;
            let r = response(
                client()?
                    .get(url)
                    .header("apikey", KEY)
                    .bearer_auth(token)
                    .send(),
            )?;
            let mut bytes = Vec::new();
            r.take(26_214_401).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != size || digest(&bytes) != hash {
                return Err(fail("Downloaded receipt failed integrity verification."));
            }
            let parent = target
                .parent()
                .ok_or_else(|| fail("Invalid receipt destination."))?;
            fs::create_dir_all(parent)?;
            let staging = self
                .paths
                .resolve(&format!("cache/{}.part", uuid::Uuid::new_v4()))?;
            fs::write(&staging, &bytes)?;
            fs::rename(staging, &target)?;
            self.mark_uploaded(relative, hash)?;
        }
        // The filesystem is scanned each cycle so images generated after metadata commit
        // are eventually uploaded even when no new database row is written.
        let receipts: Vec<String> = {
            let c = self.conn()?;
            let mut s = c.prepare("SELECT id FROM receipt_files")?;
            let rows = s.query_map([], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for id in receipts {
            uuid::Uuid::parse_str(&id).map_err(|_| fail("Invalid local receipt identity."))?;
            let dir = self.paths.resolve(&format!("receipts/{id}"))?;
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !safe_asset_name(&name) {
                    continue;
                }
                let relative = format!("receipts/{id}/{name}");
                let path = self.paths.resolve(&relative)?;
                let bytes = fs::read(path)?;
                if bytes.is_empty() || bytes.len() > 26_214_400 {
                    return Err(fail(
                        "A receipt file is empty or exceeds the 25 MB cloud limit.",
                    ));
                }
                let hash = digest(&bytes);
                let uploaded:bool=self.conn()?.query_row("SELECT EXISTS(SELECT 1 FROM cloud_uploaded WHERE relative_path=?1 AND sha256=?2)",params![relative,hash],|r|r.get(0))?;
                if uploaded {
                    continue;
                }
                let ext = name.rsplit('.').next().unwrap_or("bin");
                let object = format!("{OWNER}/{id}/{hash}.{ext}");
                let url = object_url("folio-receipts", &object)?;
                let r = client()?
                    .post(url)
                    .header("apikey", KEY)
                    .bearer_auth(token)
                    .header("Content-Type", mime(&name))
                    .body(bytes.clone())
                    .send()
                    .map_err(|_| fail("Receipt upload interrupted. It will retry."))?;
                if !r.status().is_success() {
                    // Duplicate immutable content is safe only after byte verification.
                    let mut saved = Vec::new();
                    response(
                        client()?
                            .get(object_url("folio-receipts", &object)?)
                            .header("apikey", KEY)
                            .bearer_auth(token)
                            .send(),
                    )?
                    .take(26_214_401)
                    .read_to_end(&mut saved)?;
                    if digest(&saved) != hash {
                        return Err(fail("Receipt upload failed verification."));
                    }
                }
                self.rpc(token,"folio_asset_register",json!({"receipt_id":id,"relative_path":relative,"object_path":object,"sha256":hash,"size_bytes":bytes.len()}))?;
                self.mark_uploaded(&relative, &hash)?;
            }
        }
        self.verify_local_originals()?;
        Ok(())
    }
    fn verify_local_originals(&self) -> Result<()> {
        let originals: Vec<(String, String, i64)> = {
            let c = self.conn()?;
            let mut q = c.prepare("SELECT relative_path,sha256,size_bytes FROM receipt_files")?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for (relative, expected, size) in originals {
            let path = self.paths.resolve(&relative)?;
            if !path.is_file() {
                return Err(fail("A receipt original is still awaiting transfer from another device. Keep that device open and retry sync."));
            }
            let bytes = fs::read(path)?;
            if bytes.len() as i64 != size || digest(&bytes) != expected {
                return Err(fail("A receipt original failed integrity verification. Preserve the file and restore a verified copy before syncing."));
            }
        }
        Ok(())
    }
    fn mark_uploaded(&self, path: &str, hash: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT OR IGNORE INTO cloud_uploaded VALUES(?1,?2)",
            params![path, hash],
        )?;
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn object_url(bucket: &str, path: &str) -> Result<reqwest::Url> {
    let mut u = reqwest::Url::parse(&format!("{URL}/storage/v1/object/{bucket}/"))
        .map_err(|_| fail("Invalid storage endpoint."))?;
    {
        let mut segments = u
            .path_segments_mut()
            .map_err(|_| fail("Invalid storage endpoint."))?;
        segments.pop_if_empty();
        for s in path.split('/') {
            if s.is_empty() || s == "." || s == ".." || s.contains('\\') {
                return Err(fail("Invalid cloud object path."));
            }
            segments.push(s);
        }
    }
    Ok(u)
}
fn safe_asset_name(name: &str) -> bool {
    name == "preview.webp"
        || name == "display.jpg"
        || name == "rendition.jpg"
        || name == "original.jpg"
        || name == "original.png"
        || name == "original.pdf"
        || name == "original.heic"
        || name == "original.jpeg"
        || name == "original.webp"
        || name == "original.heif"
        || name == "view.jpg"
}
fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("heic" | "heif") => "image/heic",
        _ => "application/octet-stream",
    }
}
fn same_row(local: Option<&Value>, remote: &Value) -> bool {
    if remote["deleted"] == true {
        local.is_none()
    } else {
        local.is_some_and(|l| same_value(l, &remote["data"]))
    }
}
/// Numbers compare by value: PostgreSQL sends float 1.0 as `1`, SQLite renders `1.0`.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) if x.is_f64() || y.is_f64() => {
            x.as_f64() == y.as_f64()
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_value(v, w)))
        }
        _ => a == b,
    }
}
fn record_conflict(
    c: &Connection,
    t: &str,
    key: &str,
    local: &Value,
    remote: &Value,
) -> Result<()> {
    c.execute("INSERT INTO cloud_conflicts(id,entity,record_key,local_data,remote_record) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(entity,record_key) DO UPDATE SET local_data=excluded.local_data,remote_record=excluded.remote_record,choice=NULL",params![uuid::Uuid::new_v4().to_string(),t,key,serde_json::to_string(local)?,serde_json::to_string(remote)?])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::secrets::SecretStore;
    use std::sync::Arc;
    struct Empty;
    impl SecretStore for Empty {
        fn get(&self) -> Result<Option<Zeroizing<String>>> {
            Ok(None)
        }
        fn set(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            Ok(())
        }
    }
    fn setup() -> (tempfile::TempDir, AppService) {
        let dir = tempfile::tempdir().unwrap();
        let s = AppService::open(dir.path().to_owned(), Arc::new(Empty)).unwrap();
        (dir, s)
    }
    #[test]
    fn mutations_capture_atomically_and_remote_does_not_echo() {
        let (_dir, s) = setup();
        let mut c = s.conn().unwrap();
        let tx = c.transaction().unwrap();
        tx.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','first','draft','MYR','now','now')",[]).unwrap();
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        tx.rollback().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        let remote = json!({"entity":"expense_claims","key":{"id":"test"},"revision":1,"deleted":false,"data":{"id":"test","claim_number":"t","title":"remote","description":"","status":"draft","currency":"MYR","created_at":"now","updated_at":"now","version":1,"sync_state":"local_only"}});
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.merge_remote(&tx, &remote).unwrap();
        tx.execute("UPDATE cloud_control SET applying=0", [])
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn conflict_preserves_current_local_and_remote() {
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','local','draft','MYR','now','now')",[]).unwrap();
        let remote = json!({"entity":"expense_claims","key":{"id":"test"},"revision":2,"deleted":false,"data":{"id":"test","title":"remote"}});
        s.merge_remote(&c, &remote).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT title FROM expense_claims WHERE id='test'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "local"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM cloud_conflicts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn fresh_device_discards_only_untouched_default_workspace() {
        let (_dir, s) = setup();
        let mut c = s.conn().unwrap();
        let id: String = c
            .query_row("SELECT id FROM workspaces", [], |r| r.get(0))
            .unwrap();
        let remote = json!({"entity":"workspaces","key":{"id":"remote"},"data":{"kind":"personal"},"deleted":false});
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.prune_default_workspace(&tx, &[remote]).unwrap();
        assert_eq!(
            tx.query_row("SELECT count(*) FROM workspaces WHERE id=?1", [id], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM categories", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        tx.rollback().unwrap();
        c.execute(
            "UPDATE workspaces SET name='My own workspace',version=2",
            [],
        )
        .unwrap();
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.prune_default_workspace(&tx,&[json!({"entity":"workspaces","key":{"id":"remote"},"data":{"kind":"personal"},"deleted":false})]).unwrap();
        assert_eq!(
            tx.query_row("SELECT count(*) FROM workspaces", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn legacy_expenses_keep_their_data_when_adopting_the_cloud_workspace() {
        let (_dir, s) = setup();
        let mut c = s.conn().unwrap();
        c.execute("INSERT INTO receipt_files(id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at) VALUES('legacy-receipt','legacy-hash','receipt.jpg','image/jpeg','receipts/legacy-receipt/original.jpg',1,'then','then')", []).unwrap();
        c.execute("INSERT INTO expenses(id,receipt_id,status,currency,created_at,updated_at) VALUES('legacy-expense','legacy-receipt','draft','MYR','then','then')", []).unwrap();
        let before = row(&c, "expenses", &json!({"id":"legacy-expense"})).unwrap();
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        let remote = json!({"entity":"workspaces","key":{"id":"remote"},"data":{"kind":"personal"},"deleted":false});
        s.prune_default_workspace(&tx, &[remote]).unwrap();
        assert_eq!(
            tx.query_row("SELECT count(*) FROM workspaces", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            row(&tx, "expenses", &json!({"id":"legacy-expense"})).unwrap(),
            before
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM receipt_files", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expenses'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn new_edit_survives_old_ack_and_composite_keys_match() {
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','first','draft','MYR','now','now')",[]).unwrap();
        let old: String = c
            .query_row(
                "SELECT change_id FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uuid::Uuid::parse_str(&old).unwrap().to_string(), old);
        c.execute(
            "UPDATE expense_claims SET title='newer' WHERE id='test'",
            [],
        )
        .unwrap();
        set_version(&c, "expense_claims", "{\"id\":\"test\"}", 5).unwrap();
        c.execute("DELETE FROM cloud_pending WHERE change_id=?1", [old])
            .unwrap();
        c.execute(
            "UPDATE cloud_pending SET base_revision=5 WHERE entity='expense_claims'",
            [],
        )
        .unwrap();
        let data: String = c
            .query_row(
                "SELECT data FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&data).unwrap()["title"],
            "newer"
        );
        c.execute("INSERT INTO exchange_rates VALUES('MYR','SGD','2026-11-05','2026-11-05','0.3','test','now')",[]).unwrap();
        let key: String = c
            .query_row(
                "SELECT record_key FROM cloud_pending WHERE entity='exchange_rates'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            key,
            canonical_key(
                &c,
                "exchange_rates",
                &json!({"requested_date":"2026-11-05","quote":"SGD","base":"MYR"})
            )
            .unwrap()
        );
    }
    #[test]
    fn staged_remote_resolution_waits_for_aggregate() {
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','local','draft','MYR','now','now')",[]).unwrap();
        let remote = json!({"entity":"expense_claims","key":{"id":"test"},"revision":2,"deleted":true,"data":{}});
        record_conflict(
            &c,
            "expense_claims",
            "{\"id\":\"test\"}",
            &json!({"title":"local"}),
            &remote,
        )
        .unwrap();
        let local = row(&c, "expense_claims", &json!({"id":"test"}))
            .unwrap()
            .unwrap();
        c.execute(
            "UPDATE cloud_conflicts SET choice='remote',local_data=?1",
            [serde_json::to_string(&local).unwrap()],
        )
        .unwrap();
        assert!(s.remote_conflict(&c, &remote).unwrap().is_none());
        assert!(row(&c, "expense_claims", &json!({"id":"test"}))
            .unwrap()
            .is_some());
        c.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.merge_remote(&c, &remote).unwrap();
        assert!(row(&c, "expense_claims", &json!({"id":"test"}))
            .unwrap()
            .is_none());
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn a_valid_access_token_is_reused_and_an_expired_one_is_not() {
        let (_dir, s) = setup();
        let mut cache = Some(Access {
            token: Zeroizing::new("cached".into()),
            until: Instant::now() + Duration::from_secs(60),
        });
        assert_eq!(s.cloud_access(&mut cache).unwrap().as_str(), "cached");
        cache.as_mut().unwrap().until = Instant::now();
        // This test database has no stored session, so an expired token cannot be used.
        assert!(s.cloud_access(&mut cache).is_err());
        let fresh: Session = serde_json::from_value(
            json!({"access_token":"a","refresh_token":"r","expires_in":3600,"user":{"id":OWNER}}),
        )
        .unwrap();
        assert!(fresh.access().until > Instant::now() + Duration::from_secs(3000));
        let unknown: Session = serde_json::from_value(
            json!({"access_token":"a","refresh_token":"r","user":{"id":OWNER}}),
        )
        .unwrap();
        assert!(unknown.access().until <= Instant::now());
    }
    #[test]
    fn an_opened_email_link_still_signs_in_from_the_browser_address() {
        let opened = "http://localhost:3000/#access_token=aaa&expires_at=1&expires_in=3600&refresh_token=r3fr35h&token_type=bearer&type=signup";
        assert_eq!(
            redirect_session(opened).unwrap().map(|t| t.to_string()),
            Some("r3fr35h".to_string())
        );
        let expired = "http://localhost:3000/#error=access_denied&error_code=otp_expired&error_description=Email+link+is+invalid+or+has+expired";
        assert!(redirect_session(expired)
            .unwrap_err()
            .message
            .contains("Email link is invalid or has expired"));
        let unopened = "https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/verify?token=abc&type=signup&redirect_to=http://localhost:3000";
        assert!(redirect_session(unopened).unwrap().is_none());
        assert!(redirect_session("123456").unwrap().is_none());
    }
    #[test]
    fn failed_requests_keep_the_cloud_reason() {
        let auth = br#"{"code":400,"error_code":"email_address_not_authorized","msg":"Email address not authorized"}"#;
        assert_eq!(
            server_reason(auth).as_deref(),
            Some("Email address not authorized")
        );
        let rpc = br#"{"code":"P0001","message":"Invalid financial graph"}"#;
        assert_eq!(
            server_reason(rpc).as_deref(),
            Some("Invalid financial graph")
        );
        assert_eq!(server_reason(b"<html>gateway</html>"), None);
    }
    #[test]
    fn each_local_database_has_its_own_cloud_session_slot() {
        // A test driver or copy with another data folder must never use this session.
        let (_a, first) = setup();
        let (_b, second) = setup();
        let account = first.vault_account().unwrap();
        assert_eq!(account, first.vault_account().unwrap());
        assert_ne!(account, second.vault_account().unwrap());
    }
    #[test]
    fn whole_number_decimal_from_cloud_is_not_a_conflict() {
        // PostgreSQL sends float 1.0 as `1`; SQLite renders the same local value as `1.0`.
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expenses(id,status,extraction_confidence,created_at,updated_at) VALUES('e','draft',1.0,'now','now')",[]).unwrap();
        let mut data = row(&c, "expenses", &json!({"id":"e"})).unwrap().unwrap();
        data["extraction_confidence"] = json!(1);
        let remote =
            json!({"entity":"expenses","key":{"id":"e"},"revision":1,"deleted":false,"data":data});
        assert!(s.remote_conflict(&c, &remote).unwrap().is_none());
        s.merge_remote(&c, &remote).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM cloud_conflicts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        data["extraction_confidence"] = json!(0.5);
        let changed =
            json!({"entity":"expenses","key":{"id":"e"},"revision":2,"deleted":false,"data":data});
        c.execute(
            "UPDATE expenses SET description='local edit' WHERE id='e'",
            [],
        )
        .unwrap();
        assert!(s.remote_conflict(&c, &changed).unwrap().is_some());
    }
    #[test]
    fn missing_or_corrupt_original_cannot_report_synced() {
        let (_dir, s) = setup();
        let id = uuid::Uuid::new_v4().to_string();
        let relative = format!("receipts/{id}/original.jpg");
        let bytes = b"test original";
        s.conn().unwrap().execute("INSERT INTO receipt_files(id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at) VALUES(?1,?2,'receipt.jpg','image/jpeg',?3,?4,'now','now')",params![id,digest(bytes),relative,bytes.len() as i64]).unwrap();
        assert!(s.verify_local_originals().is_err());
        let path = s.paths.resolve(&relative).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(s.verify_local_originals().is_ok());
        fs::write(&path, b"corrupt").unwrap();
        assert!(s.verify_local_originals().is_err());
    }
    #[test]
    fn tombstone_and_bad_paths() {
        assert!(object_url("folio-receipts", "a/../b").is_err());
        assert!(!safe_asset_name("../../password"));
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        let remote = json!({"entity":"expense_claims","key":{"id":"gone"},"revision":2,"deleted":true,"data":{}});
        s.merge_remote(&c, &remote).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT revision FROM cloud_versions WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
}
