# Live cloud sync verification

Verified on 6 November 2026, Asia/Kuala_Lumpur, against the existing `folio-expense` Supabase project. Implementation remains in this checkout on `feat/finance-parity`; the older `supabase-integration` worktree contains the original plans/import notes.

## Starting point and fixes

The preceding session completed the Mac's first authenticated sync, built and installed iOS, and stopped with a failing desktop visibility-change test. The simulator was waiting at an OS confirmation dialog. Its subsequent Metro bundle contained only the root layout, causing an unmatched route. Restarting the clean simulator with Expo's supported `--initialUrl` launch argument and restarting Metro with `--clear` restored all routes without source changes to routing.

Desktop sync now attempts one final transfer when its window becomes hidden while online. The regression test was observed failing before implementation and now covers departure, offline suppression, timer suspension while hidden, and listener cleanup. Transfer on departure is best effort, not an exit-time durability guarantee; durable pending work provides recovery.

## Live checks

The clean iPhone 17 simulator (`Folio Sync Test`, iOS 26.5) used a separate app container and native keychain session. One new sign-in email was requested through the native app command and consumed through the normal verification command. No privileged token or fabricated owner session was used for app authentication.

| Check                       | Result                                                                                                                                        |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Initial iOS pull            | 198 records, 12 expenses, 3 claims; no pending work or conflicts                                                                              |
| Data comparison             | All 22 synced domain tables match between Mac and iOS, including empty tables                                                                 |
| Receipt transfer            | All 25 files downloaded through native sync; SHA-256 matches the Mac for every file                                                           |
| iOS application UI          | Overview displayed imported expenses/claims; Settings displayed connected account and successful sync                                         |
| iOS → Mac                   | Created a temporary expense category on iOS; the desktop sync engine received it                                                              |
| Mac → iOS                   | Edited only that category through the desktop Rust command adapter; the running desktop engine uploaded it and iOS received version 2         |
| Pending work across restart | Deleted the test category on iOS, confirmed one pending operation after termination, relaunched; native foreground sync uploaded the deletion |
| Session persistence         | Both rebuilt clients resumed authenticated sync after process restart without another email                                                   |
| Device-local sign-out       | Signed out on iOS, restarted it and confirmed signed-out status; restarted Mac and confirmed authenticated sync still succeeds                |
| Cleanup                     | Test category absent on both devices and hosted public table; 198 active cloud records, 25 receipt assets, zero pending operations/conflicts  |
| Original data preservation  | All 22 synced domain tables on Mac match the backup taken before live testing                                                                 |

The temporary category's create/edit/delete advances the shared cursor from 198 to 201. Its tombstone, operation receipts and change history remain intentionally retained. These are protocol history, not outstanding user records. No existing financial record was edited.

After signing out, the native receipt-read command returned the downloaded PDF successfully and the iOS expense screen rendered its page. This verifies local receipt availability without an authenticated cloud session.

The Mac remains connected. The simulator is intentionally signed out following the sign-out isolation test; its downloaded data remains available locally. A complete database copy retains its `vault_id`, so it must not be used as an isolated cloud test database on the same computer.

## Automated checks

- Rust: 212 tests passed with `npm run test:rust`; clippy with warnings denied and Rust formatting passed.
- Web: production build/typecheck and all 43 unit tests passed. The updated lifecycle test also passed after adding offline/cleanup assertions.
- Browser: all 17 Playwright tests passed, including sign-in controls and the single native sync loop boundary.
- Mobile: TypeScript check passed; lint completed with no errors and two warnings.
- Web lint completed with no errors and seven warnings; Vite reported its large-chunk warning. These checks are not warning-free.
- Disposable PostgreSQL 17: all six setup/schema/security stages passed, including owner isolation, replay, stale writes, relational batches, private asset access and anonymous denial.
- Hosted Security Advisor: five informational findings for inaccessible internal sync tables; the existing leaked-password-protection warning remains. This pilot uses email links.

## Limits of this evidence

This is an iOS simulator test, not a physical iPhone, TestFlight or App Store release test. Native commands were driven through the development runtime; the Mac edit used its test-only command adapter against the test category, followed by the actual running desktop sync engine. This is not a complete tap-driven end-to-end UI test.

Restart recovery was exercised live. Packet loss during token renewal, prolonged network outages, OS suspension during a file transfer, and concurrent real-user conflict resolution were not fault-injected against the hosted owner account. Automated suites cover several related failures, but this evidence does not establish zero risk. Sync remains foreground-based and synchronized deletion is not a backup policy.

Local evidence for this continuation is under `/tmp/folio-live-verification/`; the historical pre-test backup remains under `~/Library/Application Support/com.folio.expenses/before-live-sync-test-20261105-234900/`. No authentication email body or token is included in this record.

## Subsequent physical installation

The physical iPhone 17 Pro was subsequently updated wirelessly with a standalone Release build. An iOS 27 launch crash was fixed through Expo's scene-support setting, and the user confirmed the replacement opens. Its two existing expenses and one claim were preserved across the v3-to-v8 migration. This installation and migration check does not claim live cloud convergence on the physical phone. See [physical iPhone testing](IPHONE_TESTING.md) for signing, backup and build details. The additional legacy-workspace regression raises the Rust suite total to 213 passing tests.

## Google sign-in with the account release (12 November 2026)

Build: branch `integration/auth-sync-sdk58` (schema 9), debug build on a clean iPhone 17 Pro simulator (iOS 26.5) with its own empty data folder. The owner completed **Continue with Google** in the system sign-in sheet using the Google account for the existing Folio address. No privileged token or fabricated session was used.

| Check                        | Result                                                                                                                                              |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Settings offered Google only | Apple hidden; the project has Apple off                                                                                                             |
| Account                      | Google linked to the existing owner by its verified address; the project still has exactly one user, whose sign-in methods are now email and Google |
| Simulator workspace          | Bound to the existing owner; 198 records in the 22 synced tables, 0 pending, 0 conflicts                                                            |
| Receipt files                | All 25 files transferred; all 12 originals match their stored size and SHA-256                                                                      |
| Owner's cloud data unchanged | Change counter 201 before and after (every cloud write advances it); 198 live records and 25 receipt files before and after                         |

Checked by the owner in the app and independently through read-only database queries afterwards.

Not covered: Apple (not enabled), a second Google account, a desktop sign-in, and the physical iPhone.
