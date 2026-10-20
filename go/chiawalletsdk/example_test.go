package chiawalletsdk_test

import (
	"fmt"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

func ExampleSimulator() {
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
	// Output: Transaction confirmed at height 1
}

func ExampleCoin_CoinID() {
	coin, err := sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1000)
	if err != nil {
		panic(err)
	}
	defer coin.Close()
	id, err := coin.CoinID()
	if err != nil {
		panic(err)
	}
	fmt.Println(len(id))
	// Output: 32
}
