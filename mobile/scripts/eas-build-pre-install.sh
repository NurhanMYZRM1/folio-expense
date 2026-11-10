#!/bin/sh
# EAS Build hook (wired up as the "eas-build-pre-install" script in package.json).
#
# modules/folio-core vendors FolioFFI.xcframework, which is gitignored and built
# from ../src-tauri by scripts/build-rust-ios.sh. A fresh EAS checkout does not
# have it, so build it here, before `pod install` needs it.
#
# Runs in mobile/ on EAS's macOS worker. It also needs the full repo (not just
# mobile/), which EAS uploads: the whole git repository is archived.
set -eu

# build-rust-ios.sh needs rustup (it pins the `stable` toolchain and adds the iOS
# targets itself). Install it if the image does not ship it.
if ! command -v rustup >/dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  echo "eas-build-pre-install: installing rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path
fi
PATH="$HOME/.cargo/bin:$PATH"
export PATH

rustup --version
if [ "${EAS_BUILD_PLATFORM:-ios}" = "android" ]; then
  sh scripts/build-rust-android.sh
  test -f modules/folio-core/android/src/main/jniLibs/arm64-v8a/libfolio.so
else
  sh scripts/build-rust-ios.sh
  test -d modules/folio-core/ios/Frameworks/FolioFFI.xcframework
fi
