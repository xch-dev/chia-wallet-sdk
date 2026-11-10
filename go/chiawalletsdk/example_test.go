package chiawalletsdk_test

import (
	"fmt"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// This is the example from the README.
func Example() {
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

	// Output: Created a coin with 900 mojos
}
