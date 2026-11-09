package chiawalletsdk_test

import (
	"bytes"
	"encoding/hex"
	"math"
	"math/big"
	"strconv"
	"strings"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// Ported from the regression tests for the Go bindings in #370, which cover conversions between
// Go and Rust values at their edges.

func TestIntegerEncoding(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	for _, c := range []struct {
		value string
		atom  string
	}{
		{strconv.FormatInt(math.MinInt64, 10), "8000000000000000"},
		{strconv.FormatInt(-(1<<53 + 1), 10), "dfffffffffffff"},
		{"-256", "ff00"},
		{"-129", "ff7f"},
		{"-128", "80"},
		{"-1", "ff"},
		{"0", ""},
		{"127", "7f"},
		{"128", "0080"},
		{"255", "00ff"},
		{strconv.FormatInt(1<<53+1, 10), "20000000000001"},
		{strconv.FormatInt(math.MaxInt64, 10), "7fffffffffffffff"},
		{strconv.FormatUint(math.MaxUint64, 10), "00ffffffffffffffff"},
	} {
		for _, program := range []*sdk.Program{
			must(clvm.Int(c.value)),
			must(clvm.Alloc(sdk.ClvmTypeInt{Value: c.value})),
		} {
			if value := must(program.ToInt()); value == nil || *value != c.value {
				t.Fatalf("expected %s, got %v", c.value, value)
			}

			if atom := must(program.ToAtom()); atom == nil || hex.EncodeToString(*atom) != c.atom {
				t.Fatalf("expected %s to encode as %q, got %v", c.value, c.atom, atom)
			}

			program.Destroy()
		}
	}

	for _, invalid := range []string{"", "1.5", "0x10", " 1"} {
		if _, err := clvm.Alloc(sdk.ClvmTypeInt{Value: invalid}); err == nil {
			t.Fatalf("expected an error for %q", invalid)
		}
	}
}

func TestAmountsBeyondUint64(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	spends := must(sdk.NewSpends(clvm, make([]byte, 32)))
	defer spends.Destroy()

	for i := range 2 {
		coin := must(sdk.NewCoin(seed(byte(i)), make([]byte, 32), math.MaxUint64))
		check(t, spends.AddXch(coin))
		coin.Destroy()
	}

	want := new(big.Int).Lsh(new(big.Int).SetUint64(math.MaxUint64), 1).String()

	if got := must(spends.SelectedXchAmount()); got != want {
		t.Fatalf("expected a selected amount of %s, got %s", want, got)
	}

	amounts := must(sdk.NewOfferAmounts())
	defer amounts.Destroy()

	check(t, amounts.SetXch(want))

	if got := must(amounts.Xch()); got != want {
		t.Fatalf("expected an offered amount of %s, got %s", want, got)
	}

	maxU128 := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))
	check(t, amounts.SetXch(maxU128.String()))

	for _, invalid := range []string{"-1", new(big.Int).Add(maxU128, big.NewInt(1)).String(), "abc"} {
		if err := amounts.SetXch(invalid); err == nil {
			t.Fatalf("expected an error for an amount of %s", invalid)
		}
	}
}

func TestForce1Of2WrapperMemoParse(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	memo := must(sdk.WrapperMemoForce1Of2RestrictedVariable(clvm, make([]byte, 32), 7, nil))
	defer memo.Destroy()

	ctx := must(sdk.NewMipsMemoContext())
	defer ctx.Destroy()

	parsed := must(memo.Parse(ctx))
	if parsed == nil {
		t.Fatal("expected the memo to parse")
	}
	defer (*parsed).Destroy()

	recovery := must((*parsed).AsForce1Of2RestrictedVariable())
	if recovery == nil {
		t.Fatal("expected a force 1 of 2 restricted variable memo")
	}
	defer (*recovery).Destroy()

	if nonce := (*recovery).Nonce(); nonce != 7 {
		t.Fatalf("expected a nonce of 7, got %d", nonce)
	}

	if restrictions := (*recovery).Restrictions(); len(restrictions) != 0 {
		t.Fatalf("expected no restrictions, got %d", len(restrictions))
	}

	if timelock := must((*parsed).AsTimelock()); timelock != nil {
		t.Fatalf("expected no timelock, got %d", *timelock)
	}
}

func TestOptionalNumericValues(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	number := must(clvm.BoundCheckedNumber(42))
	if value := must(number.ToBoundCheckedNumber()); value == nil || *value != 42 {
		t.Fatalf("expected 42, got %v", value)
	}

	memo := must(sdk.WrapperMemoPreventConditionOpcode(clvm, 60, true))
	defer memo.Destroy()

	ctx := must(sdk.NewMipsMemoContext())
	defer ctx.Destroy()

	parsed := must(memo.Parse(ctx))
	if parsed == nil {
		t.Fatal("expected the memo to parse")
	}
	defer (*parsed).Destroy()

	if opcode := must((*parsed).AsPreventConditionOpcode()); opcode == nil || *opcode != 60 {
		t.Fatalf("expected opcode 60, got %v", opcode)
	}
}

func TestEmptyCollections(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	for _, value := range [][]byte{nil, {}} {
		program := must(clvm.Alloc(sdk.ClvmTypeAtom{Value: value}))
		if !must(program.IsNull()) {
			t.Fatalf("expected %#v to allocate nil", value)
		}
		program.Destroy()
	}

	// An absent optional list is different from a present empty one
	absent := must(sdk.NewGetBlockSpendsResponse(nil, nil, true))
	defer absent.Destroy()

	if absent.BlockSpends() != nil {
		t.Fatal("expected no block spends")
	}

	empty := []*sdk.CoinSpend{}
	present := must(sdk.NewGetBlockSpendsResponse(&empty, nil, true))
	defer present.Destroy()

	if spends := present.BlockSpends(); spends == nil || len(*spends) != 0 {
		t.Fatalf("expected an empty list of block spends, got %v", spends)
	}

	memos := [][]byte{nil, {}, bytes.Repeat([]byte{7}, 64)}
	update := must(sdk.NewUpdateDatastoreMerkleRoot(make([]byte, 32), memos))
	defer update.Destroy()

	got := update.Memos()
	if len(got) != len(memos) {
		t.Fatalf("expected %d memos, got %d", len(memos), len(got))
	}

	for i := range memos {
		if !bytes.Equal(got[i], memos[i]) {
			t.Fatalf("memo %d: expected %x, got %x", i, memos[i], got[i])
		}
	}

	// A present but empty hash is invalid, so it must not be treated as absent
	emptyHash := []byte{}
	if metadata, err := sdk.NewHandleNftMetadata(nil, nil, &emptyHash, nil, nil, nil, nil); err == nil {
		metadata.Destroy()
		t.Fatal("expected an error for an empty hash")
	}
}

func TestStringsWithNul(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	long := strings.Repeat("x\x00", 64)

	for _, value := range []string{"", "a\x00b", "雪\x00🌱", long} {
		program := must(clvm.String(value))

		if got := must(program.ToString()); got == nil || *got != value {
			t.Fatalf("expected %q, got %v", value, got)
		}

		if atom := must(program.ToAtom()); atom == nil || string(*atom) != value {
			t.Fatalf("expected the atom %q, got %v", value, atom)
		}

		program.Destroy()

		address := must(sdk.NewAddress(make([]byte, 32), value))
		if prefix := address.Prefix(); prefix != value {
			t.Fatalf("expected the prefix %q, got %q", value, prefix)
		}
		address.Destroy()

		metadata := must(sdk.NewHandleNftMetadata(&value, []string{value}, nil, nil, nil, nil, nil))
		if name := metadata.DisplayName(); name == nil || *name != value {
			t.Fatalf("expected the display name %q, got %v", value, name)
		}

		if uris := metadata.ImageUris(); len(uris) != 1 || uris[0] != value {
			t.Fatalf("expected the image uris [%q], got %q", value, uris)
		}
		metadata.Destroy()
	}

	// Bytes that aren't valid UTF-8 can be allocated as an atom, but can't be read back as text
	atom := must(clvm.Alloc(sdk.ClvmTypeAtom{Value: []byte{0xff}}))
	defer atom.Destroy()

	if _, err := atom.ToString(); err == nil {
		t.Fatal("expected an error reading invalid UTF-8 as a string")
	}
}

func TestInvalidUtf8StringIsAnError(t *testing.T) {
	t.Skip("UniFFI lifts strings with String::from_utf8_unchecked, so a Go string that isn't valid " +
		"UTF-8 reaches Rust as an invalid String instead of being rejected")

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	if program, err := clvm.String(string([]byte{0xff})); err == nil {
		program.Destroy()
		t.Fatal("expected an error for a string that isn't valid UTF-8")
	}
}
