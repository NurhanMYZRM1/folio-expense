# Implementation phases

This records the incremental construction of the previously empty workspace. Commands assume Node and Rust are on PATH. Shared TypeScript declarations are generated into `src/bindings/generated/index.ts` with `npm run bindings`.

## Phase 1 — shell, local database, paths, domain

**Created:** `package.json`, `package-lock.json`, `tsconfig.json`, `vite.config.ts`, `index.html`, `.gitignore`; `src-tauri/Cargo.toml`, `Cargo.lock`, `build.rs`, `tauri.conf.json`, `capabilities/main.json`; `src-tauri/src/{lib.rs,main.rs,error.rs}`, `domain/*`, `db/mod.rs`, `storage/paths.rs`; `src/main.tsx`, `src/app/*`, `src/styles.css`, `src/lib/{ipc,money,constants,errors}.ts`, shared `src/components/ui.tsx`.

**Migration:** `src-tauri/migrations/001_initial.sql` establishes the eight normalized MVP tables, checks, indexes, uniqueness, and sync metadata. New tables are created transactionally; newer unknown schema versions stop startup rather than risking data.

**Implementation/design:** Rust services own a synchronized SQLite connection. Money is integer minor units, with currency-specific decimals; storage paths resolve from the platform app data directory. Explicit enums model statuses. Light/dark/system theme and a dense desktop sidebar provide the shell.

**Commands/tests:** `npm ci`, `npm run assets`, `npm run bindings`, `npm run build`, `npm run test:rust`. Tests verify migrations, money, and path confinement.

**Limits:** No SQLCipher encryption. Local OS protections are the storage security boundary. Browser preview does not persist data.

## Phase 2 — reliable receipt import

**Created:** `storage/receipt_storage.rs`, `repository/{mod,receipt_repository}.rs`, `services/receipt_service.rs`, receipt IPC handlers, `features/receipts/ImportReceipts.tsx`.

**Modified:** `services/mod.rs`, domain receipt types, app navigation, typed IPC client, shared styles and bindings.

**Migration:** No additional migration; receipt/expense/job/audit schema from phase 1 is used.

**Implementation/design:** Native multi-file picker and drag/drop, extension/signature/size/dimension validation, streaming staging copy with SHA-256, unique duplicate check, generated final paths, transactional metadata and durable jobs. Duplicate UI supports opening the existing expense or dismissing the import. Startup cleanup handles generated staging files and orphaned generated receipt directories.

**Commands/tests:** `npm run test:rust`, `npm run test:e2e`. Tests verify original bytes are unchanged, duplicate ID reuse, no duplicate expense rows, and malformed/oversized rejection.

**Limits:** 25 MB/file, 40 MP/image, 100 files per batch. Corrupted PDFs can still be stored for manual entry; rendering/OCR reports decoding errors without losing the file.

## Phase 3 — expense management and receipt viewing

**Created:** `repository/expense_repository.rs`, `services/expense_service.rs`, expense IPC handlers; `features/expenses/{Expenses,ExpenseDetail}.tsx`, `components/{ExpenseTable,ReceiptViewer}.tsx`, `lib/{receiptRendering,encoding}.ts`, dashboard feature.

**Modified:** App routes, styles, bindings, typed IPC.

**Migration:** None beyond phase 1.

**Implementation/design:** Search, category/date/status filters, sorting, 25-row pagination, thumbnails, image zoom/rotate/fit, paged canvas PDF viewer. Optimistic edit versions and per-field manual provenance prevent background overwrite. Submitted/archived claims lock member expenses.

**Commands/tests:** `npm run build`, `npm run test:rust`, `npm run test:e2e`; editor and dashboard screenshots are produced by the workflow test.

**Limits:** Metadata loads in memory for filtering. PDF preview uses canvas rendering rather than a full annotation/editor UI.

## Phase 4 — local OCR and correction

**Created:** `services/extraction/{mod,offline,normalizer}.rs`, `lib/jobs.ts`, `scripts/prepare-assets.mjs`; bundled generated OCR/PDF/font assets under ignored `public` directories.

**Modified:** Receipt processing, field confidence display, expense service, background progress UI.

**Migration:** Uses `extraction_runs`, `field_meta`, and `jobs` created in phase 1.

**Implementation/design:** Bundled English Tesseract WASM runs in a worker, with no CDN/traineddata download at runtime. PDF.js rasterizes receipt pages locally. Rust extracts candidate fields from raw text and validates values; uncertain results require review. The adapter is behind `ReceiptExtractor`, allowing native/ONNX implementations later.

**Commands/tests:** `npm run assets`, `npm run test:rust`, `npm run test:e2e`. E2E extracts actual text and totals from an image using bundled OCR, while blocking external requests.

**Limits:** English model; conservative heuristics; no itemized line extraction. Non-ISO dates use day/month/year. OCR requires the app to remain open; interrupted jobs recover.

## Phase 5 — optional online vision

**Created:** `services/extraction/online.rs`, `security/{mod,secrets}.rs`, settings domain/service/IPC.

**Modified:** Job provider routing, validation, settings UI, confidence metadata and audit records.

**Migration:** No additional migration; secrets are excluded from SQLite.

**Implementation/design:** Rust HTTPS client, structured JSON schema, independent validation, bounded responses, no redirects, timeouts, optional provider/model configuration. macOS Keychain and Windows Credential Manager store write-only API credentials. Failed/unconfigured online requests fall back to offline OCR. Endpoint changes clear saved credentials.

**Commands/tests:** Rust response-validation, secret-not-in-database, and credential-clearing tests. A live-provider smoke test requires the user's own credential and explicitly enabled setting; it is not run automatically.

**Limits:** Chat Completions vision/schema compatibility is required of third-party providers. No credential or paid API request is needed for local use.

## Phase 6 — claims, totals, and reports

**Created:** `repository/claim_repository.rs`, `services/claim_service.rs`, claim IPC; `features/claims/Claims.tsx`, `lib/{pdf,pdf.worker}.ts`.

**Modified:** Typed IPC, routes, background dispatch, settings, generated bindings.

**Migration:** No additional migration; uses claims/membership and export jobs.

**Implementation/design:** Same-currency ready expenses are grouped into draft claims. Membership, totals, status transitions, and locking are validated in Rust. A durable immutable snapshot drives PDF generation in a worker. Reports include company/employee placeholders, IDs, date, totals, repeated table headers, receipt IDs/filenames, deep links, and optional image/PDF appendices. Original PDF pages are embedded without copying interactive annotations. An app-owned export is always retained.

**Commands/tests:** Claim service tests, PDF pagination/reference tests, full offline workflow, rendered PDF visual QA.

**Limits:** 250 expenses per claim, 30 pages per PDF receipt, bounded appendix memory and 100 MB report limit. Single currency; no conversions. PDF font currently covers Latin text. Custom-scheme links depend on reader support; appendix is the portable option.

## Phase 7 — jobs, preferences, audit, hardening

**Created:** `jobs/{mod,worker}.rs`, `services/settings_service.rs`, `features/settings/Settings.tsx`, job/settings IPC modules.

**Modified:** Service startup recovery, typed IPC, app provider, worker dispatcher, storage and claim validation.

**Migration:** Uses original job/setting/audit tables; no cloud tables or infrastructure.

**Implementation/design:** Durable pending/running/completed/failed jobs with bounded retries, random lease tokens, heartbeats, crash recovery, manual retry, and processing indicators. Atomic mutation audits carry field names and IDs rather than credentials. Settings control appearance, currency, offline/online extraction, export directory, appendix, and employee/company info. CSP excludes remote scripts/frames; renderer has no generic filesystem/shell permissions.

**Commands/tests:** Crash recovery, stale lease, manual override, stale edit, path traversal/symlink, membership, and state transition tests. `npm audit` and Rust Clippy check dependencies/code quality.

**Limits:** No separate background daemon, audit administration UI, or full cloud synchronization. The future sync job type is intentionally inactive.

## Phase 8 — integration, packaging, verification

**Created:** `src-tauri/tests/core.rs`, feature-gated `src-tauri/src/bin/test_driver.rs`, `tests/{bridge.ts,e2e/workflow.spec.ts}`, `src/lib/*.test.ts`, `playwright.config.ts`, `.github/workflows/desktop.yml`, icons, formatting configuration, this documentation and notices.

**Modified:** Build/test scripts, dependency versions, runtime compatibility, lazy loading, and issues found by tests.

**Migration:** None; persistence is verified by closing/reopening real services against the same database.

**Implementation/design:** Test-only stdio bridge uses actual Rust services and SQLite, never an HTTP server. E2E runs actual OCR and pdf-lib, checks duplicates/manual correction/search/claim totals/export/restart, and blocks external browser requests. Packaging includes all OCR, font and PDF resources. CI covers Apple Silicon and Windows builds and tests; release signing uses owner-provided secrets only.

**Commands/tests:** See README and [VERIFICATION.md](VERIFICATION.md) for commands and observed results.

**Limits:** Windows GUI behavior and installer smoke checks need a Windows machine; CI configuration is not evidence of an executed Windows run. Live AI compatibility, real keychain prompts, and signed public distribution need platform/account-specific verification. See the verification record for what actually ran.
