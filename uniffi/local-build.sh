#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VERSION="${1:-0.0.4-local}"

echo "Building native library for aarch64-apple-darwin..."
cd ..
cargo build --release -p chia-wallet-sdk-cs --target aarch64-apple-darwin
cd "$SCRIPT_DIR"

echo "Generating C# bindings..."
uniffi-bindgen-cs \
  --library \
  --out-dir "$SCRIPT_DIR/cs" \
  --config "$SCRIPT_DIR/uniffi.toml" \
  "$SCRIPT_DIR/../target/aarch64-apple-darwin/release/libchia_wallet_sdk.dylib"

echo "Staging native library..."
mkdir -p "$SCRIPT_DIR/cs/runtimes/osx-arm64/native"
cp "$SCRIPT_DIR/../target/aarch64-apple-darwin/release/libchia_wallet_sdk.dylib" \
   "$SCRIPT_DIR/cs/runtimes/osx-arm64/native/"

echo "Packing NuGet (version: $VERSION)..."
dotnet pack "$SCRIPT_DIR/cs/ChiaWalletSdk.csproj" \
  -c Release \
  -o "$SCRIPT_DIR/nuget-out" \
  -p:Version="$VERSION"

echo ""
echo "Package ready: $SCRIPT_DIR/nuget-out/ChiaWalletSdk.$VERSION.nupkg"
echo ""
echo "To register the local feed (once):"
echo "  dotnet nuget add source $SCRIPT_DIR/nuget-out --name chia-local"
echo ""
echo "To add to a project:"
echo "  dotnet add package ChiaWalletSdk --version $VERSION"
