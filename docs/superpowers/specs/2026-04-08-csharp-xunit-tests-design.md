---
name: C# xUnit Test Project Design
description: Design for adding an xUnit test project for the C# UniFFI bindings in uniffi/cs/
type: project
---

# C# xUnit Test Project

## Overview

Add an xUnit test project for the C# bindings (`uniffi/cs/chia_wallet_sdk.cs`), mirroring the coverage of `pyo3/tests/test_pyo3.py` and `napi/__test__/napi.spec.ts`.

## Project Structure

```
uniffi/
  cs/
    chia_wallet_sdk.cs          (existing, generated — do not modify)
  tests/
    ChiaWalletSdkTests.csproj
    BasicTests.cs
```

`uniffi/tests/` is separate from `uniffi/cs/` to keep the generated directory clean, matching the pattern of `pyo3/tests/` being alongside but separate from the source.

## .csproj Configuration

- `<TargetFramework>net8.0</TargetFramework>`
- `<AllowUnsafeBlocks>true</AllowUnsafeBlocks>` — required by the generated UniFFI scaffolding
- `<Nullable>enable</Nullable>`
- Include the generated source directly in the same assembly:
  ```xml
  <Compile Include="../cs/chia_wallet_sdk.cs" />
  ```
- Copy the native shared library to the output directory:
  ```xml
  <None Include="../../target/release/libchia_wallet_sdk.dylib">
    <CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
  </None>
  ```
- NuGet packages: `xunit`, `xunit.runner.visualstudio`, `Microsoft.NET.Test.Sdk`

Including `chia_wallet_sdk.cs` in the same assembly (rather than a separate project reference) is required because all UniFFI-generated types are `internal`. This is consistent with the README's recommended usage pattern.

## Test Coverage

All tests live in `BasicTests.cs` in namespace `ChiaWalletSdk.Tests`.

| Test | API exercised | Mirrors |
|------|--------------|---------|
| `AllocValues` | `Clvm.Alloc(ClvmType)` with PublicKey, atom, bool, nil, RunCatTail | Python `test_alloc` |
| `CoinId` | `new Coin(...).CoinId()` against known hash | TS `calculate coin id` |
| `ByteEquality` | `ChiaWalletSdkMethods.BytesEqual` | TS `byte equality/inequality` |
| `AtomRoundtrip` | `Clvm.Atom(bytes)` → `Program.ToAtom()` | TS `atom roundtrip` |
| `StringRoundtrip` | `Clvm.Alloc(ClvmType.Atom("hello world"))` → `Program.ToString()` | TS `string roundtrip` |
| `IntRoundtrip` | `Clvm.Int("42")` → `Program.ToInt()` | TS `bigint roundtrip` |
| `PairRoundtrip` | `Clvm.Pair(a,b)` → `Program.ToPair()` | TS `pair roundtrip` |
| `ClvmSerialization` | `Serialize()` → `Deserialize()` + hex check | TS `clvm serialization` |
| `PublicKeyRoundtrip` | `PublicKey.Infinity()` → `ToBytes()` → `FromBytes()` | TS `public key roundtrip` |
| `ToHexFromHex` | `ChiaWalletSdkMethods.ToHex/FromHex` roundtrip | TS buffer test |
| `CurryRoundtrip` | `Curry([...])` → `Uncurry()` | TS `curry roundtrip` |
| `CreateAndParseCondition` | `Clvm.CreateCoin(...)` → `ParseCreateCoin()` | TS `create and parse condition` |

## Data Notes

- `Coin` constructor takes `(byte[] parentCoinInfo, byte[] puzzleHash, string amount)` — amounts are decimal strings
- `ClvmType` is a discriminated union: wrap primitives in `new ClvmType.Atom(...)`, `new ClvmType.PublicKey(pk)`, etc.; use `Clvm.Nil()`, `Clvm.Bool()`, `Clvm.Int()`, `Clvm.Atom()` for primitives instead of `Alloc` where simpler
- `ChiaWalletSdkMethods` (the static free-functions class) contains `ToHex`, `FromHex`, `BytesEqual`, `Sha256`, `CurryTreeHash`, etc.
- `Program.ToInt()` returns `string?` (decimal), not a numeric type — BigInteger.Parse if arithmetic needed
- `RunCatTail` is constructed with two `Program` args

## Build Requirements

Before running tests:
```bash
cargo build -p chia-wallet-sdk-cs --release
```

The native library must exist at `target/release/libchia_wallet_sdk.dylib` (macOS) before `dotnet test` will succeed.
