# Folio — local expense workspace

A Tauri v2 desktop MVP for private corporate expense claims. Rust and SQLite own the data. React/TypeScript provide the interface. The app opens without an account and works without an internet connection, including receipt OCR and PDF export.

## Run

Prerequisites: Node.js 22.13+ (or current LTS), Rust stable 1.90+, and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). macOS requires Xcode command line tools; Windows requires the MSVC C++ build tools. No Tesseract installation or API key is required.

```sh
npm ci
npm run assets
npm run desktop
```

`npm run dev` runs an explicitly labelled, read-only browser preview. Data operations require the desktop runtime. There is no browser localStorage database and no seeded data in the production application.

If using the project-local Rust toolchain installed during development in this workspace:

```sh
export CARGO_HOME="$PWD/.tools/cargo"
export RUSTUP_HOME="$PWD/.tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
npm run desktop
```

## Use

1. Open **Import receipts** and drop or choose PNG, JPG, JPEG, or PDF files.
2. Originals are copied to app-owned storage. Exact duplicates offer **Open existing expense** or cancellation.
3. Bundled Tesseract OCR reads receipts in the background (online AI extraction, when enabled, is tried first). A clean receipt whose figures check out against each other (subtotal + tax = total, cash − change = total, or a card payment of the same amount) is marked ready automatically, with a suggested category; blurry/unreadable scans and receipts whose figures don't agree wait for review alongside the receipt preview. Numeric dates are read day-first (DD/MM/YYYY) unless the receipt's currency is USD, which reads month-first; a date that could be read either way is flagged **Verify** only when the receipt itself doesn't show its currency or region. Currency symbols map to ISO codes (`RM` → MYR, `S$` → SGD, `US$` → USD, `€` → EUR, `£` → GBP, `¥` → JPY unless a China cue such as `CN¥` is present, `₹` → INR, `฿` → THB, `Rp` → IDR, `₩` → KRW). A single field that can't be read is cleared rather than discarding the rest of the extraction.
4. Edit merchant, date, total, tax, currency, category, and business purpose; choose **Mark as ready**.
5. Create a claim, select its currency, and add ready expenses. Each expense can belong to one claim.
6. Export a PDF, a CSV, or both. PDF reports include a summary, receipt reference register, and optional receipt appendix; each summary row's `RECEIPT` cell links internally to that receipt's register or appendix page, and the receipt file name on the register and appendix pages additionally carries a hidden `expenseapp://receipt/<id>` deep link for PDF readers that support custom-scheme links. **Export CSV** is available for a whole claim (claim detail, next to **Export PDF**) or for a hand-picked set of expenses (select rows in the expenses list); CSV files are Excel-friendly (UTF-8 with a BOM, comma-separated, CRLF line endings).
7. Reopen expenses, claims, and generated reports later without a connection.

Marking a claim submitted only updates local status; it does not send a report to anyone. Submitted/archived claim expenses are locked until the claim is reopened.

## Data and privacy

Data lives in Tauri's platform application data directory under `expense-app/`:

```text
expense-app/
  database/expenses.sqlite
  receipts/<uuid>/original.<extension>
  receipts/<uuid>/preview.webp
  exports/claims/<export-job-uuid>.pdf
  exports/csv/<export-uuid>.csv
  cache/
  logs/
```

The app displays the actual data location in Settings. The database contains relative receipt paths, not source paths. SQLite uses foreign keys, WAL, full synchronous commits, migrations, and transactional audit events. Unix app storage is owner-only. On Windows it inherits the current user's AppData ACLs. Receipt/database content is **not additionally encrypted at rest**; use FileVault or BitLocker and a protected OS account where required.

If an export directory is configured in Settings, exporting a CSV also copies a human-named file there (`Folio-<claim number>.csv` for a claim, `Folio-expenses-<date>.csv` for a selected-expenses export). Exporting again after the same name is already taken there — for example after editing an expense and re-exporting — does not fail: it retries with ` (2)`, ` (3)`, and so on inserted before `.csv` until a free name is found.

For a backup, close Folio and copy the entire `expense-app` directory, including any SQLite WAL files. Restore with the app closed. API keys stay in the OS vault and are not part of this backup. Cloud sync is not implemented or required.

Online AI extraction defaults to disabled. If explicitly enabled, Rust sends rendered receipt images to the configured HTTPS OpenAI-compatible Chat Completions endpoint. Credentials are write-only from the UI and stored using macOS Keychain / Windows Credential Manager. They are never read back into React or persisted in SQLite, localStorage, or `.env`. Changing the endpoint clears the old credential. Redirects are disabled. Invalid responses, missing credentials, and network/provider failures fall back to local OCR.

## Build and test

```sh
npm run bindings                # regenerate TS from Rust with ts-rs
npm run build                   # strict TypeScript + Vite production build
npm test                        # money + PDF tests
npm run test:rust                # real SQLite/service integration tests
npm run test:driver              # test-only stdio adapter; no network server
npm run test:e2e                 # React + Rust + SQLite + bundled OCR + real PDF
npm run format:check
npm run lint                     # ESLint for the TypeScript/React code
npm run lint:rust
npm run package                 # native installer on the current platform
```

The E2E test uses installed Chrome locally. In CI it uses Playwright Chromium (`npx playwright install chromium`). Its test-only adapter connects to the same Rust services as the Tauri commands; it does not simulate database responses. It blocks external browser requests and checks disk persistence across a Rust process restart. Native WebView packaging, keychain behavior, drag/drop, and OS deep links additionally require the platform smoke checks in [docs/VERIFICATION.md](docs/VERIFICATION.md).

Build macOS Apple Silicon explicitly with `npm run tauri -- build --target aarch64-apple-darwin`. Build Windows on a Windows host; `npm run package` produces NSIS/MSI bundles. WebView2's offline installer is bundled for Windows. Development dependencies and platform packaging assets require network access at **build time**, never at normal application runtime.

For unattended macOS packaging, use `CI=true npm run package` to skip Finder window decoration while creating the DMG. This keeps packaging independent of an unlocked graphical session.

Unsigned builds are for local evaluation. Public distribution requires your Apple Developer signing/notarization credentials and Windows code-signing identity. No signing credentials or certificates are included.

## Design and delivery notes

- [Architecture, trust boundaries, crash recovery, and extension points](docs/ARCHITECTURE.md)
- [Eight-phase implementation record: files, migrations, commands, tests, and limitations](docs/PHASES.md)
- [Verification results and platform checklist](docs/VERIFICATION.md)
- [Bundled third-party notices](THIRD_PARTY_NOTICES.md)

## MVP limits

- Offline OCR ships an English Tesseract model. Results are heuristic, not accounting advice; low-confidence values remain reviewable. More OCR languages can be bundled later.
- Receipts are limited to 25 MB and images to 40 MP. PDFs can be imported even if unreadable, but OCR/preview/appendix must be able to decode them; encrypted or malformed PDFs may require manual entry. Processing/appendices support up to 30 pages per receipt, 250 expenses per claim, and a 100 MB report limit.
- Claims are single-currency. The supported currency set includes zero-, two-, and three-decimal currencies; exchange rates and conversions are deliberately absent.
- PDF text currently embeds Noto Sans Latin. Non-Latin merchant names remain intact in SQLite but need a broader bundled PDF font for faithful report rendering.
- Durable jobs run while the desktop app is open and recover on restart. There is no separate background daemon, unlimited retry loop, or cloud service.
- The expense register paginates displayed rows; metadata is loaded into memory for filtering. Very large deployments should move filtering/aggregation to paged SQL queries. Full receipt images are never loaded in table rows.
- Deep links are optional convenience links. Every report also includes filenames, receipt IDs, and optionally portable receipt pages because PDF readers differ in custom-scheme support.
- No accounts, sync, collaboration, reimbursements, accounting integration, SSO, policy engine, or approval chain is implemented.
