#!/usr/bin/env bash
# Builds and tests all uniffi language bindings on macOS arm64.
# Each binding is built via its local-build.sh, then its test suite is run.
set -euo pipefail

cd ..

REPO="$(cd "$(dirname "$0")" && pwd)"
TARGET="aarch64-apple-darwin"
PASS=0
FAIL=0

header() { echo; echo "══════════════════════════════════════════"; echo "  $*"; echo "══════════════════════════════════════════"; }
ok()     { echo "  ✓ $*"; PASS=$((PASS + 1)); }
fail()   { echo "  ✗ $*"; FAIL=$((FAIL + 1)); }

# ── C# / dotnet ────────────────────────────────────────────────────────────────
header "C# (dotnet)"

echo "→ Building..."
if (cd "$REPO/dotnet" && bash local-build.sh -t "$TARGET"); then
  ok "dotnet build"
else
  fail "dotnet build"
fi

echo "→ Running tests..."
if dotnet test "$REPO/dotnet/tests/ChiaWalletSdkTests.csproj" --nologo -v minimal --filter "Category!=Integration"; then
  ok "dotnet tests"
else
  fail "dotnet tests"
fi

# ── Go ─────────────────────────────────────────────────────────────────────────
header "Go"

echo "→ Building..."
if (cd "$REPO/go" && bash local-build.sh -t "$TARGET"); then
  ok "go build"
else
  fail "go build"
fi

echo "→ Running tests..."
BINDINGS_DIR="$REPO/go/chia_wallet_sdk"
if (cd "$REPO/go" && \
    CGO_LDFLAGS="-L${BINDINGS_DIR} -lchia_wallet_sdk" \
    DYLD_LIBRARY_PATH="${BINDINGS_DIR}" \
    go test -v ./tests/...); then
  ok "go tests"
else
  fail "go tests"
fi

# ── C++ ────────────────────────────────────────────────────────────────────────
header "C++"

echo "→ Building..."
if (cd "$REPO/cpp" && bash local-build.sh -t "$TARGET"); then
  ok "cpp build"
else
  fail "cpp build"
fi

echo "→ Configuring CMake..."
CPP_BUILD="$REPO/cpp/tests/build"
rm -rf "$CPP_BUILD"
CMAKE_OK=0
if cmake -S "$REPO/cpp/tests" -B "$CPP_BUILD" -DCMAKE_BUILD_TYPE=Release; then
  ok "cmake configure"
  CMAKE_OK=1
else
  fail "cmake configure"
fi

if [[ $CMAKE_OK -eq 1 ]]; then
  echo "→ Compiling tests..."
  CMAKE_BUILD_OK=0
  if cmake --build "$CPP_BUILD"; then
    ok "cmake build"
    CMAKE_BUILD_OK=1
  else
    fail "cmake build"
  fi

  if [[ $CMAKE_BUILD_OK -eq 1 ]]; then
    echo "→ Running tests..."
    if ctest --test-dir "$CPP_BUILD" --output-on-failure --no-tests=error --label-exclude integration; then
      ok "cpp tests"
    else
      fail "cpp tests"
    fi
  fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────
header "Results"
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo

if [[ $FAIL -gt 0 ]]; then
  echo "Some steps FAILED."
  exit 1
else
  echo "All steps passed."
fi
