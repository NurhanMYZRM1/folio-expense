# Accounts and sign-in

Folio signs in through the existing Supabase project (`folio-expense`, reference `jvzaufwijwdqqhmhroet`). Three ways lead to the same account: the emailed sign-in link, Google, and Apple. Signing in is optional; everything works on the device without it.

## What the owner must configure before Google and Apple work

Checked on 10 November 2026 through the project's public Auth settings: **Google and Apple are both disabled**, so the buttons return Supabase's "provider is not enabled" error until the steps below are done. Email sign-in is unaffected. None of this can be done from the app or its source, and no secret belongs in the repository or a chat.

1. **Google.** In Google Cloud Console create an OAuth client of type _Web application_. Authorized redirect URI: `https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/callback`. In Supabase → Authentication → Sign In / Providers → Google, enable it and enter that client's ID and secret.
2. **Apple.** In the Apple Developer account create a Services ID with Sign in with Apple enabled. Domain: `jvzaufwijwdqqhmhroet.supabase.co`. Return URL: `https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/callback`. Create a Sign in with Apple key. In Supabase → Providers → Apple, enable it and enter the Services ID and the generated secret. Apple's secret expires (at most six months); it has to be regenerated.
3. **Return address.** Supabase → Authentication → URL Configuration → Redirect URLs: add exactly `expenseapp://auth/callback`. No wildcard.
4. **Adding a provider to an existing account.** Supabase → Authentication → Sign In / Providers → enable **manual linking**. Without it, _Add Google sign-in_ and _Add Apple sign-in_ in Settings are refused by Supabase.

The two addresses are different on purpose. Google and Apple return to Supabase (`…/auth/v1/callback`); Supabase then returns to the app (`expenseapp://auth/callback`). Receipt links (`expenseapp://receipt/<id>`) are a separate path and are not treated as sign-in.

After configuring, verify in this order: sign in by email as the existing owner on the Mac → Settings → _Add Google sign-in_ → sign out → _Continue with Google_ → the same records appear. Then the same on the iPhone.

## How sign-in works

- The app opens the provider in the **system browser** (the Mac's default browser; on iPhone the system authentication sheet). It never shows a provider page in an embedded view.
- It uses the authorization-code flow with PKCE. The verifier is created in the Rust core and stays in memory, so only the running app that started a sign-in can finish it. A callback that arrives twice, late (after five minutes), after the app was restarted, or for a sign-in started elsewhere is refused and changes nothing; start again from Settings.
- The session's refresh credential is stored in the operating system's credential store (Keychain), one entry per local database. It is never written to SQLite, logs, or the web layer; the shared screens only receive a status.
- On the iPhone the address the browser returns to is handled natively and never reaches the shared web screens.

## One account per set of records

Each local database belongs to at most one cloud account, identified by the account ID Supabase verified. **Emails and names are never compared by the app.**

- **The existing owner.** A database that connected under the email-only release is bound to that owner (migration 009). Signing in by email keeps working exactly as before.
- **Keeping the owner's records when adding Google or Apple.** Either add the provider from Settings while signed in (the signed-in session is the proof of ownership), or sign in with a provider whose verified email is the account's email, which Supabase itself links to the same account. A provider identity that belongs to a different account opens that account's own, separate records; it is never merged into the owner's.
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
