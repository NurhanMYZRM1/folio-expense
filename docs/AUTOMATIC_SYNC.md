# Automatic Supabase synchronization

Implemented in the v7 finance worktree at `/Users/nurhanfawwaz/orca/folio-expense`, on branch `feat/finance-parity`. The older `supabase-integration` worktree cannot safely open the current database. Local database migration 008 advances the app to schema v8 and adds persistent sync state; a backup was made before migration.

## Connect

Open Settings → Cloud sync, request a sign-in email for the imported owner's address, and enter the code or paste the full sign-in link without opening it first. If the link was already opened, paste the complete address the browser landed on instead. Use **I already have a sign-in link** to reuse that email. The project uses Supabase's existing email authentication configuration. The app never receives a privileged API key, and the session refresh credential stays in a separate native vault entry. That entry is keyed to a random ID stored in the local database, so fresh test databases use separate sessions. A complete database copy retains that ID and must be treated as the same installation; do not run cloud tests against a copy of a signed-in database. No email is sent until a sign-in email is explicitly requested.

Supabase's built-in email service only delivers to members of the project's organization and sends at most 2 emails per hour per project. If the request reports that the address is not authorized or rate limited, add the address to the organization or configure custom SMTP under Authentication → Emails, then request again. Failed cloud requests show Supabase's reason after the Folio message.

This installation is bound to the imported owner account. Other account identities are rejected. Sign-in enables automatic uploads of existing and new supported data; sign-out pauses transfer and preserves local work. Account switching and team sharing are not available.

## Data and recovery

The engine covers 22 durable domain tables: receipt metadata, expenses, extraction results, claims/memberships, audit records, rate snapshots, workspaces, finance and business records, and receipt item reviews. AI credentials, device settings, local workers, job leases and generated export files are excluded. Receipt originals and allowed viewing derivatives transfer through private Storage with hash checks, bounded sizes and atomic local writes.

SQLite triggers capture changes transactionally. Supabase accepts versioned operations with durable replay receipts and derives the owner from the authenticated identity. A transaction-locked owner sequence feeds durable pull pages that preserve server transaction boundaries. Remote changes apply under one deferred-foreign-key SQLite transaction. Acknowledgements cannot remove newer local edits. The app retains tombstones and version history; automatic history compaction is not enabled.

Conflict review preserves the local candidate and the cloud version. Choosing the cloud version stages that choice for the complete pulled transaction, including dependencies; a newer local edit invalidates the choice. Choosing local queues a new versioned operation. A conflict can hold a pull page and pause unrelated changes until reviewed; it never silently selects the newest timestamp.

Errors remain visible in Settings and pending work survives restart. Do not remove the outbox, edit cursors or lower the database version to fix an error. Sign in again for expired access. Keep a separate backup; synchronized deletion is not backup retention. Local files for deleted remote receipts may remain as non-discoverable local bytes until normal storage recovery removes them.

Access tokens are reused until shortly before expiry. A temporary connection failure preserves the native refresh credential and pending work. A rejected access token clears the memory cache for renewal on the next cycle; a rejected refresh credential requires another sign-in. Signing out revokes only this device's session. Desktop sync also attempts one final transfer when the window becomes hidden, provided the browser reports connectivity; this is best effort and cannot guarantee completion before the process exits. iOS uses one native foreground loop, including when an embedded web screen is open.

## Limits

- New receipt imports with identical content but different IDs on independent devices are rejected with an actionable duplicate warning. Review the differing expense values before removing the redundant local import, then retry. Automatic identity remapping is not implemented.
- A pending aggregate exceeding 2,000 records or 4 MiB pauses safely instead of splitting relational changes. Large backlogs require a reviewed migration/support operation.
- A mismatching local original receipt is an integrity error, not a successful sync. Preserve the local copy and investigate before restoring a verified original. Receipt derivatives can be regenerated locally.
- Sync runs while the app is open/foreground and resumes when it returns; it is not an always-running cloud OCR or export service. Mobile distribution requires a new native build.

## Deployment and verification

Project: [folio-expense](https://supabase.com/dashboard/project/jvzaufwijwdqqhmhroet), Singapore. Full-domain schema changes are in `supabase/automatic-sync.sql`; `supabase/imported-v7-schema.sql` is a data-free fixture/base schema. The existing imported records and receipt files remain intact.

Verification includes a disposable PostgreSQL 17 suite for baseline import, replay, stale edits/deletes, relational aggregate atomicity, unsplit pages, duplicate receipts, RLS and private asset isolation. Hosted transactional smoke tests exercised create, replay, stale conflict, pull and cross-owner denial, then rolled back every synthetic change. Browser tests cover the cloud controls with explicitly mocked email/auth/cloud responses; they do not prove a live email or two physical device session.

Live owner authentication, iOS simulator receipt downloads and desktop/iOS record convergence were verified on 6 November 2026 (Asia/Kuala_Lumpur). See [the live verification record](SYNC_LIVE_VERIFICATION.md) for the exact checks and remaining limits. This does not establish physical iPhone behavior or eliminate every network and operating-system failure mode.

Security Advisor's five `RLS enabled, no policy` informational findings apply to intentionally inaccessible internal sync tables. They have no client table grants and are accessed only through authenticated, owner-scoped RPCs. The existing [leaked password protection warning](https://supabase.com/docs/guides/auth/password-security#password-strength-and-leaked-password-protection) remains; the user-facing onboarding uses email verification.
