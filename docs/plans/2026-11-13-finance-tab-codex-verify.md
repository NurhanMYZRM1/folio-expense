Verify that the uncommitted mobile change in this worktree (Finance gets its own bottom-bar tab; Settings moves out of the bar) breaks nothing, and fix anything it does break.

Worktree: /Users/nurhanfawwaz/orca/workspaces/folio-expense/Independent-section-for-finance-in-mobile
Baseline: `git diff 730e93d` (branch was fast-forwarded to `integration/auth-sync-sdk58`; the PR will target that branch, not `main`).

## The change

Bar went from Overview · Expenses · Scan · Claims · Settings to Overview · Expenses · Scan · Finance · Claims. Five tabs is Android's maximum, so Settings became a pushed screen opened from a gear on Overview. All in `mobile/src/`:

- `app/finance/*` → `app/(tabs)/finance/*`. URLs are still `/finance` and `/finance/<section>` because `(tabs)` is a route group.
- `app/(tabs)/settings/index.tsx` → `app/settings.tsx`. URL is still `/settings`.
- `app/_layout.tsx`: root stack registers `settings` instead of `finance`, and adds `unstable_settings = { anchor: "(tabs)" }` so landing straight on `/settings` (the OAuth return in `app/auth/callback.tsx`) still has Back to the tabs.
- `app/(tabs)/(home)/_layout.tsx`: gear `headerRight` → `router.push("/settings")`.
- `native/dom-props.ts`: `TAB_ROOTS` swaps `/settings` for `/finance`. This set picks `router.dismissTo` vs `router.push` when a shared DOM screen navigates, and whether the native title follows the page heading.

## Done means

- Every route into Finance and Settings still lands on the right screen with sensible Back behaviour. That covers in-app links from the shared screens in `src/features/` (Dashboard "Open finance", Claims' Settings link, finance section links like `/finance/transactions?new`), the OAuth callback, and the bill/budget reminder notification routes in `mobile/src/core/reminders.ts` (find where `data.route` is consumed).
- Linking to `/finance` from another screen switches to the Finance tab, with no duplicate screen; `/finance/<section>` stays inside the Finance tab.
- The anchor setting doesn't disturb cold start or the `expenses/[id]` and `claims/[id]` deep links.
- Android tab and symbol names are valid for the installed expo-router (~58.0.26) and expo-symbols.
- `mobile` typecheck and lint stay at 0 errors, and the mobile Playwright specs pass or are reported as skipped, with the reason.
- Any real breakage is fixed with the smallest change. Leave the design, the other screens and unrelated code alone.

## You may, without asking

- Read anything, run typecheck/lint/Playwright, build the DOM bundle the mobile specs need (`FOLIO_DOM_BUNDLE`, see "Finance screen follow-up" in `docs/IPHONE_TESTING.md`), and edit files under `mobile/src` and `tests/` to fix breakage caused by this change. Playwright uses disposable temp databases and touches no hosted data.
- Run the iOS simulator. Start Metro from `mobile/` with `npx expo start --dev-client --port 8082`. A Debug build of this exact native code is already at `/private/tmp/claude-501/-Users-nurhanfawwaz-orca-workspaces-folio-expense-Independent-section-for-finance-in-mobile/0eed1d69-8a30-4945-9f3b-1cbc3c1ef18d/scratchpad/dd/Build/Products/Debug-iphonesimulator/Folio.app` (`com.nurhanfawwaz.folio`; Rust is unchanged, so no rebuild). Navigate with `xcrun simctl openurl booted expenseapp:///<path>` and check with `xcrun simctl io booted screenshot`. This Xcode has no Simulator.app GUI.

## Don't

- Erase the iPhone 17 Pro simulator, sign out, delete records, or start destructive sync. It holds the owner's real synced account (a backup of its data is in the same scratchpad, `sim-backup-expense-app`).
- Commit, push, touch the physical iPhone, or install from untrusted Homebrew taps.

## Not from this change

- The LogBox toast `Unable to find the 'DomWebView' view with tag '12'` on every launch also appears on unchanged `730e93d`.
- The floating grey gear on every screen is Expo's dev-menu button, and it covers header-right buttons. Hide it with `xcrun simctl spawn booted defaults write com.nurhanfawwaz.folio EXDevMenuShowFloatingActionButton -bool NO`, and delete the key when finished.

Already checked by deep link on the simulator: the new bar renders, `/finance` and `/finance/accounts` load in the Finance tab, `/settings` opens with Back, and the gear is only on Overview. Tapping wasn't possible there, so the gear tap, "Open finance" tap and tab-state retention are still unverified. Check them if you have a way to tap.

## Report

For each "Done means" item: pass, fail or not run, with evidence (command output, screenshot path or `file:line`). For each fix, give the cause and the diff. Then list what you couldn't check.
