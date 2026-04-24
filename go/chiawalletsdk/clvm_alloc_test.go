package chiawalletsdk

import (
	"math/big"
	"testing"
)

func TestAllocInt(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	p, err := clvm.Alloc(ClvmInt(42))
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	n, err := p.Int()
	if err != nil {
		t.Fatal(err)
	}
	val := new(big.Int).SetBytes(n)
	if val.Int64() != 42 {
		t.Fatalf("expected 42, got %d", val.Int64())
	}
}

func TestAllocBigInt(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	big := &big.Int{}
	big.SetString("123456789012345678901234567890", 10)
	p, err := clvm.Alloc(ClvmBigInt{V: big})
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isAtom, _ := p.IsAtom()
	if !isAtom {
		t.Fatal("expected atom")
	}
}

func TestAllocBool(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	p, err := clvm.Alloc(ClvmBool(true))
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	b, err := p.Bool()
	if err != nil {
		t.Fatal(err)
	}
	if b == nil || !*b {
		t.Fatal("expected true")
	}
}

func TestAllocString(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	p, err := clvm.Alloc(ClvmString("hello"))
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	s, err := p.String()
	if err != nil {
		t.Fatal(err)
	}
	if s == nil || *s != "hello" {
		t.Fatalf("expected 'hello', got %v", s)
	}
}

func TestAllocBytes(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	data := []byte{0xca, 0xfe}
	p, err := clvm.Alloc(ClvmBytes(data))
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	atom, err := p.Atom()
	if err != nil {
		t.Fatal(err)
	}
	if len(atom) != 2 || atom[0] != 0xca || atom[1] != 0xfe {
		t.Fatalf("expected [ca fe], got %x", atom)
	}
}

func TestAllocNil(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	p, err := clvm.Alloc(ClvmNil{})
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isNull, _ := p.IsNull()
	if !isNull {
		t.Fatal("expected nil")
	}
}

func TestAllocProgram(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	original, _ := clvm.Nil()
	defer original.Free()

	p, err := clvm.Alloc(original)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isNull, _ := p.IsNull()
	if !isNull {
		t.Fatal("expected nil program")
	}
}

func TestAllocPair(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	first, _ := clvm.Int([]byte{1})
	defer first.Free()
	rest, _ := clvm.Int([]byte{2})
	defer rest.Free()

	pair, _ := NewPair(first, rest)
	defer pair.Free()

	p, err := clvm.Alloc(pair)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected pair")
	}
}

func TestAllocCurriedProgram(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	// (1) as a trivial program
	prog, _ := clvm.Int([]byte{1})
	defer prog.Free()

	arg, _ := clvm.Int([]byte{42})
	defer arg.Free()

	cp, _ := NewCurriedProgram(prog, []*Program{arg})
	defer cp.Free()

	p, err := clvm.Alloc(cp)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected curried program to be a pair")
	}
}

func TestAllocPublicKey(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	pk, _ := sk.PublicKey()
	defer pk.Free()

	p, err := clvm.Alloc(pk)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	atom, _ := p.Atom()
	if len(atom) != 48 {
		t.Fatalf("expected 48-byte BLS public key atom, got %d bytes", len(atom))
	}
}

func TestAllocCreateCoin(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	ph := make([]byte, 32)
	cc, _ := clvm.CreateCoin(ph, 1000, nil)
	defer cc.Free()

	// Alloc the CreateCoin condition back into CLVM
	// First get it as a CreateCoin struct
	ccObj, _ := NewCreateCoin(ph, 1000, nil)
	defer ccObj.Free()

	p, err := clvm.Alloc(ccObj)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected condition to be a pair")
	}
}

func TestAllocReserveFee(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	rf, _ := NewReserveFee(500)
	defer rf.Free()

	p, err := clvm.Alloc(rf)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected condition to be a pair")
	}
}

func TestAllocAssertEphemeral(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	ae, _ := NewAssertEphemeral()
	defer ae.Free()

	p, err := clvm.Alloc(ae)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected condition to be a pair")
	}
}

func TestAllocMeltSingleton(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	ms, _ := NewMeltSingleton()
	defer ms.Free()

	p, err := clvm.Alloc(ms)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected condition to be a pair")
	}
}

func TestAllocClvmList(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	p, err := clvm.Alloc(ClvmList{ClvmInt(1), ClvmInt(2), ClvmInt(3)})
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	// A CLVM proper list is nested pairs: (1 . (2 . (3 . ())))
	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected list to be a pair")
	}

	// Walk the list to verify 3 elements
	cur := p
	count := 0
	for {
		isNull, _ := cur.IsNull()
		if isNull {
			break
		}
		ip, _ := cur.IsPair()
		if !ip {
			break
		}
		first, err := cur.First()
		if err != nil {
			t.Fatal(err)
		}
		first.Free()
		rest, err := cur.Rest()
		if err != nil {
			t.Fatal(err)
		}
		if count > 0 {
			cur.Free()
		}
		cur = rest
		count++
	}
	if count > 0 {
		cur.Free()
	}
	if count != 3 {
		t.Fatalf("expected 3 elements, got %d", count)
	}
}

func TestAllocClvmPairValue(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	p, err := clvm.Alloc(ClvmPairValue{
		First: ClvmString("hello"),
		Rest:  ClvmInt(42),
	})
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected pair")
	}
}

func TestAllocNestedList(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	// Nested structure: [[1, 2], [3, 4]]
	p, err := clvm.Alloc(ClvmList{
		ClvmList{ClvmInt(1), ClvmInt(2)},
		ClvmList{ClvmInt(3), ClvmInt(4)},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected nested list to be a pair")
	}

	// First element should also be a pair (inner list)
	first, _ := p.First()
	defer first.Free()
	firstIsPair, _ := first.IsPair()
	if !firstIsPair {
		t.Fatal("expected inner list to be a pair")
	}
}

func TestAllocPayment(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	ph := make([]byte, 32)
	ph[0] = 0xab
	payment, err := NewPayment(ph, 500, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer payment.Free()

	p, err := clvm.Alloc(payment)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected payment program to be a pair")
	}
}

func TestAllocNotarizedPayment(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	ph := make([]byte, 32)
	payment, _ := NewPayment(ph, 100, nil)
	defer payment.Free()

	nonce := make([]byte, 32)
	nonce[0] = 0x01
	np, err := NewNotarizedPayment(nonce, []*Payment{payment})
	if err != nil {
		t.Fatal(err)
	}
	defer np.Free()

	p, err := clvm.Alloc(np)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected notarized payment program to be a pair")
	}
}

func TestAllocOptionMetadata(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	optType, _ := NewOptionTypeXch(1000)
	defer optType.Free()
	om, err := NewOptionMetadata(3600, optType)
	if err != nil {
		t.Fatal(err)
	}
	defer om.Free()

	p, err := clvm.Alloc(om)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	isPair, _ := p.IsPair()
	if !isPair {
		t.Fatal("expected option metadata program to be a pair")
	}
}

func TestAllocRoundtrip(t *testing.T) {
	clvm, _ := NewClvm()
	defer clvm.Free()

	// Create a condition, alloc it, serialize, verify non-empty
	ph := make([]byte, 32)
	ph[0] = 0xab
	cc, _ := NewCreateCoin(ph, 999, nil)
	defer cc.Free()

	p, err := clvm.Alloc(cc)
	if err != nil {
		t.Fatal(err)
	}
	defer p.Free()

	serialized, err := p.Serialize()
	if err != nil {
		t.Fatal(err)
	}
	if len(serialized) == 0 {
		t.Fatal("expected non-empty serialization")
	}

	// Deserialize and verify it's still a pair
	p2, err := clvm.Deserialize(serialized)
	if err != nil {
		t.Fatal(err)
	}
	defer p2.Free()

	isPair, _ := p2.IsPair()
	if !isPair {
		t.Fatal("expected deserialized condition to be a pair")
	}
}
