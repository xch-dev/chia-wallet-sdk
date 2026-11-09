# Finds the Android NDK's clang, for building the Rust library and linking Go programs with it.
# Sourced by `generate.sh` and by CI, so both use the same compiler and API level.
#
# Reads `ANDROID_NDK_HOME` (or `ANDROID_NDK_ROOT`) and `ANDROID_API`, which defaults to 21. Sets
# `NDK`, `NDK_BIN`, `ANDROID_API`, `ANDROID_CC` and `ANDROID_CXX`.

NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [ -z "$NDK" ]; then
  echo "Set ANDROID_NDK_HOME to the Android NDK to build for Android" >&2
  return 1
fi

NDK_BIN=
for dir in "$NDK"/toolchains/llvm/prebuilt/*/bin; do
  [ -d "$dir" ] || continue
  if [ -n "$NDK_BIN" ]; then
    echo "Found more than one prebuilt toolchain in $NDK/toolchains/llvm/prebuilt" >&2
    return 1
  fi
  NDK_BIN="$dir"
done
if [ -z "$NDK_BIN" ]; then
  echo "Found no prebuilt toolchain in $NDK/toolchains/llvm/prebuilt" >&2
  return 1
fi

ANDROID_API="${ANDROID_API:-21}"
ANDROID_CC="$NDK_BIN/aarch64-linux-android$ANDROID_API-clang"
ANDROID_CXX="$NDK_BIN/aarch64-linux-android$ANDROID_API-clang++"
if [ ! -x "$ANDROID_CC" ]; then
  echo "The NDK has no compiler for Android API level $ANDROID_API: $ANDROID_CC" >&2
  return 1
fi
