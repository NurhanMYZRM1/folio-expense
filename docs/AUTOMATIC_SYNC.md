# Automatic Supabase synchronization

The engine lives in `src-tauri/src/services/cloud_sync.rs` (sync), `cloud_transport.rs` (project configuration, HTTPS client, failure classes) and `src-tauri/src/workspaces.rs` (accounts). Local schema 8 added the sync state; schema 9 binds each local database to one cloud account and keeps retry state.

## Connect

Open Settings → Cloud sync and sign in with Google, Apple, or an emailed link. Sign-in, provider setup and how accounts map to records are described in [Accounts and sign-in](ACCOUNTS.md). The app never receives a privileged API key: it sends the project's publishable key and the signed-in user's token. The session's refresh credential stays in a native vault entry keyed to a random ID stored in the local database, so fresh test databases use separate sessions. A complete database copy retains that ID and must be treated as the same installation; do not run cloud tests against a copy of a signed-in database.

For the emailed link: request it for an existing account, then enter the code or paste the full link without opening it first. If the link was already opened, paste the complete address the browser landed on. Supabase's built-in email service only delivers to members of the project's organization and sends at most 2 emails per hour; configure custom SMTP under Authentication → Emails to lift that.

Each local database belongs to one account. Signing out pauses transfer and preserves local work. Another account signing in on the same device gets its own separate records. Team sharing is not available.

## Sync states and recovery from failures

Settings shows one of: up to date, changes waiting, offline, sign in again, or needs attention, plus _Connection details_ (environment, project, whether the cloud is reachable, whether signed-in sync works, records and files waiting, conflicts, last success, next automatic retry, the last problem and its request ID). Nothing shown there is a credential or a record.

| What happened                               | What the app does                                                                                                             |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| No connection, timeout, or a server error   | Keeps the changes, shows _offline_, retries with growing delays (5 s doubling to 15 minutes, with jitter)                     |
| Rate limited                                | Waits as long as the server asks                                                                                              |
| Access token expired or refused             | Renews it once, inside the same sync, and tries again                                                                         |
| Session revoked or expired                  | Stops, shows _sign in again_, keeps every pending change                                                                      |
| Not allowed, or request rejected            | Shows the reason, keeps the changes, tries again in 15 minutes or when _Sync now_ is pressed                                  |
| The reply to an upload was lost             | The next sync recognises its own change and continues; an edit made in between stays queued and is not reported as a conflict |
| Signed out or changed account during a sync | The running sync stops at its next step and its late replies are discarded                                                    |

_Sync now_ always tries immediately. Automatic sync runs on launch, on return to the foreground, on reconnection, every 20 seconds while open, and about 2 seconds after an edit.

## Data and recovery

The engine covers 22 durable domain tables: receipt metadata, expenses, extraction results, claims/memberships, audit records, rate snapshots, workspaces, finance and business records, and receipt item reviews. AI credentials, device settings, local workers, job leases and generated export files are excluded. Receipt originals and allowed viewing derivatives transfer through private Storage with hash checks, bounded sizes and atomic local writes.

SQLite triggers capture changes transactionally. Supabase accepts versioned operations with durable replay receipts and derives the owner from the authenticated identity. A transaction-locked owner sequence feeds durable pull pages that preserve server transaction boundaries. Remote changes apply under one deferred-foreign-key SQLite transaction. Acknowledgements cannot remove newer local edits. The app retains tombstones and version history; automatic history compaction is not enabled.

Conflict review preserves the local candidate and the cloud version. Choosing the cloud version stages that choice for the complete pulled transaction, including dependencies; a newer local edit invalidates the choice. Choosing local queues a new versioned operation. A conflict can hold a pull page and pause unrelated changes until reviewed; it never silently selects the newest timestamp.

Errors remain visible in Settings and pending work survives restart. Do not remove the outbox, edit cursors or lower the database version to fix an error. Sign in again for expired access. Keep a separate backup; synchronized deletion is not backup retention. Local files for deleted remote receipts may remain as non-discoverable local bytes until normal storage recovery removes them.

Access tokens are reused until shortly before expiry. A temporary connection failure preserves the native refresh credential and pending work. A rejected access token clears the memory cache for renewal on the next cycle; a rejected refresh credential requires another sign-in. Signing out revokes only this device's session. Desktop sync also attempts one final transfer when the window becomes hidden, provided the browser reports connectivity; this is best effort and cannot guarantee completion before the process exits. iOS uses one native foreground loop, including when an embedded web screen is open.

## Limits

- The same receipt imported on two devices before either synced is matched automatically: the device that syncs second adopts the cloud's receipt and expense identity, keeps its file, and its own expense values are put to conflict review instead of being discarded. If the two copies were also added to different claims first, sync stops after that review with a "clashes with a different record" message and overwrites nothing; remove the expense from one of the claims to continue.
- An expense whose receipt is still being read is not uploaded until reading finishes, so another device never shows it as permanently processing.
- One receipt file that fails to transfer no longer blocks the others; the problem is reported and retried.
- A pending aggregate exceeding 2,000 records or 4 MiB pauses safely instead of splitting relational changes. Large backlogs require a reviewed migration/support operation.
- A mismatching local original receipt is an integrity error, not a successful sync. Preserve the local copy and investigate before restoring a verified original. Receipt derivatives can be regenerated locally.
- Sync runs while the app is open/foreground and resumes when it returns; it is not an always-running cloud OCR or export service. Mobile distribution requires a new native build.

## Deployment and verification

Project: [folio-expense](https://supabase.com/dashboard/project/jvzaufwijwdqqhmhroet), Singapore. The hosted schema is `supabase/automatic-sync.sql` on top of `supabase/imported-v7-schema.sql` (a data-free fixture). Accounts needed **no hosted schema change**: the RPCs already derive the owner from the signed-in user.

- `python3 supabase/tests/run-cloud-security.py` runs the SQL contract in a disposable PostgreSQL 17 container: import, replay, stale edits and deletes, aggregate atomicity, unsplit pages, duplicate receipts, row-level security, private files, and a second ordinary account that can neither read, overwrite nor claim the first account's records and files.
- `npm run test:rust` includes `src-tauri/tests/cloud.rs`: two devices and two accounts driven through the real commands against a local stand-in for the project (`tests/support/fake_cloud.rs`). These are mocked-cloud tests. They cover convergence, offline edits, lost replies, token renewal and revocation, rate limits, late replies, account isolation, first sign-in on a device with records, browser sign-in, duplicate receipts and interrupted downloads.
- Browser tests of the Settings controls stub authentication explicitly.

Live owner authentication, simulator receipt downloads and Mac/simulator convergence were verified on 6 November 2026 for the email-only release; see [the live verification record](SYNC_LIVE_VERIFICATION.md). That is historical evidence for the engine, not for Google or Apple sign-in, which has not been exercised against the hosted project.

Security Advisor's five `RLS enabled, no policy` informational findings apply to intentionally inaccessible internal sync tables. They have no client table grants and are reached only through authenticated, owner-scoped RPCs; adding policies would weaken that.
