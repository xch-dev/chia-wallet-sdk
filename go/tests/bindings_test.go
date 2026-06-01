package tests

import (
	"encoding/hex"
	"testing"

	chia "github.com/xch-dev/chia-wallet-sdk/go/chia_wallet_sdk"
)

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

	helloProg, err := clvm.String("Hello, world!")
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
