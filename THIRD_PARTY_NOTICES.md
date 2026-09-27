# Third-party notices

Folio depends on open-source packages locked by `package-lock.json` and `src-tauri/Cargo.lock`. Preserve upstream license files when distributing binaries. This list highlights bundled runtime assets; it does not replace every transitive dependency's license.

| Component                  | Use                                              | Upstream license                                    |
| -------------------------- | ------------------------------------------------ | --------------------------------------------------- |
| Tauri                      | Desktop host, plugins                            | MIT / Apache-2.0                                    |
| React, React Router        | Interface and navigation                         | MIT                                                 |
| Tailwind CSS, Vite         | UI styles and build tooling                      | MIT                                                 |
| SQLite / rusqlite          | Local database                                   | Public domain / MIT                                 |
| Tesseract.js               | Offline OCR worker                               | Apache-2.0                                          |
| Tesseract / Leptonica WASM | Bundled OCR engine                               | Apache-2.0 / BSD-2-Clause                           |
| `@tesseract.js-data/eng`   | Bundled English traineddata                      | Package MIT; upstream Tesseract tessdata Apache-2.0 |
| PDF.js (`pdfjs-dist`)      | Local PDF rendering, CMaps, fonts, WASM decoders | Apache-2.0 and bundled asset notices                |
| pdf-lib, fontkit fork      | PDF creation and embedded fonts                  | MIT                                                 |
| Noto Sans                  | Local report font                                | SIL Open Font License 1.1                           |
| Lucide                     | Interface icons                                  | ISC                                                 |

`npm run assets` copies the upstream OCR, font, and PDF notices into `public/licenses`, which ships with the app. Inspect the locked package's license before changing asset sources or adding OCR languages. The Folio icon is a simple vector created for this application, not a stock asset.
