# chia-wallet-sdk C# Bindings

C# (and .NET) bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), generated via [UniFFI](https://mozilla.github.io/uniffi-rs/) and [uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs).

The Rust library exposes the full SDK surface — BLS keys, addresses, CLVM, coins, conditions, offers, puzzles, RPC, and more — as a native shared library that C# loads via P/Invoke through the UniFFI scaffolding layer.

---

## Prerequisites

- Rust toolchain (1.81+) — [rustup.rs](https://rustup.rs)
- .NET 6+ SDK — [dot.net](https://dotnet.microsoft.com)
- `uniffi-bindgen-cs` — installed once per machine (see below)

---

## Step 1: Build the Native Library

```bash
# From the repo root
cargo build -p chia-wallet-sdk-cs --release
```

Output location by platform:

| Platform | File |
|----------|------|
| macOS | `target/release/libchia_wallet_sdk.dylib` |
| Linux | `target/release/libchia_wallet_sdk.so` |
| Windows | `target/release/chia_wallet_sdk.dll` |

A debug build (`--release` omitted) is faster to compile but slower at runtime.

---

## Step 2: Install `uniffi-bindgen-cs`

This tool generates the C# source file from the compiled library. Install it once; the version **must match** the `uniffi` crate version used here (`0.29`).

```bash
cargo install uniffi-bindgen-cs \
  --git https://github.com/NordSecurity/uniffi-bindgen-cs \
  --tag v0.10.0+v0.29.4
```

---

## Step 3: Generate the C# Source

```bash
# macOS
uniffi-bindgen-cs \
  --library \
  --out-dir uniffi/cs \
  --config uniffi/uniffi.toml \
  target/release/libchia_wallet_sdk.dylib

# Linux
uniffi-bindgen-cs \
  --library \
  --out-dir uniffi/cs \
  --config uniffi/uniffi.toml \
  target/release/libchia_wallet_sdk.so

# Windows
uniffi-bindgen-cs \
  --library \
  --out-dir uniffi/cs \
  --config uniffi/uniffi.toml \
  target/release/chia_wallet_sdk.dll
```

This produces `uniffi/cs/chia_wallet_sdk.cs` — a single self-contained C# file that includes all types, P/Invoke declarations, and marshalling logic.

Re-run this step any time the Rust library is rebuilt after API changes.

---

## Step 4: Use in a .NET Project

### Option A — Via NuGet (recommended)

```bash
dotnet add package ChiaWalletSdk
```

The package bundles the native library for each supported platform; no manual copy step is needed.

### Option B — In-repo via ProjectReference

If you are working inside a clone of this repository, reference the library project directly:

```xml
<!-- .csproj -->
<ItemGroup>
  <ProjectReference Include="path/to/uniffi/cs/ChiaWalletSdk.csproj" />
</ItemGroup>
```

Ensure the native library is in the output directory (set `Copy to Output Directory` in your `.csproj` or add it to the build pipeline).

Everything lives in the `ChiaWalletSdk` namespace.

---

## Quick Start

```csharp
using ChiaWalletSdk;
using System.Numerics;

// Generate a BLS key from a seed phrase
var mnemonic = Mnemonic.Generate();
var seed = mnemonic.ToSeed("");
var secretKey = SecretKey.FromSeed(seed);
var publicKey = secretKey.PublicKey();

// Encode a puzzle hash as a Chia address
var puzzleHash = /* 32-byte array */ new byte[32];
var address = new Address(puzzleHash, "xch");
Console.WriteLine(address.Encode());   // "xch1..."

// Decode an address back to a puzzle hash
var decoded = Address.Decode("xch1...");
byte[] ph = decoded.GetPuzzleHash();

// Work with amounts — bigint values come back as strings
var clvm = new Clvm();
// amounts like coin.amount are strings; parse with BigInteger
BigInteger amount = BigInteger.Parse(someAmountString);
```

---

## Running the Tests

An xUnit test suite in `uniffi/tests/` exercises the core binding surface — CLVM, keys, coins, addresses, and conditions.

### Prerequisites

Build the native library first (required before `dotnet test` can load it):

```bash
# From the repo root
cargo build -p chia-wallet-sdk-cs --release
```

### Run

```bash
cd uniffi/tests
dotnet test
```

All 13 tests should pass.

---

## API Surface

The bindings cover the full SDK:

| Module | Classes / Functions |
|--------|-------------------|
| BLS keys | `SecretKey`, `PublicKey`, `Signature`, `AggregateSignature` |
| Secp keys | `K1SecretKey`, `K1PublicKey`, `K1Signature`, `R1SecretKey`, `R1PublicKey`, `R1Signature` |
| Address | `Address` |
| Mnemonic | `Mnemonic` |
| Coin | `Coin`, `CoinSpend`, `CoinState` |
| CLVM | `Clvm`, `Program`, `CurriedProgram`, `Spend` |
| Conditions | `CreateCoin`, `AggSigMe`, `ReserveFee`, … (47 condition types) |
| Puzzles | `StandardPuzzle`, `CatPuzzle`, `NftPuzzle`, `DlPuzzle`, `SingletonPuzzle`, … |
| Offers | `Offer`, `ParsedOffer` |
| RPC | `RpcClient` (async) |
| Simulator | `Simulator` (async, testing) |
| Utils | `sha256`, `hash_to_g2`, … |
| Constants | `Constants` (puzzle hashes for all built-in puzzles) |

---

## Type Mapping Reference

| Rust type | C# type | Notes |
|-----------|---------|-------|
| `Vec<u8>` / bytes types | `byte[]` | Puzzle hashes, keys, signatures |
| `u64`, `u128`, `BigInt` | `string` | Parse with `BigInteger.Parse()` |
| `u8`–`u32`, `i8`–`i32` | native int types | Passed through directly |
| `usize` | `uint` | Mapped to `u32` |
| `f64` | `double` | |
| `bool` | `bool` | |
| `String` | `string` | |
| `Option<T>` | `T?` (nullable) | |
| `Vec<T>` | `List<T>` | |
| Rust struct / class | C# class | Reference-counted via `Arc` |
| Rust enum | C# enum | |

### BigInt / amounts

Chia amounts, heights, and arbitrary-precision integers are all passed as decimal strings. On the C# side:

```csharp
// Rust returns u64/u128/BigInt → string
string amountStr = coin.GetAmount();
BigInteger amount = BigInteger.Parse(amountStr);

// Passing back in
string newAmount = (amount + 1).ToString();
```

---

## Async Methods

Async methods (RPC calls, simulator) return `Task<T>` in C#. They run on the Rust tokio runtime bridged through UniFFI:

```csharp
var client = await RpcClient.New("https://api.coinset.org");
var state = await client.GetBlockchainState();
```

---

## Architecture

```
bindings/*.json          ← single source of truth for the API surface
       ↓
bindy_uniffi! macro      ← generates #[derive(uniffi::Object)] / #[uniffi::export]
       ↓
uniffi/ crate            ← cdylib: setup_scaffolding! + generated + hand-written alloc
       ↓  cargo build
libchia_wallet_sdk.dylib ← native shared library
       ↓  uniffi-bindgen-cs
chia_wallet_sdk.cs       ← C# source with all types and P/Invoke bindings
```

The same `bindings/*.json` schemas also drive the Node.js (`napi/`), WebAssembly (`wasm/`), and Python (`pyo3/`) backends. Adding a method to a JSON schema automatically adds it to all four backends.

---

## Known Limitations

| Limitation | Details |
|------------|---------|
| BigInt as string | `u64`/`u128`/`BigInt` map to `string`; parse with `BigInteger.Parse()`. A typed UniFFI custom type could improve this in a future version. |
| `Clvm.Alloc()` is typed | Unlike Python, which accepts `None`/`int`/`bool`/`str`/`bytes`/`list` dynamically, the C# `Alloc` takes a `ClvmType` enum. Use `Clvm.Nil()`, `Clvm.Int()`, `Clvm.Atom()` etc. for primitive values. |
| Field setters are immutable | `SetField(value)` returns a new object with the field changed; the original is unchanged. Use the return value. |
| Version pinning | `uniffi-bindgen-cs` must match the `uniffi` crate version (`0.29`). Check the tag when upgrading. |
| No static methods on objects | UniFFI 0.29 does not support non-`self` associated functions in `impl` blocks. These are exposed as free functions prefixed with the class name (e.g. `constants_puzzle_name()`). |

---

## Rebuilding After SDK Updates

When the SDK version bumps or new API is added:

```bash
# 1. Rebuild the native library
cargo build -p chia-wallet-sdk-cs --release

# 2. Regenerate C# source
uniffi-bindgen-cs generate \
  --library target/release/libchia_wallet_sdk.dylib \
  --out-dir uniffi/cs \
  --config uniffi/uniffi.toml

# 3. Replace the .cs file in your project
```

No manual patches needed — the `PatchClvmStringAmbiguity` MSBuild target in `ChiaWalletSdk.csproj` automatically fixes a name-shadowing bug in the generated code on every compile. (Root cause: `Clvm.String(string)` shadows `System.String` inside the class body, causing the generated lifecycle guards to fail. Tracked upstream at [NordSecurity/uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs).)
