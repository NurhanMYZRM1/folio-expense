# Finance expansion progress

Resume here. Read [DECISIONS.md](DECISIONS.md) (binding architecture) and
[belanje-parity.md](belanje-parity.md) (scope and per-feature status) first.
Branch: `feat/finance-parity` (local only, not pushed).

## Latest UI follow-up (2026-11-05)

- Removed the dashboard's local-first banner and the footer's local-first text.
- Fixed opening-balance typing moving focus back to the account name: the
  shared modal now initializes its focus trap only when opened, while keeping
  its current close callback. Account/workspace dialogs use the modal's focus
  marker rather than competing React autofocus.
- Save preferences is now in a sticky toolbar above the Settings form, with
  unsaved/saved status. Saving keeps the user's current scroll position.
- A delayed-load regression also exposed stale startup refreshes clearing the
  loading state too early. Only the current request may finish loading or set
  its error, so Settings initializes from saved values rather than defaults.
- Regression checks cover paced typing for new/edited signed opening balances,
  persistence, Escape/focus restoration, delayed Settings loads, and saving
  from top/middle/bottom at desktop and phone widths.
- Checks: `npm run check` passed (40 tests; 0 lint errors, 7 existing warnings),
  `npm run format:check` passed, all 14 browser journeys passed, and mobile
  typecheck/lint passed (0 errors, 2 existing warnings). No Rust changes.

## Session history

| Session        | Agent                          | Outcome                                                                                                                                                                                                                        |
| -------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1              | Claude Code (lead + 4)         | Phase 1: matrix, decisions D1–D10, ledger contract (`848cd63`, `c5d5a14`). Subagents stopped at the usage limit before writing code; their worktrees under `.claude/worktrees/` hold only superseded drafts.                   |
| 2              | Codex (lead + 3)               | Phases 2–4 and parts of 3, 5, 6 written in the main tree, uncommitted, then stopped at the usage limit mid simulator checks.                                                                                                   |
| 3 (2026-11-05) | Claude Code (lead + subagents) | Verified and committed session 2 (`b9735bd`); stopped at the usage limit with panel styles, a v3 fixture, category rules and customer/item deletion uncommitted. CS/MP subagents also stopped; their worktrees are historical. |
| 4 (2026-11-05) | Codex                          | Finished local deletion/category/upgrade work; added full data export and restore tests; reproduced and fixed server invitation authorization; all listed checks passed.                                                       |

Sessions 1–3 used a lead plus subagents in separate worktrees because Claude
Code Agent Teams was unavailable. Session 4 used one agent; it did not resume
prior agents. Neither arrangement is described as an Agent Team.

## Current state (2026-11-05, session 4)

Schema v7: `004` ledger, `005` business, `006` sync metadata, `007` receipt
line items. Commands are JSON operations in `dispatch.rs` and
`business_dispatch.rs` (D5).

Session 4 (Codex) finished and verified the main-tree work left by session 3:

- Fixed the interrupted deletion test's invalid request ID; customer/item
  deletion now checks references and deletes in one transaction. Unused rows
  delete through the desktop/shared UI; invoices and stock history prevent
  deletion with an archive instruction. Cancellation and errors are covered
  in the browser journey.
- Category duplicates (including case/spacing), rename conflicts, used-category
  deletion and archiving are covered in Rust. Duplicates show a useful error.
- The actual pre-finance v3 fixture from `main` (`126d459`) upgrades without
  changing its receipt/claim IDs, values, settings, exports or 12 audit events.
  No transaction or upload is created; reopening is idempotent. `.gitignore`
  explicitly includes this synthetic fixture's database.
- Tax and receipt-item panels now have body spacing and checkbox styles.
- Settings → **Export all my data** creates a ZIP containing every table in
  `records.json`, a consistent SQLite snapshot (including committed WAL
  changes), original receipts and existing reports. The app holds its service
  lock while taking the snapshot and copying attachments. Tests restore the
  archive into a fresh data folder and verify records, attachment bytes,
  integrity and foreign keys. Missing originals and symlinks fail; temporary
  files are cleaned on ordinary failure. No logs/cache/vault credentials or
  previous full archives are included. The ZIP is explicitly unencrypted;
  restore is manual with the app closed. Native phone sharing is wired but
  has not run on a device. `zip` moved from dev-only to normal dependencies;
  no new package or lockfile resolution was introduced.
- Ran the existing SQL security suite on disposable Postgres 17. A new outsider
  invitation test reproduced a real authorization bypass: a missing role made
  `NOT IN` evaluate to NULL, so the PL/pgSQL denial branch was skipped. The
  additive `20261105140000_invitation_authorization.sql` migration refuses
  missing membership and null roles. Expanded tests cover invitation
  revocation, wrong codes, 10/hour rate limiting and reset, personal-workspace
  isolation, and removed-member pull/write/invite denial. All pass.
  `python3 supabase/tests/run-security.py` reproduces this without publishing
  ports or contacting real accounts. This is SQL/RLS verification with fixture
  auth/storage schemas, **not** Supabase Auth/API integration verification.

The prior provider session and its worktrees were not modified or resumed.
No subagents were started in session 4.

Previously implemented and tested in Rust, with desktop UI:

- Personal finance: accounts, savings goals and loans as accounts (D2),
  transactions and transfers, categories, budgets with one-time alerts, bills
  with month-end recurrence and idempotent payments, reminders, monthly
  reports, transaction CSV export.
- Receipts: line-item extraction and review with reconciliation, recording a
  reviewed receipt into the ledger, quick-add text parsing to a review form.
- Business: profile, customers, inventory and stock movements, invoices
  (numbering, SST, issue/void, payments as one income entry), invoice PDF.
- Tax: YA2025 resident estimate, SST arithmetic, e-invoice field validation
  (prepared only, never submitted). Sources: [tax-sources.md](tax-sources.md).
- Android: JNI bridge (`src-tauri/src/mobile_jni.rs`), Kotlin module with
  ML Kit OCR, PDF rendering, speech recognition and an opt-in payment
  notification listener (Maybank, Touch 'n Go eWallet allowlist).
- Mobile: finance DOM pages, local reminder scheduling (`expo-notifications`),
  iOS speech (`FolioSpeech.swift`).
- Server schema only: `supabase/migrations/…_finance_sync.sql` (RLS,
  idempotent `sync_push`/`sync_pull`, hashed expiring invitations, member
  removal, receipt storage policies) and `supabase/tests/sync-security.sql`.

Not started or incomplete:

- `src-tauri/src/services/sync/` is empty: no Rust sync client, no change
  capture triggers (`006_sync.sql` refers to `sync::initialize_sync`, which
  does not exist yet), no sign-in, no account or team UI.
- SQLCipher encryption, account deletion, bill split and entitlements.
- Full local Supabase Auth/Storage/API integration has not been exercised.
  The current server protocol accepts ledger entities only; business and
  receipt/claim sync still need server support.
- iOS simulator build failed at link time in session 2 (`folio_invoke`
  undefined) while the Rust xcframework was being rebuilt in parallel; the
  rebuilt xcframework contains the symbols. Rebuild to confirm.
- Android: APK builds (arm64); not yet run on the emulator.

## Checks in session 4

| Check                                                                         | Result                                                                   |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `npm run lint`                                                                | 0 errors, 7 existing warnings                                            |
| `npm run build`; `npm test`                                                   | passed; 40 tests                                                         |
| `npm run format:check`                                                        | passed                                                                   |
| `npm run test:rust`                                                           | 200 tests passed (117 unit + 83 integration)                             |
| `npm run lint:rust`                                                           | passed                                                                   |
| clippy `--no-default-features --features mobile --all-targets -- -D warnings` | passed                                                                   |
| `npm run bindings`; `npm run test:driver`                                     | passed                                                                   |
| `npm run test:e2e`                                                            | 11 passed, including deletion and Settings data export                   |
| Mobile `npx tsc --noEmit`; `npx expo lint`                                    | passed; 0 errors, 2 existing warnings                                    |
| `python3 supabase/tests/run-security.py`                                      | passed; disposable Postgres 17, all migrations + expanded security suite |

No native Android/iOS builds or runtime journeys were repeated in session 4.
Journey 8 has its local data-export half covered; account deletion is still
missing. Journeys 6, 7 and 9 remain incomplete.

## Baseline checks at `b9735bd`

| Check                                                                 | Result                                                |
| --------------------------------------------------------------------- | ----------------------------------------------------- |
| `npm run lint`                                                        | 0 errors, 7 warnings (same as baseline)               |
| `npm run build`                                                       | passed                                                |
| `npm test`                                                            | 40 passed                                             |
| `npm run format:check`                                                | passed (after formatting 3 TS and several Rust files) |
| `npm run test:rust`                                                   | 194 passed (117 unit, 77 integration)                 |
| `npm run lint:rust`; clippy `--no-default-features --features mobile` | passed                                                |
| `npm run test:driver`                                                 | passed                                                |
| `npm run test:e2e`                                                    | 10 passed (8 baseline + 2 finance journeys)           |
| Mobile `npx tsc --noEmit`; `npx expo lint`                            | passed; 0 errors, 2 warnings                          |
| Android `:app:assembleDebug` (arm64-v8a)                              | passed (session 2 log)                                |
| iOS simulator `xcodebuild`                                            | failed at link (see above), to rerun                  |

Desktop UI verification: receipt → OCR → three line items reconciled to the
total → record panel; quick add "Lunch 12.50 yesterday" → review with
nothing saved; every finance screen rendered with persisted data and no page
errors (`/verify`, real Rust and SQLite).

## Suggested streams for remaining work (none currently running)

| Stream                 | Owns                                                                                                                                    |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Lead                   | docs, migration numbers, `Cargo.toml`/`package.json`, `db/mod.rs`, integration, encryption, export/deletion, upgrade fixture, UI polish |
| CS cloud (subagent)    | `src-tauri/src/services/sync/**`, `sync_dispatch.rs`, `tests/sync.rs`, `supabase/**`, account/sync/team UI                              |
| MP platform (subagent) | `mobile/**`, `src-tauri/src/{ffi,mobile_jni}.rs`; Android and iOS runtime checks, payment notification journey                          |

Migration numbers: 008 reserved for sync change capture (CS), 009 for the
lead.

## Next actionable tasks

1. CS: Rust sync client (D7) and email sign-in (D8) against the local
   Supabase stack; journeys 6 and 7.
2. MP: iOS rebuild and simulator run; Android emulator run; journey 9.
3. Lead: SQLCipher (D9), bill split, entitlements scaffold (gating off).

The MP worktree has an uncommitted podspec guard for a missing xcframework;
read it as historical reference before fixing/rechecking native builds in
the main tree. The CS worktree is clean; there is no sync engine to integrate.

## External blockers (owner action)

- Supabase project, Apple/Google sign-in credentials (production auth/sync).
- App Store Connect / Google Play products and RevenueCat keys (paid plans).
- Apple/Windows signing identities (distribution; unchanged from before).

## Notes for later work

- The first session's tax research draft in
  `.claude/worktrees/agent-a17b1e40b2be39cd2/docs/tax-sources.md` also cites
  public ruling PR 3/2023 and the 2026 PCB computerised-calculation
  specification, which would be needed to compute PCB (today PCB is an input).
