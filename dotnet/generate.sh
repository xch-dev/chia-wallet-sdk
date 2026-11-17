#!/usr/bin/env bash
# Builds the Rust library as a shared library and generates the C# bindings for it.
#
# Usage: ./generate.sh [rust-target]
#
# The target defaults to the host. Set SKIP_BUILD=1 to use a library that's already in
# target/<rust-target>/release-uniffi instead of building it.
#
# Requires `uniffi-bindgen-cs`, which can be installed with:
#   cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.11.1+v0.31.0
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

case "$TARGET" in
  aarch64-apple-darwin) RID=osx-arm64 LIB=libchia_wallet_sdk.dylib ;;
  x86_64-apple-darwin) RID=osx-x64 LIB=libchia_wallet_sdk.dylib ;;
  aarch64-unknown-linux-gnu) RID=linux-arm64 LIB=libchia_wallet_sdk.so ;;
  x86_64-unknown-linux-gnu) RID=linux-x64 LIB=libchia_wallet_sdk.so ;;
  aarch64-pc-windows-msvc) RID=win-arm64 LIB=chia_wallet_sdk.dll ;;
  x86_64-pc-windows-msvc) RID=win-x64 LIB=chia_wallet_sdk.dll ;;
  *)
    echo "Unsupported target: $TARGET" >&2
    exit 1
    ;;
esac

# CI builds the Linux libraries once and shares them between Go and .NET, so it sets SKIP_BUILD
# and puts the library where the build would have
if [ "${SKIP_BUILD:-}" != 1 ]; then
  cargo build --profile release-uniffi -p chia-wallet-sdk-uniffi --target "$TARGET"
fi

LIB="target/$TARGET/release-uniffi/$LIB"
OUT=dotnet/ChiaWalletSdk

uniffi-bindgen-cs --library "$LIB" --out-dir "$OUT" --no-format
"$(command -v python3 || command -v python)" dotnet/fix-bindings.py "$OUT/chiawalletsdk.cs"
mkdir -p "$OUT/runtimes/$RID/native"
cp "$LIB" "$OUT/runtimes/$RID/native/"

echo "Generated the C# bindings and copied the $RID library into $OUT"
