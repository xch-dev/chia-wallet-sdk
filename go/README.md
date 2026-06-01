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
	"log"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

func main() {
	sim, err := sdk.SimulatorNew()
	if err != nil {
		log.Fatal(err)
	}
	defer sim.Close()

	clvm, err := sdk.ClvmNew()
	if err != nil {
		log.Fatal(err)
	}
	defer clvm.Close()

	pair, err := sim.Bls(1000)
	if err != nil {
		log.Fatal(err)
	}
	defer pair.Close()

	coin, _ := pair.Coin()
	defer coin.Close()
	pk, _ := pair.Pk()
	defer pk.Close()
	sk, _ := pair.Sk()
	defer sk.Close()
	puzzleHash, _ := pair.PuzzleHash()

	createCoin, _ := clvm.CreateCoin(puzzleHash, 900, nil)
	defer createCoin.Close()
	reserveFee, _ := clvm.ReserveFee(100)
	defer reserveFee.Close()

	spend, _ := clvm.DelegatedSpend([]*sdk.Program{createCoin, reserveFee})
	defer spend.Close()
	clvm.SpendStandardCoin(coin, pk, spend)

	coinSpends, _ := clvm.CoinSpends()
	sim.SpendCoins(coinSpends, []*sdk.SecretKey{sk})

	height, _ := sim.Height()
	fmt.Printf("Transaction confirmed at height %d\n", height)
}
```

## Memory Management

Every SDK object wraps a Rust value behind an opaque pointer. Go's garbage collector cannot free these automatically, so you must call `Close()` when done:

```go
clvm, err := sdk.ClvmNew()
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

All SDK objects are safe for concurrent use from multiple goroutines. Each object uses an internal `sync.RWMutex` and pins to an OS thread during FFI calls.
