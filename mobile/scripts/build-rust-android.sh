#!/bin/sh
# Build both supported 64-bit ABIs; override FOLIO_ANDROID_TARGETS for a local build.
set -eu
cd "$(dirname "$0")/../../src-tauri"
export PATH="$HOME/.cargo/bin:$PATH"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.1.12297006}"
command -v cargo-ndk >/dev/null 2>&1 || cargo install cargo-ndk --locked
for target in ${FOLIO_ANDROID_TARGETS:-aarch64-linux-android x86_64-linux-android}; do
  rustup target add --toolchain stable "$target"
  cargo +stable ndk -t "$target" --platform 26 -o ../mobile/modules/folio-core/android/src/main/jniLibs build --release --lib --no-default-features --features mobile
done
