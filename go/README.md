# Chia Wallet SDK - Go Bindings

Go bindings for the [Chia Wallet SDK](https://github.com/xch-dev/chia-wallet-sdk), providing access to Chia blockchain primitives via CGo.

## Installation

```bash
go get github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk
```

Prebuilt static libraries are included for all supported platforms, so no Rust toolchain is required.

## Supported Platforms

| OS | Architecture | Target |
|----|-------------|--------|
| Linux | x86_64 | `linux/amd64` |
| Linux | ARM64 | `linux/arm64` |
| macOS | x86_64 | `darwin/amd64` |
| macOS | ARM64 | `darwin/arm64` |
| Windows | x86_64 | `windows/amd64` |
| Windows | ARM64 | `windows/arm64` |
| Android | ARM64 | `android/arm64` |

## Quick Example

```go
package main

import (
	"fmt"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

func main() {
	sim, err := sdk.NewSimulator()
	if err != nil {
		panic(err)
	}
	defer sim.Close()

	clvm, err := sdk.NewClvm()
	if err != nil {
		panic(err)
	}
	defer clvm.Close()

	pair, err := sim.Bls(1000)
	if err != nil {
		panic(err)
	}
	defer pair.Close()

	coin, err := pair.Coin()
	if err != nil {
		panic(err)
	}
	defer coin.Close()
	pk, err := pair.Pk()
	if err != nil {
		panic(err)
	}
	defer pk.Close()
	sk, err := pair.Sk()
	if err != nil {
		panic(err)
	}
	defer sk.Close()
	puzzleHash, err := pair.PuzzleHash()
	if err != nil {
		panic(err)
	}

	createCoin, err := clvm.CreateCoin(puzzleHash, 900, nil)
	if err != nil {
		panic(err)
	}
	defer createCoin.Close()
	reserveFee, err := clvm.ReserveFee(100)
	if err != nil {
		panic(err)
	}
	defer reserveFee.Close()

	spend, err := clvm.DelegatedSpend([]*sdk.Program{createCoin, reserveFee})
	if err != nil {
		panic(err)
	}
	defer spend.Close()
	if err := clvm.SpendStandardCoin(coin, pk, spend); err != nil {
		panic(err)
	}

	coinSpends, err := clvm.CoinSpends()
	if err != nil {
		panic(err)
	}
	defer sdk.CloseAll(coinSpends)
	if err := sim.SpendCoins(coinSpends, []*sdk.SecretKey{sk}); err != nil {
		panic(err)
	}

	height, err := sim.Height()
	if err != nil {
		panic(err)
	}
	fmt.Printf("Transaction confirmed at height %d\n", height)
}
```

The transaction example also runs as a [Go example test](chiawalletsdk/example_test.go).

## Memory Management

Every SDK object wraps a Rust value behind an opaque pointer. Use `Close()` for deterministic cleanup; a runtime finalizer is only a fallback:

```go
clvm, err := sdk.NewClvm()
if err != nil {
    log.Fatal(err)
}
defer clvm.Close()
```

All types that wrap Rust pointers implement `io.Closer`. Use `defer obj.Close()` immediately after creation to prevent leaks.

## Building from Source

For contributors or platforms without prebuilt libraries:

```bash
# Prerequisites: Rust toolchain (https://rustup.rs)

# Build everything and run tests
cd go
make test

# Or step by step:
make generate    # Regenerate Go/Rust bindings from JSON specs
make build       # Compile the Rust static library
make install-lib # Copy the .a file to libs/<os>_<arch>/
```

The `install-lib` target detects your current `GOOS`/`GOARCH` automatically.

## Concurrency

SDK handles are safe for concurrent use, including when passed as arguments. Field setters and `Close` use exclusive locks; other operations use shared locks. Do not copy handles; use `Clone` when you need another owned handle. Callers must synchronize changes to their own slices and `big.Int` values.

Async calls take `context.Context` first. Cancellation stops the underlying native future before returning a context error. It cannot undo effects that have already completed.

Names such as `CoinID`, `BaseURL`, `RPCClient`, and `DataURIs` follow Go initialism conventions. The earlier spellings remain available for compatibility.
