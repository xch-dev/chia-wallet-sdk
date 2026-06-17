# chia-wallet-sdk C++ Bindings

C++ bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), generated via [UniFFI](https://mozilla.github.io/uniffi-rs/) and [uniffi-bindgen-cpp](https://github.com/NordSecurity/uniffi-bindgen-cpp).

The Rust library exposes the full SDK surface — BLS keys, addresses, CLVM, coins, conditions, offers, puzzles, RPC, and more — as a native shared library that C++ consumes through the generated UniFFI scaffolding layer.

---

## Prerequisites

- Rust toolchain (1.81+) — [rustup.rs](https://rustup.rs)
- A C++20 compiler (Clang 14+, GCC 11+, or MSVC 2022)
- `uniffi-bindgen-cpp` — installed once per machine (see below)

---

## Building

The `local-build.sh` script handles all steps: compiling the native library, installing `uniffi-bindgen-cpp` if needed, generating the C++ source, and staging the shared library.

```bash
cd cpp
./local-build.sh                              # defaults: aarch64-apple-darwin
./local-build.sh -t x86_64-apple-darwin
./local-build.sh -t x86_64-unknown-linux-gnu
./local-build.sh -t x86_64-pc-windows-msvc    # PowerShell: .\local-build.ps1
```

After running, the `cpp/chia_wallet_sdk/` directory contains:

| File | Description |
| ---- | ----------- |
| `chia_wallet_sdk.hpp` | Public C++ header — include this from your code |
| `chia_wallet_sdk.cpp` | Implementation — compile and link into your binary |
| `chia_wallet_sdk_scaffolding.hpp` | FFI scaffolding declarations |
| `libchia_wallet_sdk.dylib` / `.so` / `.dll` | Native shared library |

These files are git-ignored and must be regenerated per platform.

---

## Step-by-step (manual)

### 1 — Build the native library

```bash
# From the repo root
cargo build --profile release-cpp -p chia-wallet-sdk-cpp --target aarch64-apple-darwin
```

Output by platform:

| Platform | File |
| -------- | ---- |
| macOS | `target/<target>/release-cpp/libchia_wallet_sdk.dylib` |
| Linux | `target/<target>/release-cpp/libchia_wallet_sdk.so` |
| Windows | `target/<target>/release-cpp/chia_wallet_sdk.dll` |

The `release-cpp` profile inherits from `release` but sets `strip = "none"`, which is
required so that `uniffi-bindgen-cpp` can read the `UNIFFI_META_*` symbols that the
standard `--release` profile strips.

### 2 — Install `uniffi-bindgen-cpp`

This tool generates the C++ source from the compiled library. Install it once; the
version **must match** the `uniffi` crate version used here (`0.29.4`).

```bash
cargo install uniffi-bindgen-cpp \
  --git https://github.com/NordSecurity/uniffi-bindgen-cpp \
  --tag v0.8.1+v0.29.4
```

> **Version note:** the workspace pins `uniffi = "=0.29.4"`, and `uniffi-bindgen-cpp`
> `v0.8.1+v0.29.4` targets exactly that version — no patch-version mismatch (unlike the
> Go backend, which tolerates a benign 0.29.4→0.29.5 skew).

### 3 — Generate the C++ source

```bash
uniffi-bindgen-cpp \
  --library target/aarch64-apple-darwin/release-cpp/libchia_wallet_sdk.dylib \
  --out-dir cpp/chia_wallet_sdk
```

This writes `chia_wallet_sdk.hpp`, `chia_wallet_sdk.cpp`, and
`chia_wallet_sdk_scaffolding.hpp` into `cpp/chia_wallet_sdk/`.

> **Note:** `uniffi-bindgen-cpp` does not support `--config` in `--library` mode, so the
> namespace is taken from the UniFFI component name (`chia_wallet_sdk`). Everything lives
> in the `chia_wallet_sdk` C++ namespace.

---

## Using in a C++ Project

Compile the generated `chia_wallet_sdk.cpp` alongside your code (C++20) and link the
native shared library:

```bash
c++ -std=c++20 -Icpp/chia_wallet_sdk \
    your_app.cpp cpp/chia_wallet_sdk/chia_wallet_sdk.cpp \
    -Lcpp/chia_wallet_sdk -lchia_wallet_sdk -o your_app

# Run (point the loader at the shared library)
DYLD_LIBRARY_PATH=cpp/chia_wallet_sdk ./your_app        # macOS
LD_LIBRARY_PATH=cpp/chia_wallet_sdk ./your_app          # Linux
```

Or use the provided CMake target — see `cpp/tests/CMakeLists.txt` for a working example.

---

## Quick Start

```cpp
#include <iostream>
#include "chia_wallet_sdk.hpp"

using namespace chia_wallet_sdk;

int main() {
    // CLVM round-trip
    auto clvm = std::make_shared<Clvm>();
    auto nil = clvm->nil();
    auto atom = clvm->atom({1, 2, 3});

    auto program = clvm->list({nil, atom});
    auto bytes = program->serialize();
    std::cout << "serialized " << bytes.size() << " bytes\n";

    // Encode a puzzle hash as a Chia address
    std::vector<uint8_t> puzzle_hash(32, 0);
    auto address = std::make_shared<Address>(puzzle_hash, "xch");
    std::cout << address->encode() << "\n";   // "xch1..."
    return 0;
}
```

> Method, type, and constructor names in the generated header follow
> `uniffi-bindgen-cpp` conventions. Inspect `chia_wallet_sdk.hpp` after generating for the
> exact signatures (objects are `std::shared_ptr<T>`, errors are thrown as exceptions).

---

## API Surface

The bindings cover the full SDK:

| Module | Types / Functions |
| ------ | ----------------- |
| BLS keys | `SecretKey`, `PublicKey`, `Signature`, `AggregateSignature` |
| Secp keys | `K1SecretKey`, `K1PublicKey`, `K1Signature`, `R1SecretKey`, `R1PublicKey`, `R1Signature` |
| Address | `Address` |
| Mnemonic | `Mnemonic` |
| Coin | `Coin`, `CoinSpend`, `CoinState` |
| CLVM | `Clvm`, `Program`, `CurriedProgram`, `Spend` |
| Conditions | `CreateCoin`, `AggSigMe`, `ReserveFee`, … (47 condition types) |
| Puzzles | `StandardPuzzle`, `CatPuzzle`, `NftPuzzle`, `DlPuzzle`, `SingletonPuzzle`, … |
| Offers | `Offer`, `ParsedOffer` |
| RPC | `RpcClient` (blocking — see *Async / RPC*) |
| Peer protocol | `Peer`, `Connector`, `Certificate` (blocking) |
| Simulator | `Simulator` (testing) |
| Utils | `sha256`, `hash_to_g2`, … |
| Constants | `Constants` (puzzle hashes for all built-in puzzles) |

---

## Type Mapping Reference

| Rust type | C++ type | Notes |
| --------- | -------- | ----- |
| `Vec<u8>` / bytes types | `std::vector<uint8_t>` | Puzzle hashes, keys, signatures |
| `u64`, `u128`, `BigInt` | `std::string` | Parse as a decimal integer |
| `u8`–`u32`, `i8`–`i32` | fixed-width int types | Passed through directly |
| `f64` | `double` | |
| `bool` | `bool` | |
| `String` | `std::string` | |
| `Option<T>` | `std::optional<T>` | |
| `Vec<T>` | `std::vector<T>` | |
| Rust struct / class | `std::shared_ptr<T>` | Reference-counted via `Arc` |
| Rust enum | C++ enum / variant | |

### BigInt / amounts

Chia amounts, heights, and arbitrary-precision integers are passed as decimal strings.
Parse them with your integer library of choice (e.g. `std::stoull` for values that fit in
64 bits, or a big-integer type for larger values).

### Object lifetime

Objects returned by the bindings are `std::shared_ptr<T>` wrapping a reference-counted
Rust allocation. The Rust object is freed automatically when the last `shared_ptr` goes
out of scope — no manual `Destroy()` call is required (unlike the Go backend).

---

## Async / RPC

`uniffi-bindgen-cpp` does **not** support async functions. Rather than drop the async
surface, the C++ backend is generated with the `bindy_uniffi_sync!` macro, which exposes
every `async` method and async factory as an ordinary **blocking** call. Each one drives
the underlying future to completion on a shared Tokio runtime
(`chia_sdk_bindings::block_on`) before returning.

```cpp
auto client = RpcClient::mainnet();
auto state = client->get_blockchain_state();   // blocks until the HTTP request completes
std::cout << (state->get_success() ? "ok" : "failed") << "\n";
```

Because these calls block the calling thread, run them on a worker thread if you need the
caller to stay responsive. This applies to `RpcClient` request methods and to the
peer-protocol classes (`Peer.connect`, `Peer.next`, etc.). The Go and C# backends keep
the native async API (Go: blocking; C#: `Task<T>`); only the C++ surface is synchronous.

---

## Architecture

```text
bindings/*.json           ← single source of truth for the API surface
       ↓
bindy_uniffi_sync! macro  ← generates #[derive(uniffi::Object)] / #[uniffi::export]
       ↓                      (the `_sync` variant turns async methods into blocking calls)
cpp/ crate                ← cdylib: setup_scaffolding! + generated + hand-written alloc
       ↓  cargo build --profile release-cpp
libchia_wallet_sdk.dylib ← native shared library
       ↓  uniffi-bindgen-cpp
chia_wallet_sdk.{hpp,cpp} ← C++ sources with all types and FFI bindings
```

The same `bindings/*.json` schemas also drive the Go (`go/`), C# (`dotnet/`), Node.js
(`napi/`), WebAssembly (`wasm/`), and Python (`pyo3/`) backends. Adding a method to a JSON
schema automatically adds it to all backends.

---

## Known Limitations

| Limitation | Details |
| ---------- | ------- |
| BigInt as string | `u64`/`u128`/`BigInt` map to `std::string`; parse on the C++ side. |
| `Clvm.alloc()` is typed | Unlike Python, which accepts dynamic types, `alloc` takes a `ClvmType` variant. Use the `Clvm` helper methods — `nil()`, `int()`, `atom()`, etc. — for primitive values. |
| Field setters return new objects | `set_field(value)` returns a new object with the field changed; the original is unchanged. Use the return value. |
| No `--config` in library mode | The namespace is fixed to `chia_wallet_sdk` by the UniFFI component name. |
| Async is blocking | `uniffi-bindgen-cpp` cannot generate async functions, so async methods (RPC, peer protocol) are exposed as blocking calls that run on a shared Tokio runtime (see *Async / RPC*). Run them off the main thread if responsiveness matters. |
| Generated-code patches | Two `uniffi-bindgen-cpp` defects are patched after generation: `Clvm::bool`/`int` → `bool_`/`int_` (reserved keywords), and the `VDFInfo`/`VDFProof` forward declarations → `VdfInfo`/`VdfProof` (acronym casing). Handled automatically by `local-build.sh`. |
| C++20 required | The generated code uses C++20 features. |

---

## Running the Tests

The `cpp/tests/` directory contains a CMake project whose suite mirrors the C# tests in
`dotnet/tests/BasicTests.cs` (hex/bytes utils, coin IDs, CLVM atom/string/int/pair
round-trips, public keys, serialization, currying, `alloc`, and condition parsing).

```bash
# 1. Generate bindings + native library
cd cpp
./local-build.sh -t aarch64-apple-darwin

# 2. Configure and build the test
cmake -S tests -B tests/build
cmake --build tests/build

# 3. Run
ctest --test-dir tests/build --output-on-failure
```

---

## Rebuilding After SDK Updates

When the SDK version bumps or new API is added, re-run the build script for each target
platform:

```bash
cd cpp
./local-build.sh -t aarch64-apple-darwin
./local-build.sh -t x86_64-unknown-linux-gnu
```

No manual patches are needed — the generated files are always a complete, self-contained
replacement.
