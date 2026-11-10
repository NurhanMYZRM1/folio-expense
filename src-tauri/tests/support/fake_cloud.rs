//! A local stand-in for the Supabase project: Auth, the four sync RPCs and
//! private Storage, with the same contract as `supabase/automatic-sync.sql`
//! plus switches to lose replies, cut transfers short and refuse requests.
//! Test-only; the app reaches it over real HTTP through its normal transport.
#![allow(dead_code)]
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};

pub enum Fault {
    /// Answer with this status instead of doing the work.
    Status(u16, Value, Option<(&'static str, &'static str)>),
    /// Do the work, then close the connection without answering.
    DropReply,
    /// Do the work, then send only half of the answer.
    Truncate,
    /// Do the work, then wait for the signal before answering.
    Hold(mpsc::Receiver<()>),
}

struct Record {
    entity: String,
    key: Value,
    revision: i64,
    seq: i64,
    data: Value,
    deleted: bool,
}
impl Record {
    fn json(&self) -> Value {
        json!({"entity":self.entity,"key":self.key,"revision":self.revision,"seq":self.seq,"data":self.data,"deleted":self.deleted})
    }
}
#[derive(Default)]
struct Owner {
    seq: i64,
    records: BTreeMap<String, Record>,
    changes: Vec<(i64, String, Value)>,
    operations: HashMap<String, (String, Value)>,
}
#[derive(Default)]
struct State {
    /// account ID → (email, sign-in methods)
    users: HashMap<String, (String, Vec<String>)>,
    refresh: HashMap<String, String>,
    /// A just-rotated refresh token still returns its session, as Supabase allows for ten seconds.
    spent: HashMap<String, Value>,
    access: HashMap<String, String>,
    codes: HashMap<String, (String, String)>,
    /// (provider, the provider's own account) → Folio account
    identities: HashMap<(String, String), String>,
    owners: HashMap<String, Owner>,
    objects: HashMap<String, Vec<u8>>,
    assets: Vec<Value>,
    requests: Vec<String>,
}

pub struct FakeCloud {
    pub url: String,
    state: Mutex<State>,
    faults: Mutex<Vec<(String, Fault)>>,
    offline: AtomicBool,
}

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    bearer: Option<String>,
    body: Vec<u8>,
}
type Reply = (u16, Vec<u8>);
fn ok(value: Value) -> Reply {
    (200, value.to_string().into_bytes())
}
fn refuse(status: u16, message: &str) -> Reply {
    (status, json!({"message":message}).to_string().into_bytes())
}
fn key_of(entity: &str, key: &Value) -> String {
    let sorted: BTreeMap<_, _> = key
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    format!("{entity}|{}", serde_json::to_string(&sorted).unwrap())
}

impl FakeCloud {
    pub fn start() -> Arc<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cloud = Arc::new(Self {
            url: format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port()),
            state: Mutex::default(),
            faults: Mutex::default(),
            offline: AtomicBool::new(false),
        });
        let server = cloud.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let server = server.clone();
                thread::spawn(move || server.handle(stream));
            }
        });
        cloud
    }

    // ── Controls used by tests ──────────────────────────────────────────────
    /// Registers an account as its identity provider would, returning its ID.
    pub fn user(&self, email: &str, provider: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.state
            .lock()
            .unwrap()
            .users
            .insert(id.clone(), (email.into(), vec![provider.into()]));
        id
    }
    pub fn fault(&self, path: &str, fault: Fault) {
        self.faults.lock().unwrap().push((path.into(), fault));
    }
    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }
    /// Access tokens expire; refresh tokens keep working.
    pub fn expire_access(&self) {
        self.state.lock().unwrap().access.clear();
    }
    /// Ends every session of an account, as revoking it in the dashboard does.
    pub fn revoke(&self, user: &str) {
        let mut s = self.state.lock().unwrap();
        s.access.retain(|_, u| u != user);
        s.refresh.retain(|_, u| u != user);
        s.spent.clear();
    }
    pub fn requests(&self, path: &str) -> usize {
        let s = self.state.lock().unwrap();
        s.requests.iter().filter(|r| r.contains(path)).count()
    }
    /// Live records of one account for an entity, by record key.
    pub fn records(&self, user: &str, entity: &str) -> Vec<Value> {
        let s = self.state.lock().unwrap();
        s.owners
            .get(user)
            .map(|o| {
                o.records
                    .values()
                    .filter(|r| r.entity == entity && !r.deleted)
                    .map(Record::json)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn object_paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = self.state.lock().unwrap().objects.keys().cloned().collect();
        paths.sort();
        paths
    }
    /// What the system browser does with a sign-in address: the provider
    /// verifies `email`, Supabase links or creates the account, and the browser
    /// is sent back to the app's callback with a single-use code.
    pub fn browser_sign_in(&self, address: &str, email: &str) -> String {
        let url = reqwest::Url::parse(address).unwrap();
        assert!(url.path().ends_with("/auth/v1/authorize"), "{address}");
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let provider = q["provider"].clone();
        let mut s = self.state.lock().unwrap();
        let identity = (provider.clone(), email.to_string());
        let user = match (q.get("link_user"), s.identities.get(&identity)) {
            // Linking adds this provider identity to the account that asked for it.
            (Some(user), _) => user.clone(),
            // A known provider identity signs in to the account it is linked to.
            (None, Some(user)) => user.clone(),
            // Otherwise Supabase links by the provider's verified email, or creates an account.
            (None, None) => match s.users.iter().find(|(_, (e, _))| e == email) {
                Some((id, _)) => id.clone(),
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    s.users.insert(id.clone(), (email.into(), vec![]));
                    id
                }
            },
        };
        s.identities.insert(identity, user.clone());
        let methods = &mut s.users.get_mut(&user).unwrap().1;
        if !methods.contains(&provider) {
            methods.push(provider);
        }
        let code = uuid::Uuid::new_v4().to_string();
        s.codes
            .insert(code.clone(), (user, q["code_challenge"].clone()));
        format!("{}?code={code}", q["redirect_to"])
    }

    // ── HTTP ────────────────────────────────────────────────────────────────
    fn handle(&self, mut stream: TcpStream) {
        let Some(request) = read_request(&mut stream) else {
            return;
        };
        if self.offline.load(Ordering::SeqCst) {
            return; // Connection closes with no answer.
        }
        let fault = {
            let mut faults = self.faults.lock().unwrap();
            faults
                .iter()
                .position(|(path, _)| request.path.contains(path.as_str()))
                .map(|i| faults.remove(i).1)
        };
        let mut extra = String::new();
        let (status, body) = match &fault {
            Some(Fault::Status(status, body, header)) => {
                if let Some((name, value)) = header {
                    extra = format!("{name}: {value}\r\n");
                }
                (*status, body.to_string().into_bytes())
            }
            _ => self.respond(&request),
        };
        let head = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nsb-request-id: req-{status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        match fault {
            Some(Fault::DropReply) => {}
            Some(Fault::Truncate) => {
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body[..body.len() / 2]);
            }
            Some(Fault::Hold(release)) => {
                let _ = release.recv();
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
            _ => {
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        }
    }

    fn session(s: &mut State, user: &str) -> Value {
        let (access, refresh) = (
            format!("a-{}", uuid::Uuid::new_v4()),
            format!("r-{}", uuid::Uuid::new_v4()),
        );
        s.access.insert(access.clone(), user.into());
        s.refresh.insert(refresh.clone(), user.into());
        let (email, providers) = s.users[user].clone();
        json!({"access_token":access,"refresh_token":refresh,"expires_in":3600,"token_type":"bearer",
               "user":{"id":user,"email":email,"app_metadata":{"providers":providers}}})
    }

    fn respond(&self, r: &Request) -> Reply {
        let mut s = self.state.lock().unwrap();
        let body: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        let actor = r.bearer.as_ref().and_then(|t| s.access.get(t).cloned());
        s.requests.push(format!(
            "{} {} {}",
            r.method,
            r.path,
            r.query.get("grant_type").cloned().unwrap_or_default()
        ));
        match (r.method.as_str(), r.path.as_str()) {
            ("GET", "/auth/v1/health") => ok(json!({"name":"GoTrue"})),
            ("POST", "/auth/v1/token") => {
                match r.query.get("grant_type").map(String::as_str) {
                    Some("password") => {
                        let email = body["email"].as_str().unwrap_or_default();
                        let user = s
                            .users
                            .iter()
                            .find(|(_, (e, _))| e == email)
                            .map(|(id, _)| id.clone());
                        match user {
                        Some(user) if body["password"] == format!("pw-{email}") => {
                            ok(Self::session(&mut s, &user))
                        }
                        _ => (400, json!({"error_code":"invalid_credentials","msg":"Invalid login credentials"}).to_string().into_bytes()),
                    }
                    }
                    Some("refresh_token") => {
                        let token = body["refresh_token"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        if let Some(session) = s.spent.get(&token) {
                            return ok(session.clone());
                        }
                        match s.refresh.remove(&token) {
                        Some(user) => {
                            let session = Self::session(&mut s, &user);
                            s.spent.clear();
                            s.spent.insert(token, session.clone());
                            ok(session)
                        }
                        None => (400, json!({"code":400,"error_code":"refresh_token_not_found","msg":"Invalid Refresh Token: Refresh Token Not Found"}).to_string().into_bytes()),
                    }
                    }
                    Some("pkce") => {
                        let code = body["auth_code"].as_str().unwrap_or_default();
                        let verifier = body["code_verifier"].as_str().unwrap_or_default();
                        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(Sha256::digest(verifier.as_bytes()));
                        // A code is redeemed once, and only with the verifier it was issued for.
                        match s.codes.remove(code) {
                        Some((user, expected)) if expected == challenge => {
                            ok(Self::session(&mut s, &user))
                        }
                        _ => (400, json!({"error_code":"flow_state_not_found","msg":"invalid flow state"}).to_string().into_bytes()),
                    }
                    }
                    _ => refuse(400, "unsupported grant"),
                }
            }
            ("POST", "/auth/v1/otp") => ok(json!({})),
            ("POST", "/auth/v1/verify") => {
                let email = body["email"].as_str().unwrap_or_default();
                let user = s
                    .users
                    .iter()
                    .find(|(_, (e, _))| e == email)
                    .map(|(id, _)| id.clone());
                match user {
                    Some(user) if body["token"] == "123456" => ok(Self::session(&mut s, &user)),
                    _ => (
                        403,
                        json!({"error_code":"otp_expired","msg":"Token has expired or is invalid"})
                            .to_string()
                            .into_bytes(),
                    ),
                }
            }
            ("POST", "/auth/v1/logout") => {
                if let Some(token) = &r.bearer {
                    s.access.remove(token);
                }
                (204, Vec::new())
            }
            ("GET", "/auth/v1/user/identities/authorize") => match actor {
                Some(user) => ok(json!({"url":format!(
                    "{}/auth/v1/authorize?provider={}&link_user={user}&redirect_to={}&code_challenge={}",
                    self.url, r.query["provider"], r.query["redirect_to"], r.query["code_challenge"]
                )})),
                None => refuse(401, "JWT expired"),
            },
            _ if r.path.starts_with("/rest/v1/rpc/") => {
                let Some(user) = actor else {
                    return refuse(401, "JWT expired");
                };
                match r.path.rsplit('/').next().unwrap() {
                    "folio_sync_push" => push(&mut s, &user, &body["changes"]),
                    "folio_sync_pull" => pull(
                        s.owners.entry(user).or_default(),
                        body["since"].as_i64().unwrap_or(-1),
                        body["page_limit"].as_i64().unwrap_or(0),
                    ),
                    "folio_assets" => {
                        let live = |receipt: &Value| {
                            s.owners.get(&user).is_some_and(|o| {
                                o.records
                                    .get(&key_of("receipt_files", &json!({"id":receipt})))
                                    .is_some_and(|r| !r.deleted)
                            })
                        };
                        ok(Value::Array(
                            s.assets
                                .iter()
                                .filter(|a| a["owner"] == user.as_str() && live(&a["receipt_id"]))
                                .cloned()
                                .collect(),
                        ))
                    }
                    "folio_asset_register" => {
                        let (receipt, sha) = (
                            body["receipt_id"].as_str().unwrap_or_default(),
                            body["sha256"].as_str().unwrap_or_default(),
                        );
                        let object = body["object_path"].as_str().unwrap_or_default();
                        if !object.starts_with(&format!("{user}/{receipt}/{sha}.")) {
                            return refuse(400, "Invalid receipt asset");
                        }
                        let known = s.owners.get(&user).is_some_and(|o| {
                            o.records
                                .get(&key_of("receipt_files", &json!({"id":receipt})))
                                .is_some_and(|r| !r.deleted)
                        });
                        if !known {
                            return refuse(400, "Receipt unavailable");
                        }
                        let stored = s
                            .objects
                            .get(&format!("folio-receipts/{object}"))
                            .map(Vec::len);
                        if stored.map(|n| n as i64) != body["size_bytes"].as_i64() {
                            return refuse(400, "Object not uploaded or size mismatch");
                        }
                        let mut asset = body.clone();
                        asset["owner"] = json!(user);
                        asset["bucket_id"] = json!("folio-receipts");
                        s.assets.retain(|a| {
                            !(a["owner"] == user.as_str()
                                && a["receipt_id"] == body["receipt_id"]
                                && a["relative_path"] == body["relative_path"])
                        });
                        s.assets.push(asset.clone());
                        ok(asset)
                    }
                    _ => refuse(404, "unknown function"),
                }
            }
            _ if r.path.starts_with("/storage/v1/object/") => {
                let object = r.path.trim_start_matches("/storage/v1/object/").to_string();
                let folder = object.split('/').nth(1).unwrap_or_default();
                // Storage policy: an account reads and writes only its own folder.
                match actor {
                    None => refuse(401, "JWT expired"),
                    Some(user) if user != folder => (400, json!({"statusCode":"403","error":"Unauthorized","message":"new row violates row-level security policy"}).to_string().into_bytes()),
                    Some(_) if r.method == "POST" => {
                        if s.objects.contains_key(&object) {
                            return (400, json!({"statusCode":"409","error":"Duplicate","message":"The resource already exists"}).to_string().into_bytes());
                        }
                        s.objects.insert(object, r.body.clone());
                        ok(json!({"Key":"stored"}))
                    }
                    Some(_) => match s.objects.get(&object) {
                        Some(bytes) => (200, bytes.clone()),
                        None => refuse(400, "Object not found"),
                    },
                }
            }
            _ => refuse(404, "not found"),
        }
    }
}

/// `folio_sync.push`: replay by change ID, all-or-nothing aggregates, optimistic revisions.
fn push(s: &mut State, user: &str, changes: &Value) -> Reply {
    let owner = s.owners.entry(user.into()).or_default();
    let changes = changes.as_array().cloned().unwrap_or_default();
    let fingerprint = |ch: &Value| ch.to_string();
    let duplicate_of = |owner: &Owner, ch: &Value| -> Option<Value> {
        if ch["entity"] != "receipt_files" || ch["operation"] != "upsert" {
            return None;
        }
        owner
            .records
            .values()
            .find(|r| {
                r.entity == "receipt_files"
                    && !r.deleted
                    && r.data["sha256"] == ch["data"]["sha256"]
                    && r.key["id"] != ch["key"]["id"]
            })
            .map(|r| r.key["id"].clone())
    };
    let stale = |owner: &Owner, ch: &Value| {
        let remote = owner
            .records
            .get(&key_of(ch["entity"].as_str().unwrap(), &ch["key"]));
        remote.map_or(0, |r| r.revision) != ch["baseRevision"].as_i64().unwrap_or(-1)
            || remote.is_some_and(|r| r.deleted)
    };
    let mut conflict = false;
    for ch in &changes {
        let id = ch["changeId"].as_str().unwrap_or_default();
        if let Some((saved, _)) = owner.operations.get(id) {
            if *saved != fingerprint(ch) {
                return refuse(400, "Operation ID reused");
            }
            continue;
        }
        conflict |= duplicate_of(owner, ch).is_some() || stale(owner, ch);
    }
    if conflict {
        let results: Vec<Value> = changes
            .iter()
            .map(|ch| {
                let id = ch["changeId"].as_str().unwrap_or_default();
                if let Some((_, result)) = owner.operations.get(id) {
                    return result.clone();
                }
                let existing = duplicate_of(owner, ch);
                let remote = owner.records.get(&key_of(ch["entity"].as_str().unwrap(), &ch["key"]));
                json!({"changeId":id,
                    "status": if existing.is_some() {"rejected"} else if stale(owner, ch) {"conflict"} else {"blocked"},
                    "reason": existing.as_ref().map(|_| "duplicate_receipt"),
                    "existingReceiptId": existing,
                    "revision": remote.map_or(0, |r| r.revision),
                    "record": remote.map(Record::json)})
            })
            .collect();
        return ok(Value::Array(results));
    }
    // One expense per receipt for each account, as the hosted unique constraint enforces.
    for ch in changes
        .iter()
        .filter(|c| c["entity"] == "expenses" && c["operation"] == "upsert")
    {
        let receipt = &ch["data"]["receipt_id"];
        let taken = !receipt.is_null()
            && owner.records.values().any(|r| {
                r.entity == "expenses"
                    && !r.deleted
                    && r.data["receipt_id"] == *receipt
                    && r.key["id"] != ch["key"]["id"]
            });
        if taken {
            return (
                409,
                json!({"code":"23505","message":"duplicate key value violates unique constraint"})
                    .to_string()
                    .into_bytes(),
            );
        }
    }
    let batch = uuid::Uuid::new_v4().to_string();
    let mut results = Vec::new();
    for ch in &changes {
        let id = ch["changeId"].as_str().unwrap_or_default().to_string();
        if let Some((_, result)) = owner.operations.get(&id) {
            results.push(result.clone());
            continue;
        }
        let entity = ch["entity"].as_str().unwrap().to_string();
        let key = key_of(&entity, &ch["key"]);
        let deleted = ch["operation"] == "delete";
        owner.seq += 1;
        let revision = owner.records.get(&key).map_or(0, |r| r.revision) + 1;
        let record = Record {
            entity,
            key: ch["key"].clone(),
            revision,
            seq: owner.seq,
            data: if deleted {
                Value::Null
            } else {
                ch["data"].clone()
            },
            deleted,
        };
        let result =
            json!({"changeId":id,"status":"applied","revision":revision,"record":record.json()});
        owner
            .changes
            .push((owner.seq, batch.clone(), record.json()));
        owner.records.insert(key, record);
        owner
            .operations
            .insert(id, (fingerprint(ch), result.clone()));
        results.push(result);
    }
    ok(Value::Array(results))
}

/// `folio_sync.pull`: ordered pages that never split one pushed transaction.
fn pull(owner: &mut Owner, since: i64, limit: i64) -> Reply {
    if since < 0 || !(1..=500).contains(&limit) {
        return refuse(400, "Invalid cursor");
    }
    let after: Vec<_> = owner.changes.iter().filter(|c| c.0 > since).collect();
    let cutoff = after
        .get((limit as usize).min(after.len()).saturating_sub(1))
        .map(|last| {
            after
                .iter()
                .filter(|c| c.1 == last.1)
                .map(|c| c.0)
                .max()
                .unwrap()
        })
        .unwrap_or(since);
    let records: Vec<Value> = after
        .iter()
        .filter(|c| c.0 <= cutoff)
        .map(|c| c.2.clone())
        .collect();
    let more = owner.changes.iter().any(|c| c.0 > cutoff);
    ok(json!({"records":records,"cursor":cutoff,"hasMore":more}))
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let split = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(i) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..split]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_string(), first.next()?.to_string());
    let mut length = 0;
    let mut bearer = None;
    for line in lines {
        let (name, value) = line.split_once(':')?;
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.trim().parse().ok()?,
            "authorization" => bearer = value.trim().strip_prefix("Bearer ").map(str::to_owned),
            _ => {}
        }
    }
    let mut body = buffer[split + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let url = reqwest::Url::parse(&format!("http://x{target}")).ok()?;
    Some(Request {
        method,
        path: url.path().to_string(),
        query: url.query_pairs().into_owned().collect(),
        bearer,
        body,
    })
}
