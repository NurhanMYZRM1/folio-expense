# Finance expansion progress

Resume here. Read [DECISIONS.md](DECISIONS.md) (binding architecture) and
[belanje-parity.md](belanje-parity.md) (scope and per-feature status) first.
Branch: `feat/finance-parity` (local only, not pushed).

## Session history

| Session        | Agent                          | Outcome                                                                                                                                                                                                      |
| -------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1              | Claude Code (lead + 4)         | Phase 1: matrix, decisions D1–D10, ledger contract (`848cd63`, `c5d5a14`). Subagents stopped at the usage limit before writing code; their worktrees under `.claude/worktrees/` hold only superseded drafts. |
| 2              | Codex (lead + 3)               | Phases 2–4 and parts of 3, 5, 6 written in the main tree, uncommitted, then stopped at the usage limit mid simulator checks.                                                                                 |
| 3 (2026-11-05) | Claude Code (lead + subagents) | Verified and committed session 2 (`b9735bd`); fixed the finance e2e locators; continuing Phase 5–7.                                                                                                          |

Agent Teams (shared task list, teammate messaging) has not been available in
any session; work uses a lead plus subagents in separate worktrees. That is not
an Agent Team and is not described as one.

## Current state (2026-11-05, `b9735bd`)

Schema v7: `004` ledger, `005` business, `006` sync metadata, `007` receipt
line items. Commands are JSON operations in `dispatch.rs` and
`business_dispatch.rs` (D5).

Implemented and tested in Rust, with desktop UI:

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
- SQLCipher encryption, full data export, account deletion, bill split,
  entitlements, business record deletion (customers, inventory items).
- Category rules (duplicate names, deleting a used category) exist but have
  no tests; a duplicate name surfaces a raw constraint error.
- No upgrade test from a pre-finance (v3, `main`) database with receipts,
  claims and audit history.
- Tax and receipt line-item panels lack body padding and `.check-field`
  styles (seen during verification; functional, not visually coherent).
- Supabase SQL tests have not been run in this session.
- iOS simulator build failed at link time in session 2 (`folio_invoke`
  undefined) while the Rust xcframework was being rebuilt in parallel; the
  rebuilt xcframework contains the symbols. Rebuild to confirm.
- Android: APK builds (arm64); not yet run on the emulator.

## Checks at `b9735bd`

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

## Ownership for the remaining work

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
3. Lead: upgrade fixture, category tests, panel styles, full data export,
   SQLCipher (D9), bill split, entitlements scaffold (gating off).

## External blockers (owner action)

- Supabase project, Apple/Google sign-in credentials (production auth/sync).
- App Store Connect / Google Play products and RevenueCat keys (paid plans).
- Apple/Windows signing identities (distribution; unchanged from before).

## Notes for later work

- The first session's tax research draft in
  `.claude/worktrees/agent-a17b1e40b2be39cd2/docs/tax-sources.md` also cites
  public ruling PR 3/2023 and the 2026 PCB computerised-calculation
  specification, which would be needed to compute PCB (today PCB is an input).
