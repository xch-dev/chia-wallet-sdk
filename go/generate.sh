#!/usr/bin/env bash
# Builds the Rust library as a static library and generates the Go bindings for it.
#
# Usage: ./generate.sh [rust-target]
#
# The target defaults to the host. Requires `uniffi-bindgen-go`, which can be installed with:
#   cargo install uniffi-bindgen-go --git https://github.com/NordSecurity/uniffi-bindgen-go --tag v0.7.1+v0.31.0
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

case "$TARGET" in
  aarch64-apple-darwin) PLATFORM=darwin_arm64 ;;
  x86_64-apple-darwin) PLATFORM=darwin_amd64 ;;
  aarch64-unknown-linux-gnu) PLATFORM=linux_arm64 ;;
  x86_64-unknown-linux-gnu) PLATFORM=linux_amd64 ;;
  x86_64-pc-windows-gnu) PLATFORM=windows_amd64 ;;
  *)
    echo "Unsupported target: $TARGET" >&2
    exit 1
    ;;
esac

# Match the minimum macOS version supported by Go, so that linking doesn't warn
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

cargo build --profile release-uniffi -p chia-wallet-sdk-uniffi --target "$TARGET"

LIB="target/$TARGET/release-uniffi/libchia_wallet_sdk.a"
OUT=go/chiawalletsdk

uniffi-bindgen-go --library "$LIB" --out-dir go
mkdir -p "$OUT/lib/$PLATFORM"
cp "$LIB" "$OUT/lib/$PLATFORM/"

echo "Generated the Go bindings and copied the $PLATFORM library into $OUT"
