package tests

// Test suite mirrors dotnet/tests/BasicTests.cs (the canonical suite) so the Go, C#,
// C++, and Python bindings exercise equivalent behavior.

import (
	"encoding/hex"
	"strconv"
	"testing"

	chia "github.com/xch-dev/chia-wallet-sdk/go/chia_wallet_sdk"
)

func TestToHexFromHexRoundtrip(t *testing.T) {
	bytes, err := chia.FromHex("ff")
	if err != nil {
		t.Fatal(err)
	}
	got, err := chia.ToHex(bytes)
	if err != nil {
		t.Fatal(err)
	}
	if got != "ff" {
		t.Errorf("ToHex(FromHex) = %q, want %q", got, "ff")
	}
}

func TestBytesEqual(t *testing.T) {
	eq, err := chia.BytesEqual([]byte{1, 2, 3}, []byte{1, 2, 3})
	if err != nil {
		t.Fatal(err)
	}
	if !eq {
		t.Error("BytesEqual([1 2 3], [1 2 3]) = false, want true")
	}

	neq, err := chia.BytesEqual([]byte{1, 2, 3}, []byte{1, 2, 4})
	if err != nil {
		t.Fatal(err)
	}
	if neq {
		t.Error("BytesEqual([1 2 3], [1 2 4]) = true, want false")
	}
}

func TestCoinIdKnownValue(t *testing.T) {
	parent, err := chia.FromHex("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a")
	if err != nil {
		t.Fatal(err)
	}
	puzzleHash, err := chia.FromHex("dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986")
	if err != nil {
		t.Fatal(err)
	}
	coin, err := chia.NewCoin(parent, puzzleHash, "100")
	if err != nil {
		t.Fatal(err)
	}
	defer coin.Destroy()

	coinId, err := coin.CoinId()
	if err != nil {
		t.Fatal(err)
	}
	const expected = "fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a"
	if got := hex.EncodeToString(coinId); got != expected {
		t.Errorf("CoinId = %s, want %s", got, expected)
	}
}

func TestAtomRoundtrip(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	expected := []byte{1, 2, 3}
	program, err := clvm.Atom(expected)
	if err != nil {
		t.Fatal(err)
	}
	defer program.Destroy()

	atom, err := program.ToAtom()
	if err != nil {
		t.Fatal(err)
	}
	if atom == nil {
		t.Fatal("ToAtom returned nil")
	}
	if hex.EncodeToString(*atom) != hex.EncodeToString(expected) {
		t.Errorf("ToAtom = %v, want %v", *atom, expected)
	}
}

func TestStringRoundtrip(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	const expected = "hello world"
	program, err := clvm.Atom([]byte(expected))
	if err != nil {
		t.Fatal(err)
	}
	defer program.Destroy()

	str, err := program.ToString()
	if err != nil {
		t.Fatal(err)
	}
	if str == nil {
		t.Fatal("ToString returned nil")
	}
	if *str != expected {
		t.Errorf("ToString = %q, want %q", *str, expected)
	}
}

func TestIntRoundtrip(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	for _, value := range []string{"0", "1", "420", "-1", "-100", "67108863"} {
		program, err := clvm.Int(value)
		if err != nil {
			t.Fatal(err)
		}
		got, err := program.ToInt()
		program.Destroy()
		if err != nil {
			t.Fatal(err)
		}
		if got == nil {
			t.Fatalf("ToInt(%q) returned nil", value)
		}
		if *got != value {
			t.Errorf("ToInt = %q, want %q", *got, value)
		}
	}
}

func TestPairRoundtrip(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	first, err := clvm.Int("1")
	if err != nil {
		t.Fatal(err)
	}
	defer first.Destroy()
	rest, err := clvm.Int("100")
	if err != nil {
		t.Fatal(err)
	}
	defer rest.Destroy()

	pair, err := clvm.Pair(first, rest)
	if err != nil {
		t.Fatal(err)
	}
	defer pair.Destroy()

	result, err := pair.ToPair()
	if err != nil {
		t.Fatal(err)
	}
	if result == nil || *result == nil {
		t.Fatal("ToPair returned nil")
	}
	p := *result
	defer p.Destroy()

	firstProg, err := p.GetFirst()
	if err != nil {
		t.Fatal(err)
	}
	defer firstProg.Destroy()
	firstInt, err := firstProg.ToInt()
	if err != nil {
		t.Fatal(err)
	}
	if firstInt == nil || *firstInt != "1" {
		t.Errorf("pair first = %v, want 1", firstInt)
	}

	restProg, err := p.GetRest()
	if err != nil {
		t.Fatal(err)
	}
	defer restProg.Destroy()
	restInt, err := restProg.ToInt()
	if err != nil {
		t.Fatal(err)
	}
	if restInt == nil || *restInt != "100" {
		t.Errorf("pair rest = %v, want 100", restInt)
	}
}

func TestPublicKeyRoundtrip(t *testing.T) {
	original, err := chia.PublicKeyInfinity()
	if err != nil {
		t.Fatal(err)
	}
	defer original.Destroy()

	bytes, err := original.ToBytes()
	if err != nil {
		t.Fatal(err)
	}
	restored, err := chia.PublicKeyFromBytes(bytes)
	if err != nil {
		t.Fatal(err)
	}
	defer restored.Destroy()

	restoredBytes, err := restored.ToBytes()
	if err != nil {
		t.Fatal(err)
	}
	if hex.EncodeToString(bytes) != hex.EncodeToString(restoredBytes) {
		t.Error("public key roundtrip mismatch")
	}
}

func TestClvmSerialization(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	atom123, err := clvm.Atom([]byte{1, 2, 3})
	if err != nil {
		t.Fatal(err)
	}
	defer atom123.Destroy()
	int420, err := clvm.Int("420")
	if err != nil {
		t.Fatal(err)
	}
	defer int420.Destroy()
	int100, err := clvm.Int("100")
	if err != nil {
		t.Fatal(err)
	}
	defer int100.Destroy()
	pairAtomInt, err := func() (*chia.Program, error) {
		a, err := clvm.Atom([]byte{1, 2, 3})
		if err != nil {
			return nil, err
		}
		defer a.Destroy()
		b, err := clvm.Int("100")
		if err != nil {
			return nil, err
		}
		defer b.Destroy()
		return clvm.Pair(a, b)
	}()
	if err != nil {
		t.Fatal(err)
	}
	defer pairAtomInt.Destroy()

	cases := []struct {
		program *chia.Program
		hex     string
	}{
		{atom123, "83010203"},
		{int420, "8201a4"},
		{int100, "64"},
		{pairAtomInt, "ff8301020364"},
	}

	for _, c := range cases {
		serialized, err := c.program.Serialize()
		if err != nil {
			t.Fatal(err)
		}
		if got := hex.EncodeToString(serialized); got != c.hex {
			t.Errorf("Serialize = %s, want %s", got, c.hex)
		}
		deserialized, err := clvm.Deserialize(serialized)
		if err != nil {
			t.Fatal(err)
		}
		origHash, err := c.program.TreeHash()
		if err != nil {
			t.Fatal(err)
		}
		deserHash, err := deserialized.TreeHash()
		deserialized.Destroy()
		if err != nil {
			t.Fatal(err)
		}
		if hex.EncodeToString(origHash) != hex.EncodeToString(deserHash) {
			t.Error("tree hash mismatch after roundtrip")
		}
	}
}

func TestCurryRoundtrip(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	items := make([]*chia.Program, 0, 10)
	for i := 0; i < 10; i++ {
		item, err := clvm.Int(strconv.Itoa(i))
		if err != nil {
			t.Fatal(err)
		}
		defer item.Destroy()
		items = append(items, item)
	}

	nilProg, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer nilProg.Destroy()

	curried, err := nilProg.Curry(items)
	if err != nil {
		t.Fatal(err)
	}
	defer curried.Destroy()

	uncurried, err := curried.Uncurry()
	if err != nil {
		t.Fatal(err)
	}
	if uncurried == nil || *uncurried == nil {
		t.Fatal("Uncurry returned nil")
	}
	uc := *uncurried
	defer uc.Destroy()

	ucProgram, err := uc.GetProgram()
	if err != nil {
		t.Fatal(err)
	}
	defer ucProgram.Destroy()
	nilHash, err := nilProg.TreeHash()
	if err != nil {
		t.Fatal(err)
	}
	ucHash, err := ucProgram.TreeHash()
	if err != nil {
		t.Fatal(err)
	}
	if hex.EncodeToString(nilHash) != hex.EncodeToString(ucHash) {
		t.Error("uncurried program hash mismatch")
	}

	args, err := uc.GetArgs()
	if err != nil {
		t.Fatal(err)
	}
	if len(args) != 10 {
		t.Fatalf("uncurried args = %d, want 10", len(args))
	}
	for i, arg := range args {
		v, err := arg.ToInt()
		arg.Destroy()
		if err != nil {
			t.Fatal(err)
		}
		if v == nil || *v != strconv.Itoa(i) {
			t.Errorf("uncurried arg %d = %v, want %s", i, v, strconv.Itoa(i))
		}
	}
}

func TestAlloc(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	nilProg, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer nilProg.Destroy()

	pk, err := chia.PublicKeyInfinity()
	if err != nil {
		t.Fatal(err)
	}
	pkProg, err := clvm.Alloc(chia.ClvmTypePublicKey{Value: pk})
	if err != nil {
		t.Fatal(err)
	}
	defer pkProg.Destroy()

	helloProg, err := clvm.Atom([]byte("Hello, world!"))
	if err != nil {
		t.Fatal(err)
	}
	defer helloProg.Destroy()

	fortyTwo, err := clvm.Int("42")
	if err != nil {
		t.Fatal(err)
	}
	defer fortyTwo.Destroy()

	hundred, err := clvm.Int("100")
	if err != nil {
		t.Fatal(err)
	}
	defer hundred.Destroy()

	trueProg, err := clvm.Bool(true)
	if err != nil {
		t.Fatal(err)
	}
	defer trueProg.Destroy()

	atomProg, err := clvm.Atom([]byte{1, 2, 3})
	if err != nil {
		t.Fatal(err)
	}
	defer atomProg.Destroy()

	zeroesProg, err := clvm.Atom(make([]byte, 32))
	if err != nil {
		t.Fatal(err)
	}
	defer zeroesProg.Destroy()

	nil2, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer nil2.Destroy()

	nil3, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer nil3.Destroy()

	rcNil1, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer rcNil1.Destroy()
	rcNil2, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer rcNil2.Destroy()
	runCatTail, err := chia.NewRunCatTail(rcNil1, rcNil2)
	if err != nil {
		t.Fatal(err)
	}
	defer runCatTail.Destroy()
	runCatTailProg, err := clvm.Alloc(chia.ClvmTypeRunCatTail{Value: runCatTail})
	if err != nil {
		t.Fatal(err)
	}
	defer runCatTailProg.Destroy()

	program, err := clvm.List([]*chia.Program{
		nilProg,
		pkProg,
		helloProg,
		fortyTwo,
		hundred,
		trueProg,
		atomProg,
		zeroesProg,
		nil2,
		nil3,
		runCatTailProg,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer program.Destroy()

	serialized, err := program.Serialize()
	if err != nil {
		t.Fatal(err)
	}

	const expected = "ff80ffb0c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff818fff80ff808080"
	got := hex.EncodeToString(serialized)
	if got != expected {
		t.Errorf("serialization mismatch\ngot:  %s\nwant: %s", got, expected)
	}
}

func TestCreateAndParseCondition(t *testing.T) {
	clvm, err := chia.NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Destroy()

	puzzleHash := make([]byte, 32)
	for i := range puzzleHash {
		puzzleHash[i] = 0xff
	}

	atom, err := clvm.Atom(puzzleHash)
	if err != nil {
		t.Fatal(err)
	}
	defer atom.Destroy()
	memos, err := clvm.List([]*chia.Program{atom})
	if err != nil {
		t.Fatal(err)
	}
	defer memos.Destroy()

	condition, err := clvm.CreateCoin(puzzleHash, "1", &memos)
	if err != nil {
		t.Fatal(err)
	}
	defer condition.Destroy()

	parsed, err := condition.ParseCreateCoin()
	if err != nil {
		t.Fatal(err)
	}
	if parsed == nil || *parsed == nil {
		t.Fatal("ParseCreateCoin returned nil")
	}
	cc := *parsed
	defer cc.Destroy()

	parsedPh, err := cc.GetPuzzleHash()
	if err != nil {
		t.Fatal(err)
	}
	if hex.EncodeToString(parsedPh) != hex.EncodeToString(puzzleHash) {
		t.Error("parsed puzzle hash mismatch")
	}
	amount, err := cc.GetAmount()
	if err != nil {
		t.Fatal(err)
	}
	if amount != "1" {
		t.Errorf("parsed amount = %q, want 1", amount)
	}
}

// itoa is a tiny helper to avoid importing strconv for a single use.
func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	digits := ""
	for i > 0 {
		digits = string(rune('0'+i%10)) + digits
		i /= 10
	}
	return digits
}
