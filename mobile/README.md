# Folio mobile

Expo (SDK 58) app that reuses the desktop React code from `../src` and the Rust core in
`../src-tauri` through the `modules/folio-core` native module. See `AGENTS.md` for the
day-to-day commands and `migration-progress.md` for the migration status.

## Run on an iPhone

Folio requires its own development build because `modules/folio-core` contains
custom Rust/Swift code. Expo Go cannot load that module.

With a paired iPhone, Developer Mode enabled, and an Apple development signing
identity configured on the Mac:

```bash
cd mobile
npm ci
npm run ios -- --device
```

This compiles the Rust core, builds and installs the iOS development client, and
starts Metro on port 8082. Keep the Mac and iPhone on the same network. After the
first install, use `npm start` and open the development server in Folio. Rebuild
the development client after an Expo SDK or native module change.

## Build & release

Builds and over-the-air (OTA) updates run on EAS under the Expo account
[`squadralennox`](https://expo.dev/accounts/squadralennox) (`expo.owner` in `app.json`).
Run all `eas` commands from this `mobile/` folder.

The app is linked to EAS project `5010a68a-a7af-4b99-93c9-900a0e0292ee`.
`extra.eas.projectId` and `updates.url` are configured in `app.json`.

### Sign in

```bash
npx eas-cli@latest login            # your Expo account (squadralennox)
npx eas-cli@latest whoami           # verify access to the squadralennox account
```

### Everyday commands

```bash
npx eas-cli@latest build --profile development --platform ios   # dev client (internal)
npx eas-cli@latest build --profile preview --platform ios       # internal test build
npx eas-cli@latest build --profile production --platform ios    # store build, build number auto-increments
npx eas-cli@latest update --channel preview --message "what changed"
```

Internal-distribution iOS builds (`development`, `preview`) install on registered devices only:
run `npx eas-cli@latest device:create` once per device, then build.

### Profiles and channels

| Build profile | Distribution | Channel | Notes |
|---|---|---|---|
| `development` | internal | `development` | `developmentClient: true` |
| `preview` | internal | `preview` | |
| `production` | store | `production` | `autoIncrement: true` |

`cli.appVersionSource` is `remote`: EAS owns the build number (and bumps it on `production`),
so do not edit it by hand. `version` in `app.json` is still the marketing version.

### OTA updates and native code

`runtimeVersion` uses the `fingerprint` policy. EAS hashes everything that makes up the native
binary; an update is only delivered to builds with the same hash, so a JS-only update can never
reach a binary with different native code. Two files tune what is hashed:

- `fingerprint.config.js` adds the Rust core (`../src-tauri` sources, migrations, `Cargo.toml`,
  `Cargo.lock`). It is compiled into the binary but lives outside this folder, so without it a
  Rust-only change would not change the hash.
- `.fingerprintignore` leaves out `modules/folio-core/ios/Frameworks/`, the compiled
  `FolioFFI.xcframework`. It is hashed through the Rust sources instead, because compiled output
  differs between your Mac and EAS and would otherwise make `eas update` and the build disagree.

`eas.json` is hashed too. To see the current hash: `npx fingerprint fingerprint:generate .`.
If you change Swift, a native dependency, a config plugin, the Rust core, or `eas.json`, ship a
new build (`eas build`) rather than only `eas update`. Updates published for the old hash simply do not
reach the new binary.

### Rust core on EAS (iOS)

`modules/folio-core/ios/Frameworks/FolioFFI.xcframework` is git-ignored and normally built by
`npm run rust:ios` (`scripts/build-rust-ios.sh`: rustup + the `aarch64-apple-ios` and
`aarch64-apple-ios-sim` targets, compiling `../src-tauri` with `--features mobile`).
A fresh EAS checkout does not contain it, so the `eas-build-pre-install` script in
`package.json` (`scripts/eas-build-pre-install.sh`) runs first on the macOS worker: it installs
rustup if the image lacks it, runs `build-rust-ios.sh`, and fails the build if the xcframework is
missing. It skips itself for Android builds. Expect a long, uncached first compile (two release
targets); to keep an eye on it, read the "Run eas-build-pre-install" step of the build log.

If that hook ever becomes too slow or fragile, the fallback is to build the xcframework in CI,
upload the zip to storage outside git (for example a GitHub release), and have the hook
download and unzip it into `modules/folio-core/ios/Frameworks/` instead of compiling. Keep the
fingerprint settings above as they are in that case.

### Why the repo root matters (monorepo)

The app imports `../src` and `../src-tauri` from the repository root. EAS CLI archives the
**whole git repository** (it clones the repo root, then runs the build in the folder it was
started from, here `mobile/`), not just `mobile/`. So no special layout is needed: keep
`eas.json` in `mobile/`, run `eas` from `mobile/`, and keep the files the build needs
**committed** (git-ignored files are not uploaded, which is why the xcframework is built on
EAS). The repo root is not an npm workspace; the lockfile that applies is `mobile/package-lock.json` (nothing at the root is a workspace).

### Android

Not supported yet: `modules/folio-core` has no Android side (see `migration-progress.md`).
Only run iOS builds.

### CI (optional)

To build or update from CI instead of a laptop, create an Expo access token at
https://expo.dev/accounts/squadralennox/settings/access-tokens and store it as the
`EXPO_TOKEN` secret in the CI system. Never commit the token. EAS CLI reads `EXPO_TOKEN`
automatically.
