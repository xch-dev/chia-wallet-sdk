#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

usage() {
  echo "Usage: $0 [-v VERSION] [-t TARGET]"
  echo "  -v VERSION   NuGet package version (default: 0.0.4-local)"
  echo "  -t TARGET    Rust target triple (default: aarch64-apple-darwin)"
  exit 1
}

VERSION="0.0.4-local"
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

# Derive library filename and .NET RID from the target triple
case "$TARGET" in
  *-apple-*)
    LIB_EXT="dylib"
    case "$TARGET" in
      aarch64-*) DOTNET_RID="osx-arm64" ;;
      x86_64-*)  DOTNET_RID="osx-x64" ;;
      *)         echo "Unsupported macOS arch in target: $TARGET" >&2; exit 1 ;;
    esac
    ;;
  *-linux-*)
    LIB_EXT="so"
    case "$TARGET" in
      aarch64-*) DOTNET_RID="linux-arm64" ;;
      x86_64-*)  DOTNET_RID="linux-x64" ;;
      *)         echo "Unsupported Linux arch in target: $TARGET" >&2; exit 1 ;;
    esac
    ;;
  *-windows-*)
    LIB_EXT="dll"
    case "$TARGET" in
      aarch64-*) DOTNET_RID="win-arm64" ;;
      x86_64-*)  DOTNET_RID="win-x64" ;;
      *)         echo "Unsupported Windows arch in target: $TARGET" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unrecognized target triple: $TARGET" >&2; exit 1 ;;
esac

# Windows DLLs have no "lib" prefix; all other platforms do.
LIB_PREFIX=$( [ "$LIB_EXT" = "dll" ] && echo "" || echo "lib" )
LIB_NAME="${LIB_PREFIX}chia_wallet_sdk.$LIB_EXT"
LIB_PATH="$SCRIPT_DIR/../target/$TARGET/release-cs/$LIB_NAME"

echo "Building native library for $TARGET..."
cd ..
cargo build --profile release-cs -p chia-wallet-sdk-cs --target "$TARGET"
cd "$SCRIPT_DIR"

echo "Generating C# bindings..."
uniffi-bindgen-cs \
  --library \
  --out-dir "$SCRIPT_DIR/cs" \
  --config "$SCRIPT_DIR/uniffi.toml" \
  "$LIB_PATH"

echo "Staging native library..."
mkdir -p "$SCRIPT_DIR/cs/runtimes/$DOTNET_RID/native"
cp "$LIB_PATH" "$SCRIPT_DIR/cs/runtimes/$DOTNET_RID/native/"

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
