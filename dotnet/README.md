# Chia Wallet SDK for .NET

.NET bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), generated with [UniFFI](https://mozilla.github.io/uniffi-rs/) and [uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs).

## Installation

```bash
dotnet add package ChiaWalletSdk
```

The package targets .NET 8 and includes native libraries for Linux (x64, arm64), macOS (x64, arm64), and Windows (x64).

## Example

```csharp
using ChiaWalletSdk;

using var sim = new Simulator();
using var clvm = new Clvm();
using var alice = sim.Bls(1000);

var spend = clvm.DelegatedSpend([
    clvm.CreateCoin(alice.PuzzleHash(), 900, null),
    clvm.ReserveFee(100),
]);

clvm.SpendStandardCoin(alice.Coin(), alice.Pk(), spend);
sim.SpendCoins(clvm.CoinSpends(), [alice.Sk()]);

var coins = sim.UnspentCoins(alice.PuzzleHash(), false);
Console.WriteLine($"Created a coin with {coins[0].Amount()} mojos");
```

## Usage notes

- Objects are backed by Rust memory and implement `IDisposable`, so they can be freed deterministically with `using`. Otherwise, they are freed by a finalizer.
- Errors are thrown as `ChiaException`.
- Field getters and setters are methods (such as `coin.Amount()` and `coin.SetAmount(...)`). Setters mutate the object in place. Use `Clone()` to copy an object.
- `u64` values are `ulong`. Values that can exceed 64 bits (`BigInt` and `u128`) are decimal strings, which can be parsed with `System.Numerics.BigInteger.Parse`.
- Static methods are exported on the `Sdk` class, prefixed with the class name, such as `Sdk.ConstantsCatPuzzleHash()`.
- Async methods (such as `RpcClient` requests) return a `Task`.

## Development

To build the library and generate the bindings for the current platform, install [uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs) and run the generation script:

```bash
cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.11.1+v0.31.0
./dotnet/generate.sh
dotnet test dotnet/ChiaWalletSdk.Tests
```

The generation script also applies `fix-bindings.py`, which works around a few bugs in the generator. Each fix is documented in the script, and fails loudly if the generated code changes shape.
