# Finance expansion progress

Resume here. Read [DECISIONS.md](DECISIONS.md) (binding architecture) and
[belanje-parity.md](belanje-parity.md) (scope and per-feature status) first.
Branch: `feat/finance-parity` (local only, not pushed).

## How the work is organised

Claude Code Agent Teams was **not available** in the session that started
this work: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` was unset and no team or
shared-task-list tools existed. The work instead uses a lead plus subagents,
each in its own git worktree. Subagents report to the lead and cannot message
each other; the lead owns contracts, migrations, shared files and integration.
This is not an Agent Team and is not described as one.

| Stream                     | Owns                                                                                                                                                                                                   | Must not touch                           |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------- |
| Lead                       | `docs/DECISIONS.md`, migrations numbering, `db/mod.rs`, `Cargo.toml`/`package.json` dependencies, `commands/mod.rs` lists, integration, final verification                                             | —                                        |
| CF core finance            | `src-tauri/src/services/finance/**`, `src-tauri/src/repository/finance*.rs`, `src-tauri/tests/finance.rs`, ledger entries in `dispatch.rs`, ledger hooks in `expense_service.rs` / `exchange_rates.rs` | React, mobile, migrations                |
| UI product interface       | `src/**` except bindings                                                                                                                                                                               | Rust, mobile                             |
| BT business and tax        | migration `005`, `src-tauri/src/{domain,services}/{business,tax}/**`, `src-tauri/tests/business.rs`, `docs/tax-sources.md`                                                                             | React, mobile, ledger services           |
| MP mobile/platform         | `mobile/**`, `src-tauri/src/ffi.rs`, Android/secret-store glue                                                                                                                                         | React `src/**`, ledger/business services |
| CS cloud/security (wave 2) | `supabase/**`, sync engine, auth                                                                                                                                                                       | —                                        |

Migration numbers: 004 ledger (lead, done), 005 business and tax (BT),
006+ assigned by the lead when needed.

## Baseline (main @ 126d459, 2026-11-05, before any change)

All green: `npm ci`, `npm run assets`, `npm run lint` (0 errors, 7 existing
warnings), `npm run build`, `npm test` (40), `npm run format:check`,
`npm run test:rust` (115 unit + 51 integration), `npm run lint:rust`, mobile
clippy (`--no-default-features --features mobile`), `npm run test:driver`,
`npm run test:e2e` (8/8, Chrome).

Mobile (`mobile/`): `npx tsc --noEmit` passes. `npx expo lint` **fails on
main already**: it resolves the root ESLint config, which ignores `mobile/`,
so every file is ignored and ESLint exits 2. Pre-existing, not a regression.

## Done

- Phase 1: source review, coverage matrix, decisions D1–D10.
- Ledger contract: migration `004_finance_ledger.sql` (schema v4), domain
  types `src-tauri/src/domain/finance.rs` (exported to TypeScript), typed
  client in `src/lib/ipc.ts`, desktop commands generated from one list in
  `src-tauri/src/commands/mod.rs`, mobile read-only naming rule
  (`mobile/src/core/folio.ts`), notification settings.

## In progress

Wave 1 (parallel worktrees): CF ledger services and tests; UI information
architecture and personal-finance screens; BT tax research, tax engine and
business backend; MP Android core, notifications, voice, payment alerts.

## Next actionable task

Integrate CF first (UI and BT depend on it), then UI, BT, MP; rerun the full
check list after each merge.

## External blockers (owner action)

- Supabase project, Apple/Google sign-in credentials (production auth/sync).
- App Store Connect / Google Play products and RevenueCat keys (paid plans).
- Apple/Windows signing identities (distribution; unchanged from before).
