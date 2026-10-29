#!/bin/sh
# Builds the Rust core (../src-tauri, `mobile` feature) for iOS devices and the
# simulator, and packages it as the XCFramework the FolioCore pod vendors.
set -eu
cd "$(dirname "$0")/../../src-tauri"
# rustup's toolchain, not a system/Homebrew Rust that has no iOS standard library.
rustup=$(command -v rustup || echo "$(brew --prefix rustup)/bin/rustup")
cargo=$("$rustup" which --toolchain stable cargo)
export RUSTC="$("$rustup" which --toolchain stable rustc)"
export IPHONEOS_DEPLOYMENT_TARGET=16.4
# EAS builds the simulator app for arm64 *and* x86_64, so its simulator slice must hold
# both ("library 'folio' not found" otherwise). A local build only needs this Mac's
# architecture, so the Intel simulator target is opt-in: FOLIO_IOS_SIM_X86=1 (the EAS
# pre-install hook sets it).
targets="aarch64-apple-ios aarch64-apple-ios-sim"
if [ "${FOLIO_IOS_SIM_X86:-}" = "1" ]; then
  targets="$targets x86_64-apple-ios"
fi
for target in $targets; do
  "$rustup" target add --toolchain stable "$target" >/dev/null
  "$cargo" build --release --lib --no-default-features --features mobile --target "$target"
done
sim=target/aarch64-apple-ios-sim/release/libfolio.a
if [ "${FOLIO_IOS_SIM_X86:-}" = "1" ]; then
  mkdir -p target/ios-sim-universal/release
  lipo -create \
    target/aarch64-apple-ios-sim/release/libfolio.a \
    target/x86_64-apple-ios/release/libfolio.a \
    -output target/ios-sim-universal/release/libfolio.a
  sim=target/ios-sim-universal/release/libfolio.a
fi
out=../mobile/modules/folio-core/ios/Frameworks/FolioFFI.xcframework
rm -rf "$out"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libfolio.a \
  -library "$sim" \
  -output "$out"
