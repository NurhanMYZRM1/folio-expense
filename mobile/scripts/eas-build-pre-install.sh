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

# The native module is iOS-only; an Android build runs this hook on Linux.
if [ "${EAS_BUILD_PLATFORM:-ios}" != "ios" ]; then
  echo "eas-build-pre-install: platform is ${EAS_BUILD_PLATFORM}, skipping the Rust iOS build."
  exit 0
fi

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
sh scripts/build-rust-ios.sh

test -d modules/folio-core/ios/Frameworks/FolioFFI.xcframework
echo "eas-build-pre-install: FolioFFI.xcframework built."
