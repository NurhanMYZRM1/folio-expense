# Architecture

## Tauri decision

Tauri v2 remains the desktop host. There is no required MVP capability that warrants replacing it with Electron. Native dialogs, drag/drop, OS credential stores, background Rust calls, local SQLite, and deep-link registration are supported. Tesseract WASM and PDF.js avoid a native OCR/PDF renderer installation on macOS and Windows.

## Boundaries

```text
React features → typed IPC client → Tauri commands (spawn_blocking)
                                      ↓
                                  AppService
                                      ↓
                            repositories + SQLite
                                      ↓
                         app-owned receipt/export storage

Durable SQLite jobs → leased renderer job dispatcher
    extract_receipt → Rust OnlineVisionExtractor (optional)
                   → Tesseract WASM Web Worker (offline)
                   → Rust LocalOcrExtractor heuristics + validation
    generate_thumbnail → bounded local preview → Rust validated write
    generate_pdf → immutable Rust snapshot → pdf-lib Web Worker → Rust commit
```

The renderer handles portable WASM OCR and PDF rendering; it cannot directly mutate SQLite, read arbitrary local files, or retrieve stored credentials. Business rules, state changes, money validation, optimistic versions, duplicate checks, final extraction merges, and file writes live in Rust. `ReceiptExtractor` is a Rust provider interface independent of OpenAI/Tesseract. Offline OCR produces raw text; the Rust adapter normalizes it. Online extraction is initiated by Rust only after reading current user preferences and the OS keychain.

Full-size receipt content is requested only by the detail viewer, a worker, or an appendix export. Tables request small WebP thumbnails. PDF.js is dynamically loaded when a PDF or worker actually needs it. Heavy OCR and PDF assembly run in Web Workers; all filesystem/database/network calls run on Tauri's blocking pool. A single worker serializes processing to bound memory and CPU use. Metadata queries release the DB lock before network calls.

## Source of truth and schema

`001_initial.sql` creates eight tables: `expenses`, `receipt_files`, `extraction_runs`, `expense_claims`, `claim_expenses`, `audit_events`, `jobs`, and `settings`. Migrations run in a transaction and reject databases newer than the app. Receipt hashes and memberships have unique constraints, money has integer range checks, and statuses have enumerated check constraints. UUID IDs, version counters, UTC timestamps, and `sync_state` prepare mutable records for future replication. Expense dates are date-only strings, not midnight timestamps.

Expense fields are normalized columns. `field_meta` stores per-field confidence and provenance. Extraction runs preserve their validated result and local OCR text separately for traceability. No receipt content, merchant text, or credentials are written to diagnostic logs. The reserved logs folder is intentionally empty for this MVP; operational outcomes are local jobs/audit rows.

A claim has a fixed currency; add/remove and status changes are transactional. Membership is unique per expense. Submitted/archived expenses cannot be edited or extracted until their claim is reopened. Background jobs prevent submitting/archiving while extraction is active. Claim totals come from integer SQL sums, not cached frontend totals. Exports use an immutable snapshot captured when queued; subsequent edits do not alter a report already in progress.

## Receipt ingestion and recovery

1. Check extension, regular file, size, signature, image dimensions.
2. Copy into a unique app-owned `.part` file; calculate SHA-256 while streaming; flush it to disk.
3. Check the unique hash. Duplicate results carry the existing expense ID.
4. Atomically rename to a generated final receipt directory; sync the containing directory on Unix.
5. In one SQLite transaction insert receipt, expense, audit events, and pending jobs; commit.
6. Workers can only lease jobs after this transaction commits.

Durable job insertion is inside the receipt transaction (execution is strictly after commit), closing the crash window between metadata commit and scheduling. A failed metadata transaction cleans its generated file. On startup, generated `.part` files and unreferenced UUID receipt directories are cleaned conservatively; original user files are never modified. Unknown directory names are untouched. Path resolution rejects traversal and symlink escapes.

Leases use random tokens; stale completions cannot overwrite the current job. Heartbeats expire after two minutes. Interrupted running jobs resume at startup, up to three attempts before requiring an explicit retry. A renderer reload is recovered by lease expiry. A failure keeps the receipt and moves an extracting expense to review; manual entry always remains available. Failed exports never remove a claim.

## Extraction and manual values

- Online uses strict JSON Schema, bounded response bytes, a 45-second timeout, HTTPS, and no redirects.
- Rust deserializes with unknown-field rejection and independently validates dates, currency, categories, amounts, tax/total relationship, and confidence bounds.
- A later extraction merges only fields whose source is not `manual`, including protection for manually cleared values. Optimistic versions reject stale editor writes with a reload message.
- Low-confidence core fields result in `needs_review`. A complete high-confidence extraction can be ready. Confidence indicates extraction certainty, not policy approval.
- Offline heuristics use labelled totals/tax, ISO or day-first dates, supported currency codes, and a candidate merchant line, and earn confidence from cross-checks on the receipt itself so a clean scan is ready without review: the total is confirmed by subtotal + tax + service ± rounding − discount, by cash − change, or by a card/e-wallet line paying the same amount (or, on a clearly read page, by being the largest amount); a numeric date's day/month order follows the currency printed on the receipt, and a reading in the future is discarded; a missing currency is inferred from a Malaysian/Singapore address or registration, else the Settings currency is trusted unless the receipt shows a conflicting symbol; a category is suggested from merchant keywords. Tesseract's page confidence travels with the OCR text, and a page under 60/100 caps every field below the ready threshold so unreadable scans still go to review.

## Secrets and renderer security

The only credential IPC operation writes or deletes a secret; the UI receives only `credentialConfigured`. A password input is transient and cleared before awaiting IPC. Rust wraps secrets in zeroizing buffers. Keyring errors have no raw secret or remote response bodies. HTTPS endpoint changes clear the previous credential; credentials do not follow redirects. The app CSP limits assets and connections to the bundled origin and IPC, allows WASM, and disallows remote scripts, frames, and plugins. No generic filesystem or shell permission is granted to the renderer. In-app PDF display draws pages on canvas rather than executing embedded actions.

The app-owned directory uses OS user protections. This is not database encryption or protection from malware running as the same OS account. Exported reports intentionally contain claim/receipt data and may be copied outside the data directory by the user.

## Future sync and enterprise work

The MVP does not enqueue or perform cloud synchronization. `local_only` is truthful even after edits. The reserved `future_sync` job type is not consumed. A future change can write an outbox item in the same local transaction, then upload asynchronously. Network failure must never roll back a local mutation. Sync requires tombstones for deletions, conflict resolution, and per-record remote versions before enabling multi-device writes; these are explicitly deferred.

Repositories/services are cloud-neutral. Add Supabase, AWS, Azure, or a company API behind an outbox transport and receipt-reference resolver. PDF reference construction supports either `expenseapp://receipt/<id>` or a future HTTPS base without changing report layout. Company identities, roles, policies, and approval workflow should be separate domain modules, not booleans bolted onto the expense state machine.

## Primary implementation references

- [Tauri capabilities and least privilege](https://v2.tauri.app/security/capabilities/)
- [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tesseract.js local asset installation](https://github.com/naptha/tesseract.js/blob/master/docs/local-installation.md)
- [OpenAI structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
