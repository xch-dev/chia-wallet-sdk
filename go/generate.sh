#!/usr/bin/env bash
# Builds the Rust library as a static library and generates the Go bindings for it.
#
# Usage: ./generate.sh [rust-target]
#
# The target defaults to the host. Requires `uniffi-bindgen-go`, which can be installed with:
#   cargo install uniffi-bindgen-go --git https://github.com/NordSecurity/uniffi-bindgen-go --tag v0.7.1+v0.31.0
#
# Building for Android requires the Android NDK, found through `ANDROID_NDK_HOME`. Building for
# Windows on arm64 requires LLVM MinGW (https://github.com/mstorsjo/llvm-mingw) on the `PATH`.
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

case "$TARGET" in
  aarch64-apple-darwin) PLATFORM=darwin_arm64 ;;
  x86_64-apple-darwin) PLATFORM=darwin_amd64 ;;
  aarch64-unknown-linux-gnu) PLATFORM=linux_arm64 ;;
  x86_64-unknown-linux-gnu) PLATFORM=linux_amd64 ;;
  x86_64-pc-windows-gnu) PLATFORM=windows_amd64 ;;
  aarch64-pc-windows-gnullvm) PLATFORM=windows_arm64 ;;
  aarch64-linux-android) PLATFORM=android_arm64 ;;
  *)
    echo "Unsupported target: $TARGET" >&2
    exit 1
    ;;
esac

# Match the minimum macOS version supported by Go, so that linking doesn't warn
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

# Android builds use the NDK's clang, both for the C dependencies and for linking
if [ "$PLATFORM" = android_arm64 ]; then
  NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
  if [ -z "$NDK" ]; then
    echo "Set ANDROID_NDK_HOME to the Android NDK to build for $TARGET" >&2
    exit 1
  fi
  NDK_BIN=$(echo "$NDK"/toolchains/llvm/prebuilt/*/bin)
  export CC_aarch64_linux_android="$NDK_BIN/aarch64-linux-android${ANDROID_API:-21}-clang"
  export CXX_aarch64_linux_android="$NDK_BIN/aarch64-linux-android${ANDROID_API:-21}-clang++"
  export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
  export RANLIB_aarch64_linux_android="$NDK_BIN/llvm-ranlib"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"

  # The NDK's CMake toolchain file needs the ABI, which can't be passed through the cmake crate
  # (used to build zlib-ng), so it's set in a toolchain file that wraps the NDK's
  TOOLCHAIN_FILE="$PWD/target/$TARGET/android.toolchain.cmake"
  mkdir -p "$(dirname "$TOOLCHAIN_FILE")"
  cat > "$TOOLCHAIN_FILE" <<EOF
set(ANDROID_ABI arm64-v8a)
set(ANDROID_PLATFORM android-${ANDROID_API:-21})
include("$NDK/build/cmake/android.toolchain.cmake")
EOF
  export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android="$TOOLCHAIN_FILE"
fi

cargo build --profile release-uniffi -p chia-wallet-sdk-uniffi --target "$TARGET"

LIB="target/$TARGET/release-uniffi/libchia_wallet_sdk.a"
OUT=go/chiawalletsdk

# uniffi-bindgen-go can't read the metadata from the gnullvm static library, but the bindings
# are the same for every target, so they're generated from the DLL built alongside it
METADATA_LIB="$LIB"
if [ "$TARGET" = aarch64-pc-windows-gnullvm ]; then
  METADATA_LIB="target/$TARGET/release-uniffi/chia_wallet_sdk.dll"
fi

uniffi-bindgen-go --library "$METADATA_LIB" --out-dir go
mkdir -p "$OUT/lib/$PLATFORM"
cp "$LIB" "$OUT/lib/$PLATFORM/"

echo "Generated the Go bindings and copied the $PLATFORM library into $OUT"
