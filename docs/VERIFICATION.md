# Verification record

Run date: 27 September 2026. Host: Apple Silicon (`arm64`), macOS 26.6.2, Node 26, Rust 1.98.1. These results describe observed checks, not a claim that every supported operating system was tested.

## Automated results

Prettier and Rust formatting checks also passed.

| Check                                                                                | Result                                          |
| ------------------------------------------------------------------------------------ | ----------------------------------------------- |
| Strict TypeScript and Vite production build                                          | Passed                                          |
| Rust unit + integration tests (`cargo test`)                                         | 73 passed (52 unit, 21 integration)             |
| TypeScript unit tests (money, receipt image pre-processing, PDF)                     | 20 passed                                       |
| CSV export tests (Rust unit + integration, plus the E2E export step)                 | Passed                                          |
| PDF internal receipt-link tests (summary/register/appendix links, hidden deep links) | Passed                                          |
| Browser + real Rust/SQLite integration                                               | 2 passed                                        |
| Rust Clippy, all default-feature targets, warnings denied                            | Passed                                          |
| Dependency audit (`npm audit`)                                                       | 0 reported vulnerabilities at verification time |
| Rust Clippy, `test-support` targets, warnings denied                                 | Passed                                          |
| macOS Keychain (`platform-verify keychain`)                                          | Passed                                          |
| macOS Apple Silicon release packaging                                                | `.app` and `.dmg` built                         |
| macOS native release smoke test                                                      | Passed                                          |
| Live online AI extraction (Google Gemini)                                            | Passed                                          |
| Windows build / native smoke test                                                    | Not executed here; CI matrix provided           |

The first end-to-end test imports a generated PNG through the app's picker boundary, copies it through the real Rust ingestion service, runs bundled Tesseract OCR, detects a duplicate, changes fields, marks the expense ready, creates a claim, produces an actual three-page PDF, checks receipt links and totals, exports the claim to CSV and verifies the file exists with the expected merchant and amount, searches expenses, restarts Rust, and verifies the same records and manual provenance remain. The original file's bytes are unchanged.

The second test imports a two-page PDF, locally renders both pages, extracts the real total with Tesseract, navigates the PDF viewer, and generates a four-page report with both original receipt pages embedded. Both tests block external browser requests and assert that none were attempted. No credential, cloud database, CDN, or production data is involved. The test bridge runs real application services; native window/dialog mechanics are tested separately.

Rust coverage includes migrations and constraints, exact money/currency precision, file signatures and size limits, duplicate hashing, relative paths and symlink confinement, state transitions, same-currency claim totals/membership, strict extraction validation, manual edits and manual clears, optimistic edit conflicts, interrupted jobs and stale leases, WebKit PNG-to-WebP thumbnail conversion, credential exclusion/endpoint changes, and old report visibility after queue rollover. It also covers extraction field sanitisation for both the offline OCR and online AI paths (currency symbol and month-name/date normalisation in English and Malay, day-first-except-USD date resolution, confidence clean-up down to exactly the expected fields, a hostile-input case that must still satisfy the same validation the worker applies), the online provider's printed-currency/date override rules (an unambiguous printed symbol wins, a bare `¥` or `$` defers to the model's own currency when that is already consistent with the symbol), and CSV export (column order and formatting, RFC 4180 quoting and formula-injection guarding, per-currency decimal exponents, and retrying with a `(2)`, `(3)`, … suffix when the export directory already has a file with that name).

## Native macOS observations

A packaged debug `.app` launched with no account and an empty workspace. The native file picker imported a synthetic receipt into the platform data directory. The packaged offline OCR engine extracted **KOPI HOUSE / MYR 84.50** and populated the review queue. Navigation and the dark desktop layout were inspected in the native WebView.

That smoke test exposed WebKit returning PNG when asked for a WebP canvas thumbnail. Rust now validates and re-encodes the thumbnail as canonical WebP, with a passing regression test. The release package includes this fix.

The final release was rebuilt after the credential-store change below and installed to `/Applications/Folio.app`; the installed executable's SHA-256 matches the build output. In the installed release, the synthetic receipt's thumbnail and OCR jobs completed. The merchant, business purpose, and Meals category were edited and saved, and the expense was marked ready. Re-extraction reported that manual corrections are kept. A claim was created and the expense added (total MYR 84.50). **Export PDF** produced a three-page report with the summary, reference register, and receipt appendix. After quitting and relaunching, the claim, its total, and the completed report were still listed, and **Open PDF** opened the saved report in Preview.

## Credential store

`platform-verify keychain` is a feature-gated binary (`--features test-support`) that is not included in desktop bundles. It uses the same `SecretStore` operations as production, against a disposable `com.folio.expenses.verification` Keychain entry with a random UUID account, so the user's real credential is never read or replaced. It checks create, read, persistence across a separate process (compared by SHA-256, never printed), replace, delete, and idempotent delete. It passed on this Mac and the temporary entry was removed.

`platform-verify ai <synthetic-receipt.png>` runs a live extraction against the endpoint and model saved in Folio's Settings, using the credential stored through Settings. It passed against Google Gemini's OpenAI-compatible endpoint (`https://generativelanguage.googleapis.com/v1beta/openai`, model `gemini-3.8-flash`): the synthetic receipt returned schema-valid structured data with MYR 84.50. It was re-run after this pass's extraction prompt and response-parsing changes (the system/user prompt split, the `dateAsPrinted`/`currencyAsPrinted` fields, and the printed-currency override fix) and passed again with the same result. An earlier attempt against OpenAI returned HTTP 429 (no API credit) and Folio reported the planned local-OCR fallback message. Google no longer offers `gemini-2.5-flash` to new accounts (HTTP 404). Keys are entered only in **Folio → Settings → API credential**. To repeat the check:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --features test-support --bin platform-verify ai <synthetic-receipt.png>
```

The production code starts empty. Native smoke testing added one synthetic receipt to this machine's local Folio workspace; it is not bundled sample data.

The final release `.app` is approximately 34.6 MiB and the `.dmg` is 21.5 MiB. The disk image passed `hdiutil verify`. The final bundle contains only the production executable; the binding generator and test driver are feature-gated development tools. Packaging succeeded with `CI=true` after the interactive Finder decoration step failed in the locked session.

Installer SHA-256: `10479421baf59086925260fea787984413c09419fe9f6c2ea8629872aef39292`.

## Report and interface inspection

The generated report summary, reference register, image appendix, and both PDF appendix pages were rasterized and inspected for clipping, overlap, readable amounts, and page references. PDF unit tests also cover pagination, reference construction, and empty source pages. Light/dark dashboard, editor, and claim detail screenshots are in [screenshots](screenshots/). They use synthetic data from the integration test.

## Reproduce

```sh
npm ci
npm run assets
npm run bindings
npm run build
npm test
npm run test:rust
npm run test:driver
npm run test:e2e
npm run format:check
npm run lint:rust
npm run package
```

Use the project-local Rust environment described in the README if Rust is not on PATH. On CI, Playwright downloads Chromium at build/test time. Local tests use installed Chrome. Build artifacts and generated test reports are ignored by source control.

## Platform release checklist

Run on an Apple Silicon Mac and a Windows 11 machine before public distribution:

- Install the native bundle under a normal user account; start while disconnected with online AI disabled.
- Import PNG, JPEG, a scanned PDF, and a multipage PDF through both drag/drop and the native multi-file picker. Confirm original bytes remain unchanged.
- Confirm thumbnail, local OCR, manual correction, search/filter/sort, currency handling, and claim totals. Re-extract after a correction and verify the manual value survives.
- Generate/open reports with and without appendices. Test long names, non-Latin text limitations, corrupt/encrypted PDFs, and an unwritable export folder.
- Close/reopen during processing and after a completed claim. Verify recovery, data persistence, and reopening an older generated report.
- Test the optional `expenseapp://receipt/<id>` handler using supported PDF readers. Filename/UUID references and appendices must remain useful even when a reader blocks custom links.
- Configure a test provider with your own credential. Verify OS-vault storage and deletion, valid extraction, invalid/refused responses, disabled online mode, and network-failure fallback. On macOS, OS-vault storage was verified with a disposable entry (see Credential store); a live extraction passed against Google Gemini. Refused/invalid responses and network-failure fallback in the UI remain to be exercised.
- On Windows, test a clean installation with WebView2 absent using the bundled offline installer, native drag/drop, Credential Manager, and uninstall/reinstall data retention.
- Apply owner-provided macOS signing/notarization and Windows code signing. Test downloaded signed installers on clean machines. Current artifacts are unsigned evaluation builds.

The GitHub Actions matrix is configuration for these platforms, not evidence of an executed Windows run. No cloud synchronization or online account is needed to execute the core offline checklist.
