# chia-wallet-sdk Go Bindings

Go bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), generated via [UniFFI](https://mozilla.github.io/uniffi-rs/) and [uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go).

The Rust library exposes the full SDK surface — BLS keys, addresses, CLVM, coins, conditions, offers, puzzles, RPC, and more — as a native shared library that Go loads via CGo through the UniFFI scaffolding layer.

---

## Prerequisites

- Rust toolchain (1.81+) — [rustup.rs](https://rustup.rs)
- Go 1.24+ — [go.dev](https://go.dev)
- A C compiler (required by CGo) — Xcode CLT on macOS, `build-essential` on Linux, MSVC or MinGW on Windows
- `uniffi-bindgen-go` — installed once per machine (see below)

---

## Building

The `local-build.sh` script handles all steps: compiling the native library, installing `uniffi-bindgen-go` if needed, generating the Go source, and staging the shared library.

```bash
cd go
./local-build.sh                              # defaults: aarch64-apple-darwin
./local-build.sh -t x86_64-apple-darwin
./local-build.sh -t x86_64-unknown-linux-gnu
./local-build.sh -t x86_64-pc-windows-msvc   # PowerShell: .\local-build.ps1
```

After running, the `go/chia_wallet_sdk/` directory contains:

| File | Description |
|------|-------------|
| `chia_wallet_sdk.go` | All generated Go types, functions, and CGo glue |
| `chia_wallet_sdk.h` | C header required at CGo compile time |
| `libchia_wallet_sdk.dylib` / `.so` / `.dll` | Native shared library |

These files are git-ignored and must be regenerated per platform.

---

## Step-by-step (manual)

### 1 — Build the native library

```bash
# From the repo root
cargo build --profile release-go -p chia-wallet-sdk-go --target aarch64-apple-darwin
```

Output by platform:

| Platform | File |
|----------|------|
| macOS | `target/<target>/release-go/libchia_wallet_sdk.dylib` |
| Linux | `target/<target>/release-go/libchia_wallet_sdk.so` |
| Windows | `target/<target>/release-go/chia_wallet_sdk.dll` |

### 2 — Install `uniffi-bindgen-go`

```bash
cargo install uniffi-bindgen-go \
  --git https://github.com/NordSecurity/uniffi-bindgen-go \
  --tag v0.5.0+v0.29.5
```

> **Note:** the workspace is pinned to `uniffi = "=0.29.4"`. The 0.29.4→0.29.5 patch-version mismatch between this tool and the compiled library is benign. When `uniffi-bindgen-cs` releases a `+v0.29.5` tag, both tools and the workspace pin can be aligned in one step.

### 3 — Generate the Go source

```bash
uniffi-bindgen-go \
  --library target/aarch64-apple-darwin/release-go/libchia_wallet_sdk.dylib \
  --out-dir go/
```

This writes `go/chia_wallet_sdk/chia_wallet_sdk.go` and `go/chia_wallet_sdk/chia_wallet_sdk.h`.

---

## Using in a Go Project

### CGo linking

The generated code uses `#include <chia_wallet_sdk.h>` and links against `libchia_wallet_sdk`. The linker must be able to find the shared library. The simplest approach during development is to set `CGO_LDFLAGS`:

```bash
export CGO_LDFLAGS="-L/path/to/chia-wallet-sdk/go/chia_wallet_sdk"
go build ./...
```

Or install the library system-wide:

```bash
# macOS
sudo cp go/chia_wallet_sdk/libchia_wallet_sdk.dylib /usr/local/lib/

# Linux
sudo cp go/chia_wallet_sdk/libchia_wallet_sdk.so /usr/local/lib/
sudo ldconfig
```

### Import

```go
import chia "github.com/xch-dev/chia-wallet-sdk/go/chia_wallet_sdk"
```

---

## Quick Start

```go
package main

import (
    "fmt"
    "math/big"

    chia "github.com/xch-dev/chia-wallet-sdk/go/chia_wallet_sdk"
)

func main() {
    // Generate a 24-word BLS key
    mnemonic, err := chia.MnemonicGenerate(true)
    if err != nil {
        panic(err)
    }
    defer mnemonic.Destroy()

    seed, err := mnemonic.ToSeed("")
    if err != nil {
        panic(err)
    }

    sk, err := chia.SecretKeyFromSeed(seed)
    if err != nil {
        panic(err)
    }
    defer sk.Destroy()

    pk, err := sk.PublicKey()
    if err != nil {
        panic(err)
    }
    defer pk.Destroy()

    // Encode a puzzle hash as a Chia address
    puzzleHash := make([]byte, 32)
    addr, err := chia.NewAddress(puzzleHash, "xch")
    if err != nil {
        panic(err)
    }
    defer addr.Destroy()

    encoded, err := addr.Encode()
    if err != nil {
        panic(err)
    }
    fmt.Println(encoded) // "xch1..."

    // Decode an address back to its puzzle hash
    decoded, err := chia.AddressDecode("xch1...")
    if err != nil {
        panic(err)
    }
    defer decoded.Destroy()

    ph, err := decoded.GetPuzzleHash()
    if err != nil {
        panic(err)
    }
    _ = ph

    // Amounts are returned as decimal strings; use math/big for arithmetic
    // (example: coin.GetAmount() returns a string)
    amountStr := "1000000000000"
    amount, _ := new(big.Int).SetString(amountStr, 10)
    newAmount := new(big.Int).Add(amount, big.NewInt(1)).String()
    _ = newAmount

    // RPC
    client, err := chia.NewRpcClient("https://api.coinset.org")
    if err != nil {
        panic(err)
    }
    defer client.Destroy()

    state, err := client.GetBlockchainState()
    if err != nil {
        panic(err)
    }
    defer state.Destroy()
}
```

---

## API Surface

The bindings cover the full SDK:

| Module | Types / Functions |
|--------|------------------|
| BLS keys | `SecretKey`, `PublicKey`, `Signature`, `AggregateSignature` |
| Secp keys | `K1SecretKey`, `K1PublicKey`, `K1Signature`, `R1SecretKey`, `R1PublicKey`, `R1Signature` |
| Address | `Address`, `AddressDecode` |
| Mnemonic | `MnemonicGenerate`, `NewMnemonic`, `MnemonicFromEntropy` |
| Coin | `Coin`, `CoinSpend`, `CoinState` |
| CLVM | `Clvm`, `Program`, `CurriedProgram`, `Spend` |
| Conditions | `CreateCoin`, `AggSigMe`, `ReserveFee`, … (47 condition types) |
| Puzzles | `StandardPuzzle`, `CatPuzzle`, `NftPuzzle`, `DlPuzzle`, `SingletonPuzzle`, … |
| Offers | `Offer`, `ParsedOffer` |
| RPC | `NewRpcClient`, `RpcClientMainnet`, `RpcClientTestnet11`, `RpcClientLocal` |
| Simulator | `Simulator` (testing) |
| Utils | `Sha256`, `HashToG2`, … |
| Constants | `Constants` (puzzle hashes for all built-in puzzles) |

---

## Type Mapping Reference

| Rust type | Go type | Notes |
|-----------|---------|-------|
| `Vec<u8>` / bytes types | `[]byte` | Puzzle hashes, keys, signatures |
| `u64`, `u128`, `BigInt` | `string` | Parse with `new(big.Int).SetString(s, 10)` |
| `u8`–`u32` | `uint8`–`uint32` | |
| `i8`–`i32` | `int8`–`int32` | |
| `u64` | `uint64` | |
| `f64` | `float64` | |
| `bool` | `bool` | |
| `String` | `string` | |
| `Option<T>` | `*T` | `nil` represents `None` |
| `Vec<T>` | `[]T` | |
| Rust struct / class | Go struct (pointer receiver) | Reference-counted via `Arc`; call `Destroy()` when done |
| Rust enum | Go type with constants | |

### BigInt / amounts

Chia amounts, heights, and arbitrary-precision integers are passed as decimal strings. Use `math/big` on the Go side:

```go
amountStr, _ := coin.GetAmount()       // returns "1750000000000"
amount, _ := new(big.Int).SetString(amountStr, 10)
incremented := new(big.Int).Add(amount, big.NewInt(1)).String()
```

### Object lifetime

Every object returned by the bindings is heap-allocated on the Rust side and reference-counted. Call `Destroy()` when you are done with an object, or defer it immediately after construction:

```go
sk, err := chia.SecretKeyFromSeed(seed)
if err != nil { ... }
defer sk.Destroy()
```

Failing to call `Destroy()` leaks the Rust allocation. The garbage collector does not automatically free Rust objects.

---

## RPC

RPC calls are synchronous (blocking). Wrap them in a goroutine if you need concurrency:

```go
client, err := chia.RpcClientMainnet()
if err != nil { panic(err) }
defer client.Destroy()

// Blocking call — runs on the Rust tokio runtime under the hood
state, err := client.GetBlockchainState()
if err != nil { panic(err) }
defer state.Destroy()
```

---

## Architecture

```
bindings/*.json          ← single source of truth for the API surface
       ↓
bindy_uniffi! macro      ← generates #[derive(uniffi::Object)] / #[uniffi::export]
       ↓
go/ crate                ← cdylib: setup_scaffolding! + generated + hand-written alloc
       ↓  cargo build --profile release-go
libchia_wallet_sdk.dylib ← native shared library
       ↓  uniffi-bindgen-go
chia_wallet_sdk.go       ← Go package with all types and CGo bindings
```

The same `bindings/*.json` schemas also drive the C# (`uniffi/`), Node.js (`napi/`), WebAssembly (`wasm/`), and Python (`pyo3/`) backends. Adding a method to a JSON schema automatically adds it to all backends.

---

## Known Limitations

| Limitation | Details |
|------------|---------|
| BigInt as string | `u64`/`u128`/`BigInt` map to `string`; parse with `math/big`. |
| Manual `Destroy()` | Rust objects must be explicitly destroyed; the Go GC does not reach Rust heap allocations. |
| `Clvm.Alloc()` is typed | Unlike Python, which accepts dynamic types, `Alloc` takes a `ClvmType` enum. Use `NewClvm()` helper methods — `Nil()`, `Int()`, `Atom()`, etc. — for primitive values. |
| Field setters return new objects | `SetField(value)` returns a new object with the field changed; the original is unchanged. Use the return value. |
| CGo required | Pure-Go builds are not supported. A C compiler must be available in the build environment. |
| Version note | `uniffi-bindgen-go` `v0.5.0+v0.29.5` is used with a `uniffi = "=0.29.4"` workspace. The patch-version mismatch is benign. Align both when `uniffi-bindgen-cs` releases a `+v0.29.5` tag. |

---

## Rebuilding After SDK Updates

When the SDK version bumps or new API is added, re-run the build script for each target platform:

```bash
cd go
./local-build.sh -t aarch64-apple-darwin
./local-build.sh -t x86_64-unknown-linux-gnu
```

No manual patches are needed — the generated file is always a complete, self-contained replacement.
