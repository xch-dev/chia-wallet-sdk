#!/usr/bin/env bash
# Patches known uniffi-bindgen-cpp code-generation defects so the output compiles.
# Shared by local-build.sh and .github/workflows/cpp.yml; local-build.ps1 mirrors these
# rules for Windows, so keep the two in sync.
#
#   1. Clvm methods `bool`/`int` and the CatalogRegistry/XchandlesRegistry `register`
#      methods are emitted with their (reserved) C++ keyword names; rename them to
#      `bool_`/`int_`/`register_`.
#   2. The forward declarations for VdfInfo/VdfProof are emitted with all-caps acronyms
#      (`VDFInfo`/`VDFProof`) while every other reference uses `VdfInfo`/`VdfProof`.
set -euo pipefail

OUT_DIR="${1:?usage: patch-bindings.sh <dir containing chia_wallet_sdk.hpp/.cpp>}"
HPP="$OUT_DIR/chia_wallet_sdk.hpp"
CPP="$OUT_DIR/chia_wallet_sdk.cpp"

sed -i.bak \
  -e 's/std::shared_ptr<Program> bool(/std::shared_ptr<Program> bool_(/' \
  -e 's/std::shared_ptr<Program> int(/std::shared_ptr<Program> int_(/' \
  -e 's/> register(/> register_(/' \
  -e 's/struct VDFInfo;/struct VdfInfo;/' \
  -e 's/struct VDFProof;/struct VdfProof;/' \
  "$HPP"
sed -i.bak \
  -e 's/Clvm::bool(/Clvm::bool_(/' \
  -e 's/Clvm::int(/Clvm::int_(/' \
  -e 's/Registry::register(/Registry::register_(/' \
  "$CPP"
rm -f "$HPP.bak" "$CPP.bak"

# Fail loudly on any remaining keyword-named method or acronym forward declaration (a new
# colliding method, or a generator change that stopped one of the rules above matching).
KEYWORDS='bool|int|register|char|short|long|float|double|void|auto|class|struct|union|enum|delete|new|default|operator|template|typename|this|throw|try|catch|switch|case|goto|return|const|static|virtual|explicit|friend|inline|mutable|private|protected|public|signed|unsigned|sizeof|typedef|using|namespace|export|extern|volatile|true|false|nullptr|and|or|not|xor'
problems=$({
  grep -nE "[A-Za-z0-9_>&*] ($KEYWORDS)\(" "$HPP"
  grep -nE "[A-Z][A-Za-z0-9]*::($KEYWORDS)\(" "$CPP"
  grep -nE '^struct VDF[A-Za-z]*;' "$HPP"
} || true)
if [ -n "$problems" ]; then
  echo "ERROR: generated C++ still has keyword-named methods or acronym forward declarations:" >&2
  echo "$problems" >&2
  echo "Add a rule for each to cpp/patch-bindings.sh (and cpp/local-build.ps1)." >&2
  exit 1
fi
