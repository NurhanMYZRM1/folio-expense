// Tells @expo/fingerprint what counts as "native code" for this app.
//
// runtimeVersion uses the "fingerprint" policy (app.json): an OTA update
// (`eas update`) is only delivered to binaries whose fingerprint matches the one
// the update was published with. By default the fingerprint covers the native
// folders, config plugins and native modules inside mobile/ (that includes the
// Swift in modules/folio-core/ios), but NOT the Rust core in ../src-tauri, which
// is compiled into FolioFFI.xcframework and shipped inside the binary.
//
// Without the entries below, a Rust-only change would keep the same fingerprint
// and a JS-only update could be delivered to an app with an older Rust core.
// Paths are relative to mobile/.
/** @type {import('@expo/fingerprint').Config} */
module.exports = {
  extraSources: [
    // The Rust crate that `npm run rust:ios` compiles into FolioFFI.xcframework.
    // (target/ is build output and is deliberately not listed.)
    { type: "dir", filePath: "../src-tauri/src", reasons: ["folio-rust-core"] },
    { type: "dir", filePath: "../src-tauri/migrations", reasons: ["folio-rust-core"] },
    { type: "file", filePath: "../src-tauri/Cargo.toml", reasons: ["folio-rust-core"] },
    { type: "file", filePath: "../src-tauri/Cargo.lock", reasons: ["folio-rust-core"] },
  ],
};
