package chiawalletsdk_test

import (
	"bytes"
	"strings"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// signStandardSpend spends the coin to a new coin of `amount` with the same puzzle hash, and
// signs it with the required signatures for testnet11, which both simulators use.
func signStandardSpend(t *testing.T, pair *sdk.BlsPairWithCoin, amount uint64) *sdk.SpendBundle {
	t.Helper()

	clvm := must(sdk.NewClvm())
	createCoin := must(clvm.CreateCoin(pair.PuzzleHash(), amount, nil))
	spend := must(clvm.DelegatedSpend([]*sdk.Program{createCoin}))
	check(t, clvm.SpendStandardCoin(pair.Coin(), pair.Pk(), spend))

	coinSpends := must(clvm.CoinSpends())
	required := must(clvm.RequiredSignatures(coinSpends, must(sdk.AggSigConstantsTestnet11())))

	if len(required.Secp()) != 0 {
		t.Fatalf("expected no secp signatures, got %d", len(required.Secp()))
	}

	var signatures []*sdk.Signature
	for _, item := range required.Bls() {
		if !bytes.Equal(must(item.PublicKey().ToBytes()), must(pair.Pk().ToBytes())) {
			t.Fatal("expected the required signature to be for the coin's key")
		}
		signatures = append(signatures, must(pair.Sk().Sign(must(item.Message()))))
	}

	if len(signatures) != 1 {
		t.Fatalf("expected one bls signature, got %d", len(signatures))
	}

	return must(sdk.NewSpendBundle(coinSpends, must(sdk.SignatureAggregate(signatures))))
}

func TestSignWithRequiredSignatures(t *testing.T) {
	sim := must(sdk.NewSimulator())
	alice := must(sim.Bls(1000))

	check(t, sim.NewTransaction(signStandardSpend(t, alice, 900)))

	coins := must(sim.UnspentCoins(alice.PuzzleHash(), false))
	if len(coins) != 1 || coins[0].Amount() != 900 {
		t.Fatalf("expected one coin of 900 mojos, got %d coins", len(coins))
	}
}

func TestRequiredSignaturesPuzzleHashMismatch(t *testing.T) {
	sim := must(sdk.NewSimulator())
	alice := must(sim.Bls(1000))
	bob := must(sim.Bls(1000))

	clvm := must(sdk.NewClvm())
	createCoin := must(clvm.CreateCoin(alice.PuzzleHash(), 1000, nil))
	spend := must(clvm.DelegatedSpend([]*sdk.Program{createCoin}))

	// Bob's key gives a puzzle reveal that doesn't match Alice's coin.
	check(t, clvm.SpendStandardCoin(alice.Coin(), bob.Pk(), spend))

	_, err := clvm.RequiredSignatures(must(clvm.CoinSpends()), must(sdk.AggSigConstantsTestnet11()))
	if err == nil || !strings.Contains(err.Error(), "does not match coin puzzle hash") {
		t.Fatalf("expected a puzzle hash mismatch error, got %v", err)
	}
}

// buildWithFee applies the actions with the fee, spends every pending coin with the wallet's
// standard puzzle, and takes the coin spends out of the Clvm so the next attempt starts fresh.
func buildWithFee(
	t *testing.T,
	clvm *sdk.Clvm,
	pair *sdk.BlsPairWithCoin,
	spends *sdk.Spends,
	actions []*sdk.Action,
	xchCoins []*sdk.Coin,
	fee uint64,
) (*sdk.Outputs, []*sdk.CoinSpend) {
	t.Helper()

	finished := must(spends.ApplyWithFee(actions, xchCoins, fee, nil))

	for _, pending := range must(finished.PendingSpends()) {
		delegated := must(clvm.DelegatedSpend(must(pending.Conditions())))
		check(t, finished.Insert(must(must(pending.Coin()).CoinId()), must(clvm.StandardSpend(pair.Pk(), delegated))))
	}

	outputs := must(finished.Spend())
	return outputs, must(clvm.CoinSpends())
}

func TestApplyWithMinimumFee(t *testing.T) {
	sim := must(sdk.NewSimulator())
	clvm := must(sdk.NewClvm())

	// The first coin covers the payment exactly, so the fee needs the second one.
	alice := must(sim.Bls(1000))
	feeCoin := must(sim.NewCoin(alice.PuzzleHash(), 1_000_000_000_000))
	bob := must(sim.Bls(0))

	spends := must(sdk.NewSpends(clvm, alice.PuzzleHash()))
	actions := []*sdk.Action{must(sdk.ActionSend(must(sdk.IdXch()), bob.PuzzleHash(), 1000, nil))}
	xchCoins := []*sdk.Coin{alice.Coin(), feeCoin}
	height := uint32(9_500_000)

	var fee, cost uint64
	var outputs *sdk.Outputs
	var coinSpends []*sdk.CoinSpend
	converged := false

	for attempt := 0; attempt < 8; attempt++ {
		outputs, coinSpends = buildWithFee(t, clvm, alice, spends, actions, xchCoins, fee)
		cost = must(sdk.SpendBundleCost(coinSpends, true, &height))
		needed := must(sdk.RecommendedFee(cost, nil))

		if fee >= needed {
			converged = true
			break
		}

		fee = needed
	}

	if !converged {
		t.Fatal("the fee didn't converge")
	}

	if fee == 0 || fee != must(sdk.MinimumFee(cost)) || must(outputs.Fee()) != fee {
		t.Fatalf("expected the minimum fee for cost %d, got %d", cost, fee)
	}

	spentFeeCoin := false
	for _, coinSpend := range coinSpends {
		if bytes.Equal(must(coinSpend.Coin().CoinId()), must(feeCoin.CoinId())) {
			spentFeeCoin = true
		}
	}
	if !spentFeeCoin {
		t.Fatal("expected the fee coin to be spent")
	}

	check(t, sim.SpendCoins(coinSpends, []*sdk.SecretKey{alice.Sk()}))

	coins := must(sim.UnspentCoins(bob.PuzzleHash(), false))
	total := uint64(0)
	for _, coin := range coins {
		total += coin.Amount()
	}
	if total != 1000 {
		t.Fatalf("expected bob to receive 1000 mojos, got %d", total)
	}
}

func TestApplyWithFeeInsufficientFunds(t *testing.T) {
	sim := must(sdk.NewSimulator())
	clvm := must(sdk.NewClvm())

	alice := must(sim.Bls(1000))
	bob := must(sim.Bls(0))

	spends := must(sdk.NewSpends(clvm, alice.PuzzleHash()))
	actions := []*sdk.Action{must(sdk.ActionSend(must(sdk.IdXch()), bob.PuzzleHash(), 1000, nil))}

	if _, err := spends.ApplyWithFee(actions, []*sdk.Coin{alice.Coin()}, 1, nil); err == nil {
		t.Fatal("expected an error when the coins can't cover the fee")
	}

	if coinSpends := must(clvm.CoinSpends()); len(coinSpends) != 0 {
		t.Fatalf("expected the failed call to leave no coin spends, got %d", len(coinSpends))
	}
}
