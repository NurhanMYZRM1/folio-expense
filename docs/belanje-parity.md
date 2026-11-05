# Belanje feature coverage matrix

Belanje (by AIZZ TECH SOLUTIONS) is used only as a **functional reference** for
publicly documented user capabilities. Folio keeps its own name, interface,
text and code. Nothing here claims knowledge of Belanje's private screens or
undocumented behaviour. A marketing statement is evidence of an intended
capability, not of how it works.

Sources (read 2026-11-05):

| Key | Source                                      | Notes                                                                                                                                                                                                 |
| --- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L   | https://aizztech.com/belanje/               | Landing page feature list                                                                                                                                                                             |
| P   | https://aizztech.com/belanje/privacy-policy | Last updated 2026-09-08; data categories imply features                                                                                                                                               |
| T   | https://aizztech.com/belanje/terms          | Disclaimers, teams, subscriptions                                                                                                                                                                     |
| R   | https://aizztech.com/belanje/return-policy  | Plus & Pro billing via App Store / Google Play                                                                                                                                                        |
| H   | https://www.aizztech.com/                   | Rendered by its JS bundle: tagline "receipt scanning and voice logging"; screenshot alt text "Maybank balance card, shortcuts for bills, receipts, bill split and analytics, and recent transactions" |

Evidence: **E** = explicitly stated capability; **D** = implied by data the
policy says it collects for that purpose; **I** = implied only (image alt
text); **F** = not in the sources, a Folio decision required by the brief.

Status: **missing**, **partial**, **implemented** (persistence + logic + UI +
tests), **verified** (implemented and exercised end to end on the platforms
named), **blocked** (needs an owner-provided external account or credential).

Owners: **Lead** (architecture, migrations, integration); **CF** core finance
(Rust/SQLite); **UI** product interface (React); **MP** mobile/platform (Expo,
iOS, Android); **BT** business and tax; **CS** cloud and security.
Platforms: D desktop (macOS/Windows, Tauri), i iOS, A Android.

## 1. Personal finance

| ID  | Capability                          | Ev.                             | Folio today                                                                       | To build                                                                                            | Plat. | Owner · deps           | Acceptance                                                                              | Status                        |
| --- | ----------------------------------- | ------------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ----- | ---------------------- | --------------------------------------------------------------------------------------- | ----------------------------- |
| PF1 | Record income and expenses          | E (L, P)                        | Receipt-based expenses for claims only (`services/expense_service.rs`); no income | Ledger `transactions` (D2), create/edit/delete                                                      | D i A | CF, UI · migration 004 | Income and expense rows persist across restart; balances and reports change accordingly | partial — schema and contract |
| PF2 | Categories                          | E (L)                           | Fixed claim categories (`domain/expense.rs`)                                      | Per-workspace income/expense categories: add, rename, archive, delete if unused                     | D i A | CF, UI                 | Defaults seeded once; duplicate names refused; used category cannot be deleted          | partial — schema              |
| PF3 | Notes, dates, payment methods       | D (P)                           | Expense notes/dates only                                                          | Transaction note, local date, payment method enum                                                   | D i A | CF, UI                 | Values round-trip; invalid dates refused                                                | partial — schema              |
| PF4 | Accounts/wallets and total balance  | I (L preview, H "balance card") | None                                                                              | Accounts (cash, bank, e-wallet, card, savings, loan, other) with opening balance; computed balances | D i A | CF, UI                 | Balance = opening + income − expense ± transfers, after any edit or delete              | partial — schema              |
| PF5 | Monthly reports and analytics       | E (L), I (H)                    | Claim totals only                                                                 | `report_monthly`: income, spending, net, by category, daily, vs previous month                      | D i A | CF, UI                 | Totals are integer SQL sums in base currency; unconverted rows counted, not guessed     | missing                       |
| PF6 | Search, filter, edit, delete        | F                               | Expense list search/filter (`features/expenses`)                                  | Transaction filter: dates, account, category, kind, text, amount; paging                            | D i A | CF, UI                 | Filters combine; paging totals correct                                                  | missing                       |
| PF7 | Transfers between accounts          | F                               | None                                                                              | `transfer` kind; excluded from income/spending                                                      | D i A | CF, UI                 | A transfer changes two balances and no report total                                     | partial — schema              |
| PF8 | Multi-currency, explicit conversion | F (existing)                    | Receipt conversion at ECB rates (`domain/exchange.rs`) — **verified**             | Accounts in any supported currency; base amount + rate stored per transaction                       | D i A | CF                     | 0/2/3-decimal currencies exact; offline rows pending, filled by sweep                   | partial                       |

## 2. Budgets

| ID  | Capability                                  | Ev.      | Folio today | To build                                                                                            | Plat. | Owner · deps | Acceptance                                                                            | Status           |
| --- | ------------------------------------------- | -------- | ----------- | --------------------------------------------------------------------------------------------------- | ----- | ------------ | ------------------------------------------------------------------------------------- | ---------------- |
| BG1 | Monthly budgets with progress and remaining | E (L, P) | None        | Budget per category or overall, per calendar month, base currency                                   | D i A | CF, UI · PF1 | Progress recomputes after edits/deletes; percent floor; over shows negative remaining | partial — schema |
| BG2 | Over-budget alerts                          | E (L, P) | None        | Alert rows at `alert_percent` and 100%, once per budget/month/threshold; delivered as notifications | D i A | CF, MP       | Crossing twice in a month alerts once; disabled alerts never fire                     | partial — schema |

## 3. Bills and subscriptions

| ID  | Capability                              | Ev.      | Folio today | To build                                                                 | Plat. | Owner · deps | Acceptance                                                                      | Status           |
| --- | --------------------------------------- | -------- | ----------- | ------------------------------------------------------------------------ | ----- | ------------ | ------------------------------------------------------------------------------- | ---------------- |
| BL1 | Bills and subscriptions with recurrence | E (L, P) | None        | Once/weekly/monthly/yearly, interval, end date, cancel                   | D i A | CF, UI       | Month-end clamping (31 Jan → 28/29 Feb → 31 Mar); re-anchoring on edit          | partial — schema |
| BL2 | Reminders                               | E (L, P) | None        | `list_reminders` with stable ids; OS notifications rescheduled on change | D i A | CF, MP       | Edit/cancel updates future reminders; timezone change moves them to local 09:00 | missing          |
| BL3 | Payment status, upcoming, overdue       | F        | None        | Occurrence list; pay = one linked expense                                | D i A | CF, UI       | Paying twice yields one transaction; deleting it reopens the occurrence         | partial — schema |

## 4. Loans and savings goals

| ID  | Capability                                           | Ev.   | Folio today | To build                                                             | Plat. | Owner · deps | Acceptance                                                          | Status           |
| --- | ---------------------------------------------------- | ----- | ----------- | -------------------------------------------------------------------- | ----- | ------------ | ------------------------------------------------------------------- | ---------------- |
| LN1 | Loans: principal, balance, repayments, progress      | D (P) | None        | Loan accounts; repayments are transfers (D2); no interest projection | D i A | CF, UI       | Outstanding and progress exact after each repayment                 | partial — schema |
| SG1 | Savings goals: target, contributions, date, progress | D (P) | None        | Savings accounts with target; contributions are transfers            | D i A | CF, UI       | Progress = balance/target; total balance unchanged by contributions | partial — schema |

## 5. Receipts and quick entry

| ID  | Capability                         | Ev.      | Folio today                                                          | To build                                                                                 | Plat.               | Owner · deps | Acceptance                                                                                  | Status                       |
| --- | ---------------------------------- | -------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------- | ------------ | ------------------------------------------------------------------------------------------- | ---------------------------- |
| RC1 | Scan receipt: merchant, total, tax | E (L, P) | OCR + optional online extraction, review, duplicates, originals kept | —                                                                                        | D i                 | —            | Baseline e2e 8/8 (desktop, 2026-11-05); iOS per `mobile/migration-progress.md` (2026-10-04) | verified (D); iOS not re-run |
| RC2 | Receipt line items                 | E (L, P) | None                                                                 | Items with quantity/unit/amount, editable; reconcile with subtotal, tax, discount, total | D i                 | CF, UI       | Mismatch shown, never silently "ready"                                                      | missing                      |
| RC3 | Quick-add text                     | E (P)    | None                                                                 | Rust parser (English + Malay cues) → editable draft → save                               | D i A               | CF, UI       | Uncertain fields highlighted; nothing saved before review                                   | partial — contract           |
| RC4 | Voice logging                      | E (H)    | None                                                                 | Speech-to-text → same parser and review                                                  | i A (D if feasible) | MP · RC3     | Denied permission and offline recogniser handled                                            | missing                      |
| RC5 | Delete receipts and extracted data | E (P)    | `delete_expenses` removes rows and files                             | Also removes linked transaction (D2)                                                     | D i A               | CF           | No orphan file or row                                                                       | partial                      |
| RC6 | Bill split                         | I (H)    | None                                                                 | Split a total into shares (equal or custom), largest-remainder rounding                  | D i A               | CF, UI       | Shares always sum to total                                                                  | missing                      |

## 6. Business mode

| ID  | Capability               | Ev.          | Folio today                 | To build                                                            | Plat. | Owner · deps | Acceptance                                                     | Status           |
| --- | ------------------------ | ------------ | --------------------------- | ------------------------------------------------------------------- | ----- | ------------ | -------------------------------------------------------------- | ---------------- |
| BZ1 | Separate business mode   | E (L)        | Single local workspace      | Business workspaces (D1) and switcher                               | D i A | CF, UI       | No personal record appears in a business view or query         | partial — schema |
| BZ2 | Business profile         | D (P)        | Company name setting        | Legal name, SSM no., TIN, SST no., address, contact                 | D i A | BT           | Required e-invoice fields validated                            | missing          |
| BZ3 | Customers                | D (P)        | None                        | Customer records with TIN/ID, address, contact                      | D i A | BT           | Used by invoices; deleting a used customer refused             | missing          |
| BZ4 | Invoices                 | E (L, P, T)  | None                        | Line items, totals, SST, numbering, due dates, payments, PDF, share | D i A | BT, UI · PF1 | Payment = one income transaction; invoice itself is not income | missing          |
| BZ5 | Inventory                | E (L, P)     | None                        | Items, stock adjustments, movement history                          | D i A | BT, UI       | Issue moves stock once; void returns it                        | missing          |
| BZ6 | Business reports         | F            | None                        | Revenue, receivables aging, stock value                             | D i A | BT, UI       | Figures reconcile with ledger and invoices                     | missing          |
| BZ7 | Corporate expense claims | F (existing) | Claims, PDF/CSV/XLSX export | Keep first-class                                                    | D i   | —            | Baseline e2e passes after every change                         | verified (D)     |

## 7. Malaysian tax helpers

| ID  | Capability                           | Ev.          | Folio today | To build                                                                          | Plat. | Owner · deps | Acceptance                                                       | Status  |
| --- | ------------------------------------ | ------------ | ----------- | --------------------------------------------------------------------------------- | ----- | ------------ | ---------------------------------------------------------------- | ------- |
| TX1 | Income-tax estimate                  | E (L, P, T)  | None        | Resident rates by year of assessment from LHDN sources                            | D i A | BT           | Worked examples match official tables; unsupported years refused | missing |
| TX2 | Reliefs and marital status           | E (L), D (P) | None        | Relief caps by year, sourced                                                      | D i A | BT           | Caps enforced; each figure cites a source                        | missing |
| TX3 | PCB/MTD inputs and estimate          | D (P), E (T) | None        | MTD per LHDN computerised-method specification, or inputs only where unverifiable | D i A | BT           | Matches official worked examples or says it cannot               | missing |
| TX4 | SST calculation and invoice fields   | E (L, P, T)  | None        | Sales/service tax rates by effective date, from Customs sources                   | D i A | BT · BZ4     | Rate history respected by invoice date                           | missing |
| TX5 | E-invoice preparation and validation | E (L, T)     | None        | MyInvois field set and validation; prepared document only                         | D i A | BT · BZ2-4   | Never submitted; labelled "prepared, not submitted"              | missing |
| TX6 | Estimate disclaimers                 | E (T)        | —           | Clear labelling in every tax screen                                               | D i A | UI           | Visible on each result                                           | missing |

## 8. Accounts and sync

| ID  | Capability                       | Ev.          | Folio today               | To build                                      | Plat. | Owner · deps     | Acceptance                                      | Status                                                                                             |
| --- | -------------------------------- | ------------ | ------------------------- | --------------------------------------------- | ----- | ---------------- | ----------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| AC1 | Sign in: email, Apple, Google    | E (P)        | No accounts               | Supabase Auth (D8); optional                  | D i A | CS               | Offline use without sign-in unchanged           | blocked — needs Supabase project and Apple/Google credentials for production; local stack possible |
| AC2 | Cross-device sync                | E (P)        | `sync_state` columns only | Outbox, push/pull, conflicts, tombstones (D7) | D i A | CS · all schemas | Retries never duplicate; deletes never reappear | missing                                                                                            |
| AC3 | Private receipt storage          | E (P)        | Local only                | Private bucket per workspace (D7)             | D i A | CS               | Non-members get 403                             | missing                                                                                            |
| AC4 | Works offline without an account | F (existing) | Yes                       | Preserve                                      | D i   | All              | No network needed for local features            | verified (D)                                                                                       |
| AC5 | Connect local data to an account | F            | —                         | Explicit, previewed upload                    | D i A | CS               | Nothing uploads before confirmation             | missing                                                                                            |

## 9. Business teams

| ID  | Capability                 | Ev.      | Folio today | To build                                  | Plat. | Owner · deps | Acceptance                                       | Status  |
| --- | -------------------------- | -------- | ----------- | ----------------------------------------- | ----- | ------------ | ------------------------------------------------ | ------- |
| TM1 | Invite with connect code   | E (P)    | None        | Hashed, expiring, revocable codes (D6)    | D i A | CS           | Expired/revoked/guessed codes fail; rate limited | missing |
| TM2 | Role-based permissions     | E (T)    | None        | owner/admin/editor/viewer enforced by RLS | D i A | CS           | Viewer writes rejected by the server             | missing |
| TM3 | Remove members             | E (T, P) | None        | Removal revokes access immediately        | D i A | CS           | Removed member's requests fail                   | missing |
| TM4 | Personal data never shared | E (P)    | —           | Personal workspace unshareable            | D i A | CS           | Server refuses members on personal workspaces    | missing |

## 10. Notifications and Android payment detection

| ID  | Capability                               | Ev.                   | Folio today                        | To build                                                                                                  | Plat.  | Owner · deps      | Acceptance                                                                           | Status  |
| --- | ---------------------------------------- | --------------------- | ---------------------------------- | --------------------------------------------------------------------------------------------------------- | ------ | ----------------- | ------------------------------------------------------------------------------------ | ------- |
| NT1 | Bill reminders and budget alerts, opt-in | E (P)                 | None                               | Local notifications from `list_reminders`; per-bill and global toggles                                    | D i A  | MP · BL2, BG2     | Permission asked in context; denial handled                                          | missing |
| NT2 | Android payment notification detection   | E (P)                 | None                               | NotificationListenerService, allowlisted banks/e-wallets, on-device parse, suggestions to confirm, dedupe | A only | MP · Android core | Unrelated notifications never stored or sent; repeated notification → one suggestion | missing |
| NT3 | Android app                              | I (R, T: Google Play) | Not supported (`mobile/README.md`) | Kotlin side of `folio-core`, Rust Android targets                                                         | A      | MP                | App opens, imports a receipt, persists across restart on an emulator                 | missing |

## 11. Privacy and data control

| ID  | Capability                            | Ev.             | Folio today                                    | To build                                          | Plat. | Owner · deps | Acceptance                                                      | Status       |
| --- | ------------------------------------- | --------------- | ---------------------------------------------- | ------------------------------------------------- | ----- | ------------ | --------------------------------------------------------------- | ------------ |
| PV1 | Export my data                        | E (P)           | Claim/expense CSV, PDF, XLSX                   | Transactions CSV; full JSON export of all records | D i A | CF, CS       | Export round-trips all records                                  | missing      |
| PV2 | Delete receipts and records           | E (P)           | Expenses/receipts                              | All new record types                              | D i A | CF, BT       | Deleted rows and files gone locally (and on server once synced) | partial      |
| PV3 | Delete account                        | E (L, P, T)     | No accounts                                    | Server deletion + local choice                    | D i A | CS · AC1     | Server data removed; retention stated matches implementation    | missing      |
| PV4 | Encryption at rest                    | E (L, P)        | TLS in transit; no at-rest encryption (README) | D9                                                | D i A | CS           | No E2E claim; key loss behaviour stated                         | missing      |
| PV5 | Diagnostics without financial content | E (P), existing | No diagnostic logs                             | Keep                                              | D i A | All          | No amounts/merchant text in logs                                | verified (D) |
| PV6 | Withdraw consent                      | E (P)           | Online extraction toggle                       | Notification and payment-alert toggles            | D i A | MP           | Turning off stops further processing                            | missing      |

## 12. Optional paid plans

| ID  | Capability                             | Ev.      | Folio today | To build                                                   | Plat. | Owner · deps | Acceptance                                           | Status                                       |
| --- | -------------------------------------- | -------- | ----------- | ---------------------------------------------------------- | ----- | ------------ | ---------------------------------------------------- | -------------------------------------------- |
| PL1 | Plus/Pro via App Store and Google Play | E (R, T) | None        | Configurable entitlements; gating off until products exist | i A   | MP, CS       | No existing feature gated; no invented prices or IDs | blocked — store products and RevenueCat keys |
| PL2 | Entitlement service (RevenueCat named) | E (P)    | None        | Server-verified entitlements                               | i A   | CS           | Expiry/refund revoke within one refresh              | blocked                                      |
| PL3 | Restore, cancel, renewal               | E (R)    | None        | Restore button; links to store subscription management     | i A   | MP           | Restore works in sandbox                             | blocked                                      |

## Not adopted

- Age 18+ requirement (T) is a legal term of that service, not a capability.
- Live LHDN/MyInvois submission: the sources describe estimates and helpers,
  and the privacy policy says tax figures are "not filed with any authority".
