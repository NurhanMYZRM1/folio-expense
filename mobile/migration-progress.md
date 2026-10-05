# Folio Expense mobile — migration worklist

## Decisions

- **Core stays in Rust.** `../src-tauri` builds without Tauri. The `mobile`
  feature exposes a C ABI (`src/ffi.rs`: `folio_open`, `folio_invoke`,
  `folio_string_free`) over the JSON dispatcher shared with the test driver
  (`src/dispatch.rs`). Data lives in Application Support/expense-app, the same
  layout as the desktop.
- **Native module** `modules/folio-core` (Swift): core calls on a background
  queue, Apple Vision OCR (replaces tesseract.js), PDFKit page rendering
  (replaces pdf.js). Vendors `FolioFFI.xcframework`, built by `npm run rust:ios`.
- **Jobs** run once natively (`src/core/jobs.ts`, port of `../src/lib/jobs.ts`):
  OCR → `complete_ocr`, previews → `complete_thumbnail`, claim PDFs with the
  shared pdf-lib generator → `complete_pdf`.
- **DOM screens** reuse `../src/features/*` with Tauri APIs shimmed
  (`src/dom/shims`); exports open the share sheet under their desktop names.
- Needs a dev build (custom native module): `npm run ios`.

## Routes

| Desktop route | Native route | State |
|---|---|---|
| `/import` | `(tabs)/import` (Scan) | ✅ native — camera / photos / files |
| `/expenses` | `(tabs)/expenses` | ✅ native — segmented status, search, thumbnails |
| `/` | `(tabs)/(home)` | DOM shell (phone layout) |
| `/expenses/:id` | `expenses/[id]` | DOM shell (phone layout) |
| `/claims` | `(tabs)/claims` | DOM shell (phone layout) |
| `/claims/:id` | `claims/[id]` | DOM shell (phone layout) |
| `/settings` | `(tabs)/settings` | DOM shell (phone layout) |

## Shared features ported

- **HEIC receipts** (desktop `feat/heic-receipts`): the Rust core decodes HEIC
  itself (pure Rust, so it builds for iOS unchanged). Photos are now picked at
  full quality in their current representation, so iPhone photos are stored as
  the untouched HEIC original instead of a re-compressed JPEG; Files accepts
  `.heic/.heif`; rare formats (ProRAW, WebP…) are converted to JPEG first.
  OCR and thumbnails read the JPEG rendition (`receipts/<id>/display.jpg`).
- **Motion polish** (desktop `feat/motion-polish`): DOM screens get it from the
  shared code (`motion` + `@types/react-dom` added here). Native screens use
  `src/native/motion.tsx` — the same spring presets on Reanimated: staggered
  list entrances, per-file import progress bar, toasts that spring in and
  leave quickly, press scale, soft shadows, and the desktop's AA colour tokens.

## nativize-later

- [ ] Expense detail (edit form → native form; viewer → native zoomable image)
- [ ] Claims list / detail
- [ ] Overview

## Verified (2026-10-04, iPhone 17 Pro simulator, iOS 26.5)

- Synthetic receipt imported from Photos → Vision OCR → expense "KEDAI KOPI
  SAMPLE", 3 Oct 2026, MYR 24.38 (tax 1.38), Meals, status ready; thumbnail stored.
- Claim created, receipt added (MYR 24.38), PDF generated (129 KB), CSV exported
  and opened in the share sheet.
- No Metro ERROR/WARN after launch.
- Desktop unaffected: lint (0 errors), 21/21 vitest, web build, prettier,
  cargo fmt, clippy (desktop + mobile, -D warnings), 103 Rust tests.

## Not done

- Android: no Android SDK/NDK on this Mac. Needs `cargo-ndk` targets and a
  Kotlin side for `modules/folio-core`, then `eas build -p android`.
- Online AI extraction is wired (`try_online`) but untested here (needs an API key).
