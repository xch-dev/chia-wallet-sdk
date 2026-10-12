package chiawalletsdk_test

import (
	"bytes"
	"encoding/hex"
	"fmt"
	"strings"
	"sync"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// must unwraps a result, panicking on errors (which fails the test that called it).
func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

func check(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func fromHex(value string) []byte {
	return must(hex.DecodeString(value))
}

func TestCoinId(t *testing.T) {
	coin := must(sdk.NewCoin(
		fromHex("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
		fromHex("dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986"),
		100,
	))
	defer coin.Destroy()

	expected := fromHex("fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a")
	if coinID := must(coin.CoinId()); !bytes.Equal(coinID, expected) {
		t.Fatalf("expected coin id %x, got %x", expected, coinID)
	}
}

func TestSettersMutateInPlace(t *testing.T) {
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 100))
	defer coin.Destroy()

	clone := coin.Clone()
	defer clone.Destroy()

	check(t, coin.SetAmount(18_446_744_073_709_551_615))

	if amount := coin.Amount(); amount != 18_446_744_073_709_551_615 {
		t.Fatalf("expected the maximum u64 amount, got %d", amount)
	}

	if amount := clone.Amount(); amount != 100 {
		t.Fatalf("expected the clone to be unchanged, got %d", amount)
	}
}

func TestWrongLengthIsAnError(t *testing.T) {
	_, err := sdk.NewCoin(make([]byte, 31), make([]byte, 32), 100)
	if err == nil {
		t.Fatal("expected an error for a 31 byte parent coin id")
	}

	if !strings.Contains(err.Error(), "expected 32 bytes, found 31") {
		t.Fatalf("unexpected error message: %v", err)
	}
}

func TestAtomRoundtrip(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	expected := []byte{1, 2, 3}
	atom := must(clvm.Alloc(sdk.ClvmTypeAtom{Value: expected}))
	defer atom.Destroy()

	if value := must(atom.ToAtom()); value == nil || !bytes.Equal(*value, expected) {
		t.Fatalf("expected %x, got %v", expected, value)
	}
}

func TestStringRoundtrip(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	program := must(clvm.Alloc(sdk.ClvmTypeText{Value: "hello world"}))
	defer program.Destroy()

	if value := must(program.ToString()); value == nil || *value != "hello world" {
		t.Fatalf("unexpected string %v", value)
	}
}

func TestBigIntRoundtrip(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	for _, expected := range []string{
		"0",
		"1",
		"420",
		"67108863",
		"-1",
		"-100",
		"-421489719874198729487129847",
		"4384723984791283749823764732649187498237483927482",
	} {
		program := must(clvm.Int(expected))
		if value := must(program.ToInt()); value == nil || *value != expected {
			t.Fatalf("expected %s, got %v", expected, value)
		}
		program.Destroy()
	}

	if _, err := clvm.Int("not a number"); err == nil {
		t.Fatal("expected an error for an invalid integer")
	}
}

func TestPairRoundtrip(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(clvm.Pair(must(clvm.BoundCheckedNumber(1)), must(clvm.Int("100"))))
	defer pair.Destroy()

	value := must(pair.ToPair())
	if value == nil {
		t.Fatal("expected a pair")
	}

	if first := must((*value).First().ToBoundCheckedNumber()); first == nil || *first != 1 {
		t.Fatalf("unexpected first value %v", first)
	}

	if rest := must((*value).Rest().ToInt()); rest == nil || *rest != "100" {
		t.Fatalf("unexpected rest value %v", rest)
	}
}

func TestClvmValueAllocation(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pk := must(sdk.PublicKeyInfinity())
	defer pk.Destroy()

	cases := []struct {
		value    sdk.ClvmType
		expected *sdk.Program
	}{
		{sdk.ClvmTypeNil{}, must(clvm.Nil())},
		{sdk.ClvmTypeBool{Value: true}, must(clvm.Bool(true))},
		{sdk.ClvmTypeInt{Value: "-42"}, must(clvm.Int("-42"))},
		{sdk.ClvmTypeAtom{Value: []byte{1, 2, 3}}, must(clvm.Atom([]byte{1, 2, 3}))},
		{sdk.ClvmTypeText{Value: "Hello, world!"}, must(clvm.String("Hello, world!"))},
		{sdk.ClvmTypePublicKey{Value: pk}, must(clvm.Atom(must(pk.ToBytes())))},
	}

	for _, c := range cases {
		actual := must(clvm.Alloc(c.value))
		if !bytes.Equal(must(actual.TreeHash()), must(c.expected.TreeHash())) {
			t.Errorf("unexpected allocation for %#v", c.value)
		}
	}
}

func TestConditionAllocation(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	puzzleHash := make([]byte, 32)

	condition := must(sdk.NewCreateCoin(puzzleHash, 1000, nil))
	defer condition.Destroy()

	allocated := must(clvm.Alloc(sdk.ClvmTypeCreateCoin{Value: condition}))
	direct := must(clvm.CreateCoin(puzzleHash, 1000, nil))

	if !bytes.Equal(must(allocated.TreeHash()), must(direct.TreeHash())) {
		t.Fatal("expected the allocated condition to match the direct one")
	}

	parsed := must(allocated.ParseCreateCoin())
	if parsed == nil || (*parsed).Amount() != 1000 {
		t.Fatal("expected to parse the condition back")
	}
}

func TestCurryAddFunction(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	addMod := must(clvm.Deserialize(fromHex("ff10ff02ff0580")))
	addToTen := must(addMod.Curry([]*sdk.Program{must(clvm.Int("10"))}))
	solution := must(clvm.List([]*sdk.Program{must(clvm.Int("5"))}))
	output := must(addToTen.Run(solution, 10_000_000, true))
	defer output.Destroy()

	if value := must(output.Value().ToInt()); value == nil || *value != "15" {
		t.Fatalf("unexpected result %v", value)
	}

	if cost := output.Cost(); cost != 1082 {
		t.Fatalf("unexpected cost %d", cost)
	}
}

func TestStaticMethods(t *testing.T) {
	program := must(sdk.ConstantsP2DelegatedPuzzleOrHiddenPuzzle())
	hash := must(sdk.ConstantsP2DelegatedPuzzleOrHiddenPuzzleHash())

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	if !bytes.Equal(must(must(clvm.Deserialize(program)).TreeHash()), hash) {
		t.Fatal("expected the standard puzzle to match its hash")
	}
}

func TestMnemonic(t *testing.T) {
	mnemonic := must(sdk.MnemonicFromEntropy(make([]byte, 32)))
	defer mnemonic.Destroy()

	phrase := must(mnemonic.ToString())
	if !must(sdk.MnemonicVerify(phrase)) {
		t.Fatalf("expected %q to be a valid mnemonic", phrase)
	}

	parsed := must(sdk.NewMnemonic(phrase))
	defer parsed.Destroy()

	if !bytes.Equal(must(parsed.ToEntropy()), make([]byte, 32)) {
		t.Fatal("expected the entropy to roundtrip")
	}
}

func TestAddressRoundtrip(t *testing.T) {
	puzzleHash := bytes.Repeat([]byte{0xab}, 32)

	address := must(sdk.NewAddress(puzzleHash, "xch"))
	defer address.Destroy()

	encoded := must(address.Encode())
	decoded := must(sdk.AddressDecode(encoded))
	defer decoded.Destroy()

	if decoded.Prefix() != "xch" || !bytes.Equal(decoded.PuzzleHash(), puzzleHash) {
		t.Fatalf("unexpected decoded address %s", encoded)
	}
}

func TestSimulatorTransaction(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	alice := must(sim.Bls(1000))
	defer alice.Destroy()

	conditions := []*sdk.Program{
		must(clvm.CreateCoin(alice.PuzzleHash(), 900, nil)),
		must(clvm.ReserveFee(100)),
	}

	spend := must(clvm.DelegatedSpend(conditions))
	check(t, clvm.SpendStandardCoin(alice.Coin(), alice.Pk(), spend))
	check(t, sim.SpendCoins(must(clvm.CoinSpends()), []*sdk.SecretKey{alice.Sk()}))

	coins := must(sim.UnspentCoins(alice.PuzzleHash(), false))
	if len(coins) != 1 || coins[0].Amount() != 900 {
		t.Fatalf("expected a single 900 mojo coin, got %d coins", len(coins))
	}
}

func TestConcurrentUse(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	var wg sync.WaitGroup
	errs := make(chan error, 32)

	for i := range 32 {
		wg.Add(1)
		go func() {
			defer wg.Done()

			program, err := clvm.Int(fmt.Sprint(i))
			if err != nil {
				errs <- err
				return
			}
			defer program.Destroy()

			value, err := program.ToInt()
			if err != nil {
				errs <- err
				return
			}

			if value == nil || *value != fmt.Sprint(i) {
				errs <- fmt.Errorf("expected %d, got %v", i, value)
			}
		}()
	}

	wg.Wait()
	close(errs)

	for err := range errs {
		t.Error(err)
	}
}

func TestRpcAgainstFullNodeSimulator(t *testing.T) {
	sim := must(sdk.NewFullNodeSimulator())
	defer sim.Destroy()

	// Async methods block the calling goroutine until the Rust future completes
	server := must(sim.StartServer())
	defer func() { check(t, server.Close()) }()

	rpc := must(sdk.NewRpcClient(server.Url()))
	defer rpc.Destroy()

	info := must(rpc.GetNetworkInfo())
	defer info.Destroy()

	if !info.Success() {
		t.Fatal("expected the network info request to succeed")
	}

	if name := info.NetworkName(); name == nil || *name != "simulator0" {
		t.Fatalf("unexpected network name %v", name)
	}

	state := must(rpc.GetBlockchainState())
	defer state.Destroy()

	if !state.Success() || state.BlockchainState() == nil {
		t.Fatal("expected the blockchain state request to succeed")
	}
}
