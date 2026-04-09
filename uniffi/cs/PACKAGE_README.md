# ChiaWalletSdk

C# bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk) -- a Rust library for building Chia blockchain wallets, exposed to .NET via [UniFFI](https://mozilla.github.io/uniffi-rs/).

Native libraries for macOS (arm64, x64), Windows (x64), and Linux (x64) are bundled in the package.

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
var puzzleHash = new byte[32];
var address = new Address(puzzleHash, "xch");
Console.WriteLine(address.Encode());   // "xch1..."

// Async RPC
var client = await RpcClient.New("https://api.coinset.org");
var state = await client.GetBlockchainState();
```

## API Surface

| Module | Classes / Functions |
|--------|-------------------|
| BLS keys | `SecretKey`, `PublicKey`, `Signature`, `AggregateSignature` |
| Secp keys | `K1SecretKey`, `K1PublicKey`, `K1Signature`, `R1SecretKey`, `R1PublicKey`, `R1Signature` |
| Address | `Address` |
| Mnemonic | `Mnemonic` |
| Coin | `Coin`, `CoinSpend`, `CoinState` |
| CLVM | `Clvm`, `Program`, `CurriedProgram`, `Spend` |
| Conditions | `CreateCoin`, `AggSigMe`, `ReserveFee`, and more |
| Puzzles | `StandardPuzzle`, `CatPuzzle`, `NftPuzzle`, `DlPuzzle`, `SingletonPuzzle`, etc. |
| Offers | `Offer`, `ParsedOffer` |
| RPC | `RpcClient` (async) |
| Simulator | `Simulator` (async, testing) |
| Utils | `sha256`, `hash_to_g2`, etc. |
| Constants | `Constants` (puzzle hashes for all built-in puzzles) |

## Type Mapping

| Rust type | C# type | Notes |
|-----------|---------|-------|
| `Vec<u8>` / bytes | `byte[]` | Puzzle hashes, keys, signatures |
| `u64`, `u128`, `BigInt` | `string` | Parse with `BigInteger.Parse()` |
| `u8`--`u32`, `i8`--`i32` | native int types | Passed through directly |
| `Option<T>` | `T?` (nullable) | |
| `Vec<T>` | `List<T>` | |
| Rust struct / class | C# class | Reference-counted via `Arc` |

### Amounts and large integers

Chia amounts and arbitrary-precision integers are passed as decimal strings:

```csharp
string amountStr = coin.GetAmount();
BigInteger amount = BigInteger.Parse(amountStr);
```

## Async Methods

Async methods (RPC, simulator) return `Task<T>` and run on the Rust tokio runtime:

```csharp
var client = await RpcClient.New("https://api.coinset.org");
var state = await client.GetBlockchainState();
```

## More Information

- [Source & full documentation](https://github.com/dkackman/chia-wallet-sdk/tree/main/uniffi)
- [Upstream Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk)
- License: Apache-2.0
