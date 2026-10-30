#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# uniffi-bindgen-go v0.7.1 targets uniffi 0.31.0, which is exactly the version
# the workspace is pinned to (uniffi = "=0.31.0").
# See https://github.com/NordSecurity/uniffi-bindgen-go/releases for available tags.
UNIFFI_BINDGEN_GO_TAG="v0.7.1+v0.31.0"

usage() {
  echo "Usage: $0 [-v VERSION] [-t TARGET]"
  echo "  -v VERSION   Go module version tag (default: v0.0.1-local)"
  echo "  -t TARGET    Rust target triple (default: aarch64-apple-darwin)"
  exit 1
}

VERSION="v0.0.1-local"
TARGET="aarch64-apple-darwin"

while getopts ":v:t:h" opt; do
  case $opt in
    v) VERSION="$OPTARG" ;;
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
# Cargo builds chia_wallet_sdk_go; it is staged as chia_wallet_sdk for consumers.
LIB_NAME="${LIB_PREFIX}chia_wallet_sdk.$LIB_EXT"
BUILT_NAME="${LIB_PREFIX}chia_wallet_sdk_go.$LIB_EXT"
LIB_PATH="$SCRIPT_DIR/../target/$TARGET/release-go/$BUILT_NAME"

echo "Building native library for $TARGET..."
cd "$SCRIPT_DIR/.."
cargo build --profile release-go -p chia-wallet-sdk-go --target "$TARGET"
cd "$SCRIPT_DIR"

if ! command -v uniffi-bindgen-go &>/dev/null; then
  echo "Installing uniffi-bindgen-go $UNIFFI_BINDGEN_GO_TAG..."
  cargo install uniffi-bindgen-go \
    --git https://github.com/NordSecurity/uniffi-bindgen-go \
    --tag "$UNIFFI_BINDGEN_GO_TAG"
fi

echo "Generating Go bindings..."
uniffi-bindgen-go \
  --library "$LIB_PATH" \
  --out-dir "$SCRIPT_DIR"

echo "Patching naming collisions..."
"$SCRIPT_DIR/patch-bindings.sh" "$SCRIPT_DIR/chia_wallet_sdk/chia_wallet_sdk.go"

echo "Staging native library..."
mkdir -p "$SCRIPT_DIR/chia_wallet_sdk"
cp "$LIB_PATH" "$SCRIPT_DIR/chia_wallet_sdk/$LIB_NAME"

echo ""
echo "Go bindings generated in: $SCRIPT_DIR/chia_wallet_sdk/"
echo ""
echo "To build a Go project using these bindings, the linker must find the native"
echo "library. Set CGO_LDFLAGS when building from outside this directory:"
echo "  CGO_LDFLAGS=\"-L\$(realpath $SCRIPT_DIR/chia_wallet_sdk) -lchia_wallet_sdk\" go build ./..."
echo ""
echo "Or install the library system-wide (macOS example):"
echo "  sudo cp $SCRIPT_DIR/chia_wallet_sdk/$LIB_NAME /usr/local/lib/"
