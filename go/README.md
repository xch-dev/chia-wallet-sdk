# Chia Wallet SDK for Go

Go bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), generated with [UniFFI](https://mozilla.github.io/uniffi-rs/) and [uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go).

## Installation

```bash
go get github.com/xch-dev/chia-wallet-sdk/go@v0.36.0
```

Each release of the module includes the generated bindings and a prebuilt static library for every supported platform, so no Rust toolchain is required. A C compiler is needed, since the package uses cgo.

| OS      | Architectures  |
| ------- | -------------- |
| Linux   | amd64, arm64   |
| macOS   | amd64, arm64   |
| Windows | amd64 (MinGW)  |

## Example

```go
package main

import (
	"fmt"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

func main() {
	sim, _ := sdk.NewSimulator()
	clvm, _ := sdk.NewClvm()
	alice, _ := sim.Bls(1000)

	createCoin, _ := clvm.CreateCoin(alice.PuzzleHash(), 900, nil)
	reserveFee, _ := clvm.ReserveFee(100)
	spend, _ := clvm.DelegatedSpend([]*sdk.Program{createCoin, reserveFee})

	if err := clvm.SpendStandardCoin(alice.Coin(), alice.Pk(), spend); err != nil {
		panic(err)
	}

	coinSpends, _ := clvm.CoinSpends()

	if err := sim.SpendCoins(coinSpends, []*sdk.SecretKey{alice.Sk()}); err != nil {
		panic(err)
	}

	coins, _ := sim.UnspentCoins(alice.PuzzleHash(), false)
	fmt.Println("Created a coin with", coins[0].Amount(), "mojos")
}
```

## Usage notes

- Objects are backed by Rust memory, which is freed by a finalizer when they're garbage collected. Call `Destroy()` to free them deterministically.
- Methods that can fail return an `error`. Field getters (such as `coin.Amount()`) can't fail, so they return the value directly.
- Setters (such as `coin.SetAmount(...)`) mutate the object in place. Use `Clone()` to copy an object.
- `u64` values are `uint64`. Values that can exceed 64 bits (`BigInt` and `u128`) are decimal strings, which can be parsed with `math/big`.
- Optional values are pointers, so optional objects are pointers to pointers (such as `**Program`).
- Static methods are exported as functions prefixed with the class name, such as `sdk.ConstantsCatPuzzleHash()`.
- Async methods (such as `RpcClient` requests) block the calling goroutine until they complete.
- Objects are safe to use from multiple goroutines.

## Development

To build the library and generate the bindings for the current platform, install [uniffi-bindgen-go](https://github.com/NordSecurity/uniffi-bindgen-go) and run the generation script:

```bash
cargo install uniffi-bindgen-go --git https://github.com/NordSecurity/uniffi-bindgen-go --tag v0.7.1+v0.31.0
./go/generate.sh
cd go && go test ./...
```

The generated bindings and libraries aren't checked in. They are committed to the `go/v*` tags by the release workflow instead.
