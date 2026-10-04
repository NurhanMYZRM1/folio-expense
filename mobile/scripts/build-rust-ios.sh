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
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
  "$rustup" target add --toolchain stable "$target" >/dev/null
  "$cargo" build --release --lib --no-default-features --features mobile --target "$target"
done
out=../mobile/modules/folio-core/ios/Frameworks/FolioFFI.xcframework
rm -rf "$out"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libfolio.a \
  -library target/aarch64-apple-ios-sim/release/libfolio.a \
  -output "$out"
