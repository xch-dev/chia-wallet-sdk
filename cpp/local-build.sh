#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# uniffi-bindgen-cpp v0.10.0 targets uniffi 0.31.0, which is exactly the version
# the workspace is pinned to (uniffi = "=0.31.0").
# See https://github.com/NordSecurity/uniffi-bindgen-cpp/tags for available tags.
UNIFFI_BINDGEN_CPP_TAG="v0.10.0+v0.31.0"

usage() {
  echo "Usage: $0 [-t TARGET]"
  echo "  -t TARGET    Rust target triple (default: aarch64-apple-darwin)"
  exit 1
}

TARGET="aarch64-apple-darwin"

while getopts ":t:h" opt; do
  case $opt in
    t) TARGET="$OPTARG" ;;
    h) usage ;;
    :) echo "Option -$OPTARG requires an argument." >&2; usage ;;
    \?) echo "Unknown option: -$OPTARG" >&2; usage ;;
  esac
done

# Derive library filename from the target triple
case "$TARGET" in
  *-apple-*)   LIB_EXT="dylib" ;;
  *-linux-*)   LIB_EXT="so"    ;;
  *-windows-*) LIB_EXT="dll"   ;;
  *) echo "Unrecognized target triple: $TARGET" >&2; exit 1 ;;
esac

LIB_PREFIX=$( [ "$LIB_EXT" = "dll" ] && echo "" || echo "lib" )
# Cargo builds chia_wallet_sdk_cpp; it is staged as chia_wallet_sdk for consumers.
LIB_NAME="${LIB_PREFIX}chia_wallet_sdk.$LIB_EXT"
BUILT_NAME="${LIB_PREFIX}chia_wallet_sdk_cpp.$LIB_EXT"
LIB_PATH="$SCRIPT_DIR/../target/$TARGET/release-cpp/$BUILT_NAME"

echo "Building native library for $TARGET..."
cd "$SCRIPT_DIR/.."
cargo build --profile release-cpp -p chia-wallet-sdk-cpp --target "$TARGET"
cd "$SCRIPT_DIR"

if ! command -v uniffi-bindgen-cpp &>/dev/null; then
  echo "Installing uniffi-bindgen-cpp $UNIFFI_BINDGEN_CPP_TAG..."
  cargo install uniffi-bindgen-cpp \
    --git https://github.com/NordSecurity/uniffi-bindgen-cpp \
    --tag "$UNIFFI_BINDGEN_CPP_TAG"
fi

echo "Generating C++ bindings..."
mkdir -p "$SCRIPT_DIR/chia_wallet_sdk"
uniffi-bindgen-cpp \
  --library "$LIB_PATH" \
  --out-dir "$SCRIPT_DIR/chia_wallet_sdk"

echo "Patching generated bindings..."
"$SCRIPT_DIR/patch-bindings.sh" "$SCRIPT_DIR/chia_wallet_sdk"

echo "Staging native library..."
cp "$LIB_PATH" "$SCRIPT_DIR/chia_wallet_sdk/$LIB_NAME"

echo ""
echo "C++ bindings generated in: $SCRIPT_DIR/chia_wallet_sdk/"
echo "  chia_wallet_sdk.hpp              - public C++ header"
echo "  chia_wallet_sdk.cpp             - implementation (compile and link into your app)"
echo "  chia_wallet_sdk_scaffolding.hpp - FFI scaffolding declarations"
echo "  $LIB_NAME      - native shared library"
echo ""
echo "Build a consumer with C++20, compiling chia_wallet_sdk.cpp and linking the library:"
echo "  c++ -std=c++20 -I$SCRIPT_DIR/chia_wallet_sdk your_app.cpp \\"
echo "      $SCRIPT_DIR/chia_wallet_sdk/chia_wallet_sdk.cpp \\"
echo "      -L$SCRIPT_DIR/chia_wallet_sdk -lchia_wallet_sdk -o your_app"
