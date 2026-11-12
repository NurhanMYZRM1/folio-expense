# Accounts and sign-in

Folio signs in through the existing Supabase project (`folio-expense`, reference `jvzaufwijwdqqhmhroet`). Three ways lead to the same account: the emailed sign-in link, Google, and Apple. Signing in is optional; everything works on the device without it.

## Project configuration

State on 12 November 2026, checked through the project's public Auth settings and two test sign-in attempts:

| Setting                                                   | State                                                                                                                                                                                                        |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Email sign-in                                             | On                                                                                                                                                                                                           |
| Google                                                    | **On.** Sign-in goes to Google and returns to `https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/callback`                                                                                                    |
| Return address `expenseapp://auth/callback`               | **Allowed**, exactly. An address not on the list is replaced by the default site URL, so the list is strict as intended                                                                                      |
| Desktop return address `http://127.0.0.1:*/auth/callback` | **Explicitly allowed.** Added and verified in Supabase URL Configuration on 12 November 2026. A live Mac sign-out → Google sign-in returned to the release build and restored the existing owner’s records.  |
| Manual linking (needed for _Add Google sign-in_)          | Reported as enabled by the owner; not observable from outside, so confirmed only when it is first used                                                                                                       |
| Apple                                                     | **Off.** Needs an Apple Developer Program membership, which is not available. Folio hides _Continue with Apple_ and _Add Apple sign-in_ while the project has Apple off, and starting it anyway explains why |

No secret belongs in the repository or a chat.

### Enabling Apple later

1. In an Apple Developer account, create a Services ID with Sign in with Apple. Domain: `jvzaufwijwdqqhmhroet.supabase.co`. Return URL: `https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/callback`. Create a Sign in with Apple key.
2. In Supabase → Authentication → Sign In / Providers → Apple, enable it and enter the Services ID and the generated secret. The secret expires after at most six months and must be regenerated.
3. Nothing changes in the app: the Apple buttons appear once the project reports Apple as on.

Before an App Store release, check Apple's current login-service rule (App Review Guideline 4.8), which can require an additional privacy-focused sign-in option in apps that offer Google.

### Google, for reference

Google Cloud Console OAuth client of type _Web application_, authorized redirect URI `https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/callback`, entered under Supabase → Providers → Google. Redirect URLs contains `expenseapp://auth/callback` for iPhone and `http://127.0.0.1:*/auth/callback` for desktop.

The addresses are different on purpose. Google returns to Supabase (`…/auth/v1/callback`); Supabase then returns to the app:

- **iPhone:** `expenseapp://auth/callback`, which iOS hands straight back to the system sign-in sheet that started the attempt.
- **Desktop:** `http://127.0.0.1:<port>/auth/callback`, a one-time listener the running app opens on this computer for that attempt only ([RFC 8252, section 7.3](https://www.rfc-editor.org/rfc/rfc8252#section-7.3)). An app link is not used on the desktop: macOS can hand it to any installed copy of Folio (old disk images, test builds), and browsers often refuse to open an app at the end of a sign-in redirect, which leaves the provider page loading. The `*` in the allowlist entry matches the port only; it cannot match another host or path.

Receipt links (`expenseapp://receipt/<id>`) are a separate path and are not treated as sign-in.

## How sign-in works

- The app opens the provider in the **system browser** (the Mac's default browser; on iPhone the system authentication sheet). It never shows a provider page in an embedded view. On the desktop, Settings shows _Finish signing in in your browser_ until the browser returns, with a _Cancel sign-in_ button; the attempt also ends after five minutes. The browser tab then says whether sign-in finished and can be closed.
- It uses the authorization-code flow with PKCE. The verifier is created in the Rust core and stays in memory, so only the running app that started a sign-in can finish it. A callback that arrives twice, late (after five minutes), after the app was restarted, or for a sign-in started elsewhere is refused and changes nothing; start again from Settings.
- The session's refresh credential is stored in the operating system's credential store (Keychain), one entry per local database. It is never written to SQLite, logs, or the web layer; the shared screens only receive a status.
- On the iPhone the address the browser returns to is handled natively and never reaches the shared web screens.

## One account per set of records

Each local database belongs to at most one cloud account, identified by the account ID Supabase verified. **Emails and names are never compared by the app.**

- **The existing owner.** A database that connected under the email-only release is bound to that owner (migration 009). Signing in by email keeps working exactly as before.
- **Keeping the owner's records when adding Google or Apple.** Either add the provider from Settings while signed in (the signed-in session is the proof of ownership), or sign in with Google using the account's own address: Google verifies that address and Supabase links the sign-in to the same account. A provider identity that belongs to a different account opens that account's own, separate records; it is never merged into the owner's.
- **Another account on the same device** gets its own folder (`accounts/<account-id>/` inside the data folder) with its own database, receipt files and credential entry. Signing back in as the first account returns to its records. The last active account is remembered across restarts.
- **Records already on a device with no account** never join an account silently. The first sign-in asks:
  - _Add to this account_ — a consistent snapshot is saved first (`database/before-account-<time>.sqlite`), then the records upload and stay in sync. If this is interrupted, signing in again simply continues.
  - _Keep them separate_ — they stay on the device, untouched, and the account opens with its own records.
  - _Cancel sign-in_ — nothing changes.

## Limits

- Signing out keeps that account's records visible on the device; it only stops transfers. There is no in-app way to remove an account's local folder.
- The original, account-less records are reachable only while no other account is active on the device; after choosing _Keep them separate_ they remain on disk in the original data folder.
- Generated PDF, CSV and Excel exports stay on the device that made them.
- Sync runs while the app is open and catches up when it returns to the foreground. iOS does not allow continuous background sync.

## Live desktop verification — 12 November 2026, 14:19 MYT

Quit the older Folio instance and opened this worktree's Release app built from the loopback fix (`eed3cde`, documentation `8d17c42`). After the owner cleared the new build's Keychain prompt, signed out on the Mac and selected Continue with Google. The browser returned to the one-time loopback listener and displayed “You’re signed in”; Folio showed the same original owner, Email and Google, and no pending changes. No new account or data-adoption choice was created.

Before/after read-only checks retained one user, counter 231, 227 live records, and 29 receipt assets. Aggregate record and asset fingerprints were identical. All 35 local tables were compared with a fresh SQLite snapshot; only the authentication/sync control row changed. The 14 expenses and 5 claims were unchanged, schema stayed at 9, and integrity was `ok`, with zero pending changes/conflicts. The iPhone sign-in method was not changed by this desktop test.
