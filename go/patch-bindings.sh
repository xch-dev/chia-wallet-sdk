#!/usr/bin/env bash
# Patches known uniffi-bindgen-go naming collisions in the generated Go source.
# Shared by local-build.sh and .github/workflows/go.yml; local-build.ps1 mirrors these
# rules for Windows, so keep the two in sync.
#
# uniffi-bindgen-go emits a named constructor `Foo.bar()` as `func FooBar(`, which collides
# with a separately generated `FooBar` type (OptionType.cat() vs OptionTypeCat,
# DelegatedPuzzle.oracle() vs DelegatedPuzzleOracle). Rename those constructors.
set -euo pipefail

GENERATED="${1:?usage: patch-bindings.sh <path/to/chia_wallet_sdk.go>}"

sed -i.bak \
  -e 's/^func OptionTypeCat(/func NewOptionTypeFromCat(/' \
  -e 's/^func OptionTypeNft(/func NewOptionTypeFromNft(/' \
  -e 's/^func OptionTypeRevocableCat(/func NewOptionTypeFromRevocableCat(/' \
  -e 's/^func OptionTypeXch(/func NewOptionTypeFromXch(/' \
  -e 's/^func DelegatedPuzzleOracle(/func NewDelegatedPuzzleFromOracle(/' \
  "$GENERATED"
rm -f "$GENERATED.bak"

# Fail loudly on any remaining func/type name collision (a new colliding factory, or a
# generator change that stopped one of the rules above from matching).
collisions=$(comm -12 \
  <(grep -oE '^func [A-Z][A-Za-z0-9]*\(' "$GENERATED" | sed -e 's/^func //' -e 's/($//' | sort -u) \
  <(grep -oE '^type [A-Z][A-Za-z0-9]* ' "$GENERATED" | sed -e 's/^type //' -e 's/ $//' | sort -u))
if [ -n "$collisions" ]; then
  echo "ERROR: generated Go has functions that collide with type names:" >&2
  echo "$collisions" >&2
  echo "Add a rename for each to go/patch-bindings.sh (and go/local-build.ps1)." >&2
  exit 1
fi
