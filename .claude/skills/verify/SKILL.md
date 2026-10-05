---
name: verify
description: Run the Folio desktop UI against the real Rust backend and bundled OCR in a headless browser, to see a change working (receipt import, extraction, expense editor).
---

# Verify Folio in the running app

The desktop app is Tauri, but the UI can run in Chromium against the real
Rust services through the stdio `test-driver` (the same bridge as
`tests/bridge.ts`). Receipts, SQLite and Tesseract OCR are all real.

## Setup

```sh
npm ci && npm run assets          # assets: OCR model + PDF font (14 tests need them too)
npm run test:driver               # builds src-tauri/target/debug/test-driver
npm run dev &                     # Vite on http://127.0.0.1:1420
```

In cloud containers launch Chromium with
`executablePath: '/opt/pw-browsers/chromium'`. Full `cargo clippy` needs GTK;
use `--no-default-features` there.

## Drive it

Copy the shape of `tests/e2e/workflow.spec.ts` / `tests/bridge.ts`:
spawn `test-driver <tmpdir>/app`, `page.exposeFunction('__folioInvoke', …)`
that returns the receipt paths for `plugin:dialog|open` and forwards every
other command to the driver, and set `window.__TAURI_INTERNALS__` in an init
script.

**Gotcha:** call `page.goto('about:blank')` after `addInitScript` and before
opening the app. Navigating from an already-loaded page only changes the hash,
the init script never runs, and the app shows "Interface preview · Launch the
desktop app" and refuses imports.

Then open `/#/import`, click **Drag receipts into your workspace**, poll
`list_jobs` until every job is `completed`/`failed`, and read results with
`list_expenses` (status, `fieldMeta` confidences, `premises`). The raw OCR
text of each run is in the SQLite `extraction_runs` table under
`<tmpdir>/app/database/expenses.sqlite` (use Python's `sqlite3`; the CLI is
not installed).

## Worth driving

- Undamaged photos (tilted, table background, shadow) should reach `ready`;
  blurred, torn or faded ones should stay `needs_review`.
- `src-tauri/tests/fixtures/*.heic` are tiny red/blue test images, not
  receipts: OCR reads nothing from them by design.
- Expense editor (`/#/expenses/<id>`): edit a field, **Save draft**, reload,
  and check it shows "Manually entered".
