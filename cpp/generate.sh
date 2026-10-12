#!/usr/bin/env bash
# Builds the Rust library (with async methods exposed as blocking calls) and generates the C++
# bindings for it.
#
# Usage: ./generate.sh [rust-target]
#
# The target defaults to the host. Requires `uniffi-bindgen-cpp`, which can be installed with:
#   cargo install uniffi-bindgen-cpp --git https://github.com/NordSecurity/uniffi-bindgen-cpp --tag v0.10.0+v0.31.0
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

# uniffi-bindgen-cpp doesn't support async functions
cargo build --profile release-uniffi -p chia-wallet-sdk-uniffi --features blocking --target "$TARGET"

LIB_DIR="target/$TARGET/release-uniffi"
OUT=cpp/generated

case "$TARGET" in
  *-windows-*) LIB="$LIB_DIR/chia_wallet_sdk.dll" ;;
  *-apple-*) LIB="$LIB_DIR/libchia_wallet_sdk.dylib" ;;
  *) LIB="$LIB_DIR/libchia_wallet_sdk.so" ;;
esac

rm -rf "$OUT"
mkdir -p "$OUT/lib"
uniffi-bindgen-cpp --library "$LIB" --out-dir "$OUT"
"$(command -v python3 || command -v python)" cpp/fix-bindings.py "$OUT/chiawalletsdk.hpp" "$OUT/chiawalletsdk.cpp"
cp "$LIB_DIR"/*chia_wallet_sdk* "$OUT/lib/"
rm -f "$OUT/lib/"*.d

echo "Generated the C++ bindings and copied the libraries into $OUT"
