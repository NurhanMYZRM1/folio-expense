# Finance expansion: architecture decisions

These decisions extend Folio from corporate expense claims into personal and
small-business finance (see [belanje-parity.md](belanje-parity.md) for scope).
They are binding for every stream of work; change them here first.

Status legend: **Decided** = implemented against; **Designed** = agreed shape,
not built yet.

## D1. Workspaces (Decided)

- `workspaces.kind` is `personal` or `business`. Each local database has
  exactly one personal workspace, created at startup when missing, plus zero
  or more business workspaces created by the user.
- Every ledger and business record carries `workspace_id`; every query is
  scoped to one workspace. Cross-workspace references are rejected by the
  service layer (and, once synced, by server row-level security).
- Existing receipts, expenses and claims stay unscoped. They are the corporate
  claim inbox of the device's user. A receipt becomes a ledger record only by
  being explicitly recorded into an account of some workspace (D2).
- Only business workspaces can ever be shared with a team (D6). Personal
  workspaces have a single owner, enforced on the server.

## D2. Ledger (Decided)

- One `transactions` table, kinds `income`, `expense`, `transfer`. Amounts are
  positive integers in the account's currency; the kind gives the direction.
- Account balance = opening balance + income − expense − transfers out +
  transfers in (the received `to_amount_minor`). Balances are always computed
  in SQL, never cached, so edits and deletions cannot drift them.
- Transfers are never income or spending: budgets and reports exclude them.
- **Savings goals are savings accounts with a target.** Progress is the
  account balance; a contribution is a transfer into it, a withdrawal a
  transfer out. Balances reconcile by construction.
- **Loans are loan accounts.** `borrowed` loans carry a negative balance (you
  owe), `lent` loans a positive one (owed to you). Repayments are transfers
  (bank → borrowed loan, or lent loan → bank); interest and fees are expenses.
  Outstanding = |balance|; progress = (principal − outstanding) / principal.
  Folio makes no interest projection; the annual rate is informational.
- Loan accounts are excluded from "Total balance" and shown under Loans.
  Archiving an account requires a zero balance, so totals never hide money.
- **Receipts → ledger:** `transactions.expense_id` is unique, so a receipt is
  recorded at most once. The account's currency must equal the receipt's
  claimed currency (no hidden conversion). The receipt stays the source of
  truth for amount, date and payee: receipt edits update the linked
  transaction in the same database transaction, and the transaction editor
  refuses to change those three fields. Deleting the receipt deletes its
  transaction (the UI says so); deleting the transaction keeps the receipt.
- **Bills → ledger:** a bill payment _is_ an expense transaction with
  `(bill_id, bill_due_on)`, unique. Paying an occurrence twice returns the
  existing transaction; deleting that transaction makes the occurrence unpaid.
- **Invoices → ledger (Designed, D10):** an invoice is not income; each
  recorded payment against it is one income transaction linked to that payment.
- **Claims:** unchanged. A reimbursement from an employer is recorded by the
  user as income (category "Reimbursement") if they track it; Folio does not
  infer it, so a claim never double-counts.

## D3. Money (Decided)

- Integer minor units (`i64`) everywhere, scaled by the currency exponent
  (`currency_exponent`: 0, 2 or 3 decimals). No floating point in any
  authoritative amount, total, budget or tax calculation.
- Conversion uses `convert_minor` (i128, half-up to the target's minor unit)
  on the exact published decimal rate. Each transaction stores
  `base_amount_minor` in the workspace base currency, converted at the cached
  ECB rate for its date, together with that rate and rate date. It is NULL
  while no rate is available (offline); reports exclude and count such rows
  ("n transactions awaiting an exchange rate") and the existing
  `convert_pending` sweep fills them in.
- "Total balance" sums only accounts in the base currency; other-currency
  balances are listed beside it, unconverted.
- Allocation (bill split, tax apportioning) uses the largest-remainder method
  on minor units, so shares always add up to the total.
- Values crossing into JavaScript must stay within ±2^53; Rust rejects larger
  aggregates instead of losing precision.

## D4. Dates and time (Decided)

- Financial dates are local calendar dates (`YYYY-MM-DD`), not instants.
  Budget periods are calendar months (`YYYY-MM`) of those dates.
- "Today" is the device's local date. Commands accept an optional `today`
  argument (tests and the UI pass it); otherwise Rust uses `chrono::Local`.
- Recurrence is computed on naive dates. Monthly and yearly schedules keep the
  anchor's day and clamp to the month's last day (31 Jan → 28/29 Feb → 31 Mar;
  29 Feb → 28 Feb in common years).
- Editing a bill's schedule re-anchors it at the chosen next due date; earlier
  unpaid occurrences of the old schedule are dropped, paid ones stay in history.
- A reminder fires at 09:00 local time on (due date − reminder days). Clients
  reschedule all OS notifications from `list_reminders` on start and after
  any change, so a timezone change moves reminders to the new local 09:00.
- Record timestamps (`created_at`, `updated_at`) stay UTC RFC 3339.

## D5. Shared API contract (Decided)

- New core operations are JSON commands in `src-tauri/src/dispatch.rs` only.
  Desktop exposes them through Tauri commands generated by one macro
  (`commands/core.rs`) that forwards the raw request body to `dispatch`; the
  mobile FFI and the test driver already call `dispatch`. No per-command
  wrappers.
- Read-only commands are named `list_*`, `get_*`, `read_*`, `report_*` or
  `parse_*`. The mobile app relies on this to avoid refetch loops.
- Types come from Rust via ts-rs (`npm run bindings`); the typed client is
  `src/lib/ipc.ts`.

## D6. Accounts, tenancy and permissions (Designed, phase 5)

- Optional sign-in: email (magic link or password), Sign in with Apple,
  Sign in with Google. Folio keeps working offline without an account.
- Connecting existing local data to an account is an explicit, previewed
  action. Nothing uploads silently, including during migrations.
- Roles (a Folio decision): **owner** (everything, including deleting the
  workspace and managing admins), **admin** (manage members and data),
  **editor** (create, edit, delete business records), **viewer** (read only).
- Server row-level security enforces membership and role on every table and
  storage object; the UI only mirrors it.
- Invitations are connect codes of 12 Crockford base32 characters (60 bits),
  stored as SHA-256 hashes, expiring after 7 days, revocable, single use, and
  redeemed through a rate-limited function. Removing a member revokes access
  on their next request; their device keeps nothing it can sync back.

## D7. Sync (Designed, phase 5)

Shape: **the server stores versioned records; the Rust core stays the rule
engine.**

- Server table `records(workspace_id, entity, id, version, data, deleted_at,
server_seq, updated_by, updated_at)`, keyed by (entity, id). Each syncable
  local row is one record and `data` is that row as JSON. Business rules and
  relational checks run in the Rust core on every device (one implementation
  for desktop and mobile). The server enforces who may read and write each
  workspace, versions, idempotency and tombstones, so there is no per-table
  server schema to keep in step with local migrations.
- Writes go only through `sync_push(changes)`, a security-definer function
  with explicit membership and role checks; there are no direct insert or
  update policies. A change carries a `change_id` UUID, entity, id, workspace,
  op (upsert/delete), base version and data. Results: applied (new version
  and sequence), conflict (server copy returned) or rejected (tombstoned or
  forbidden). Applied change ids are kept, so a retried push returns the same
  results and never applies twice.
- Conflicts are version-based, never silent last-writer-wins. The server copy
  stays canonical; the client keeps its losing edit in `sync_conflicts` and
  offers "keep mine" (re-push on top of the server version) or "keep theirs".
  A delete wins over a concurrent edit and leaves a permanent tombstone; an
  upsert of a tombstoned id is rejected, so stale clients cannot resurrect it.
- Pull: `sync_pull(workspace, since, limit)` returns records and tombstones in
  sequence order. The client keeps a cursor per workspace and applies each
  page in one SQLite transaction with deferred foreign keys. Rows with a
  pending local change are not overwritten (their push conflicts instead).
  Incoming data passes the same Rust validation; invalid records are
  quarantined, not applied.
- Change capture: with sync enabled, SQLite triggers on each syncable table
  upsert one `sync_outbox` row per (entity, id), keeping the oldest base
  version, so several edits before a push become one change. Rows written by
  pull are marked `synced` and skipped by the triggers. Service code is
  unchanged, and a mutation never waits on the network.
- Never synced (device-local): jobs, extraction runs, budget alerts,
  transaction suggestions, the exchange-rate cache, audit events,
  device-specific settings (theme, export folder, AI endpoint) and
  credentials.
- Receipts, expenses and claims sync into the user's personal workspace.
  Receipt files upload to the private bucket at `<workspace>/<receipt id>/<file
name>` before their row is pushed; other devices download on first view.
  Storage policies use the same membership check as records.
- Per-row state shown to users: local only (sync off), pending (in the
  outbox), synced, conflict, failed (last error; retried with backoff from
  1 s to 5 min).
- Connecting a device previews counts per record type and uploads only after
  confirmation. Signing in on a second device that already has local data
  asks whether to merge it into the account or keep it local-only.

## D8. Backend (Designed, phase 5)

Supabase: Postgres (transactional pushes, row-level security), Auth
(email one-time codes, Apple, Google), private Storage, and Edge Functions
for work that needs the service role (account deletion, store webhooks).
Local development uses the Docker-based stack (`supabase start`). Chosen for
server-enforced authorization without a bespoke auth server and an
open-source, self-hostable stack. Desktop sign-in uses an emailed one-time
code (no redirect needed) or OAuth with PKCE through the system browser and
the `expenseapp://` deep link; phones use native Apple/Google sign-in and
exchange the ID token. Creating the hosted project, provider credentials and
keys is an owner action.

## D9. Encryption (Designed, phase 5)

- In transit: TLS only (rustls), no plaintext fallback.
- Local at rest today: OS user protections only (documented in README).
  Phase 5 target: the SQLite database encrypted with SQLCipher under a random
  256-bit key held in the OS keychain/keystore; iOS files use complete file
  protection. Losing the key loses local data unless it was synced; this is
  stated in the UI before enabling.
- Cloud at rest: provider-managed encryption plus RLS. This is **not**
  end-to-end encryption and must never be described as such.
- Diagnostics never contain amounts, merchant or receipt text, or secrets.

## D10. Business and tax (Designed, phase 4)

- Business records (customers, invoices, inventory) live in a business
  workspace. Invoice totals, numbering and SST are computed in Rust.
- Issuing an invoice changes nothing in the ledger; each payment creates one
  income transaction. Stock moves when an invoice is issued (not when paid);
  voiding an issued invoice returns its stock.
- Tax rules are data keyed by year of assessment with a source URL for each
  figure. Unsupported years or inputs are refused with an explanation, never
  guessed. Results are labelled estimates; nothing is filed or submitted.
