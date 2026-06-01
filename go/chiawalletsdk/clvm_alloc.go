package chiawalletsdk

import "math/big"

// ClvmValue represents any value that can be allocated into the CLVM.
// Implemented by all binding types that correspond to CLVM-allocatable values,
// plus primitive wrapper types for int, bool, string, bytes, and nil.
type ClvmValue interface {
	clvmAlloc(c *Clvm) (*Program, error)
}

// ── Primitive wrapper types ─────────────────────────────────────────────

// ClvmInt wraps an int64 as a CLVM value.
type ClvmInt int64

func (v ClvmInt) clvmAlloc(c *Clvm) (*Program, error) {
	return c.BoundCheckedNumber(float64(v))
}

// ClvmBigInt wraps a *big.Int as a CLVM value.
type ClvmBigInt struct{ V *big.Int }

func (v ClvmBigInt) clvmAlloc(c *Clvm) (*Program, error) {
	b := v.V.Bytes()
	if v.V.Sign() < 0 {
		// Two's complement for negative BigInts: negate, subtract 1, flip bits
		neg := new(big.Int).Neg(v.V)
		neg.Sub(neg, big.NewInt(1))
		raw := neg.Bytes()
		for i := range raw {
			raw[i] = ^raw[i]
		}
		// Ensure high bit is set (negative)
		if len(raw) == 0 || raw[0]&0x80 == 0 {
			raw = append([]byte{0xff}, raw...)
		}
		b = raw
	} else if len(b) > 0 && b[0]&0x80 != 0 {
		// Positive but high bit set — prepend zero byte
		b = append([]byte{0x00}, b...)
	}
	return c.Int(b)
}

// ClvmBool wraps a bool as a CLVM value.
type ClvmBool bool

func (v ClvmBool) clvmAlloc(c *Clvm) (*Program, error) {
	return c.Bool(bool(v))
}

// ClvmString wraps a string as a CLVM value.
type ClvmString string

func (v ClvmString) clvmAlloc(c *Clvm) (*Program, error) {
	return c.String(string(v))
}

// ClvmBytes wraps a byte slice as a CLVM atom.
type ClvmBytes []byte

func (v ClvmBytes) clvmAlloc(c *Clvm) (*Program, error) {
	return c.Atom([]byte(v))
}

// ClvmNil represents a CLVM nil value.
type ClvmNil struct{}

func (v ClvmNil) clvmAlloc(c *Clvm) (*Program, error) {
	return c.Nil()
}

// ClvmList wraps a slice of ClvmValues as a CLVM proper list.
type ClvmList []ClvmValue

func (v ClvmList) clvmAlloc(c *Clvm) (*Program, error) {
	programs := make([]*Program, len(v))
	for i, item := range v {
		p, err := c.Alloc(item)
		if err != nil {
			// Free already-allocated programs on error
			for j := 0; j < i; j++ {
				programs[j].Free()
			}
			return nil, err
		}
		programs[i] = p
	}
	result, err := c.List(programs)
	if err != nil {
		for _, p := range programs {
			p.Free()
		}
		return nil, err
	}
	return result, nil
}

// ClvmPairValue wraps two ClvmValues as a CLVM cons pair.
type ClvmPairValue struct {
	First ClvmValue
	Rest  ClvmValue
}

func (v ClvmPairValue) clvmAlloc(c *Clvm) (*Program, error) {
	first, err := c.Alloc(v.First)
	if err != nil {
		return nil, err
	}
	rest, err := c.Alloc(v.Rest)
	if err != nil {
		first.Free()
		return nil, err
	}
	result, err := c.Pair(first, rest)
	if err != nil {
		first.Free()
		rest.Free()
		return nil, err
	}
	return result, nil
}

// ── Alloc method ────────────────────────────────────────────────────────

// Alloc allocates a value into the CLVM, returning a Program.
// Accepts any ClvmValue: binding types (Program, PublicKey, CreateCoin, etc.),
// primitive wrappers (ClvmInt, ClvmString, ClvmBytes, ClvmBool, ClvmNil),
// and composite types (ClvmList, ClvmPairValue).
func (c *Clvm) Alloc(value ClvmValue) (*Program, error) {
	return value.clvmAlloc(c)
}

// ── Interface implementations for binding types ─────────────────────────

// Program is already a CLVM value — clone it.
func (o *Program) clvmAlloc(c *Clvm) (*Program, error) {
	return o.Clone()
}

// Pair allocates as a cons pair.
func (o *Pair) clvmAlloc(c *Clvm) (*Program, error) {
	first, err := o.First()
	if err != nil {
		return nil, err
	}
	defer first.Free()
	rest, err := o.Rest()
	if err != nil {
		return nil, err
	}
	defer rest.Free()
	return c.Pair(first, rest)
}

// CurriedProgram allocates by currying args into the program.
func (o *CurriedProgram) clvmAlloc(c *Clvm) (*Program, error) {
	prog, err := o.Program()
	if err != nil {
		return nil, err
	}
	defer prog.Free()
	args, err := o.Args()
	if err != nil {
		return nil, err
	}
	defer func() {
		for _, a := range args {
			a.Free()
		}
	}()
	return prog.Curry(args)
}

// ── Key types → atom ────────────────────────────────────────────────────

func (o *PublicKey) clvmAlloc(c *Clvm) (*Program, error) {
	b, err := o.Bytes()
	if err != nil {
		return nil, err
	}
	return c.Atom(b)
}

func (o *Signature) clvmAlloc(c *Clvm) (*Program, error) {
	b, err := o.Bytes()
	if err != nil {
		return nil, err
	}
	return c.Atom(b)
}

func (o *K1PublicKey) clvmAlloc(c *Clvm) (*Program, error) {
	b, err := o.Bytes()
	if err != nil {
		return nil, err
	}
	return c.Atom(b)
}

func (o *K1Signature) clvmAlloc(c *Clvm) (*Program, error) {
	b, err := o.Bytes()
	if err != nil {
		return nil, err
	}
	return c.Atom(b)
}

func (o *R1PublicKey) clvmAlloc(c *Clvm) (*Program, error) {
	b, err := o.Bytes()
	if err != nil {
		return nil, err
	}
	return c.Atom(b)
}

func (o *R1Signature) clvmAlloc(c *Clvm) (*Program, error) {
	b, err := o.Bytes()
	if err != nil {
		return nil, err
	}
	return c.Atom(b)
}

// ── Condition types ─────────────────────────────────────────────────────

func (o *Remark) clvmAlloc(c *Clvm) (*Program, error) {
	rest, err := o.Rest()
	if err != nil {
		return nil, err
	}
	defer rest.Free()
	return c.Remark(rest)
}

func (o *AggSigParent) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigParent(pk, msg)
}

func (o *AggSigPuzzle) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigPuzzle(pk, msg)
}

func (o *AggSigAmount) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigAmount(pk, msg)
}

func (o *AggSigPuzzleAmount) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigPuzzleAmount(pk, msg)
}

func (o *AggSigParentAmount) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigParentAmount(pk, msg)
}

func (o *AggSigParentPuzzle) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigParentPuzzle(pk, msg)
}

func (o *AggSigUnsafe) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigUnsafe(pk, msg)
}

func (o *AggSigMe) clvmAlloc(c *Clvm) (*Program, error) {
	pk, err := o.PublicKey()
	if err != nil {
		return nil, err
	}
	defer pk.Free()
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.AggSigMe(pk, msg)
}

func (o *CreateCoin) clvmAlloc(c *Clvm) (*Program, error) {
	ph, err := o.PuzzleHash()
	if err != nil {
		return nil, err
	}
	amt, err := o.Amount()
	if err != nil {
		return nil, err
	}
	memos, err := o.Memos()
	if err != nil {
		return nil, err
	}
	if memos != nil {
		defer memos.Free()
	}
	return c.CreateCoin(ph, amt, memos)
}

func (o *ReserveFee) clvmAlloc(c *Clvm) (*Program, error) {
	amt, err := o.Amount()
	if err != nil {
		return nil, err
	}
	return c.ReserveFee(amt)
}

func (o *CreateCoinAnnouncement) clvmAlloc(c *Clvm) (*Program, error) {
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.CreateCoinAnnouncement(msg)
}

func (o *CreatePuzzleAnnouncement) clvmAlloc(c *Clvm) (*Program, error) {
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	return c.CreatePuzzleAnnouncement(msg)
}

func (o *AssertCoinAnnouncement) clvmAlloc(c *Clvm) (*Program, error) {
	id, err := o.AnnouncementId()
	if err != nil {
		return nil, err
	}
	return c.AssertCoinAnnouncement(id)
}

func (o *AssertPuzzleAnnouncement) clvmAlloc(c *Clvm) (*Program, error) {
	id, err := o.AnnouncementId()
	if err != nil {
		return nil, err
	}
	return c.AssertPuzzleAnnouncement(id)
}

func (o *AssertConcurrentSpend) clvmAlloc(c *Clvm) (*Program, error) {
	id, err := o.CoinId()
	if err != nil {
		return nil, err
	}
	return c.AssertConcurrentSpend(id)
}

func (o *AssertConcurrentPuzzle) clvmAlloc(c *Clvm) (*Program, error) {
	ph, err := o.PuzzleHash()
	if err != nil {
		return nil, err
	}
	return c.AssertConcurrentPuzzle(ph)
}

func (o *AssertSecondsRelative) clvmAlloc(c *Clvm) (*Program, error) {
	s, err := o.Seconds()
	if err != nil {
		return nil, err
	}
	return c.AssertSecondsRelative(s)
}

func (o *AssertSecondsAbsolute) clvmAlloc(c *Clvm) (*Program, error) {
	s, err := o.Seconds()
	if err != nil {
		return nil, err
	}
	return c.AssertSecondsAbsolute(s)
}

func (o *AssertHeightRelative) clvmAlloc(c *Clvm) (*Program, error) {
	h, err := o.Height()
	if err != nil {
		return nil, err
	}
	return c.AssertHeightRelative(h)
}

func (o *AssertHeightAbsolute) clvmAlloc(c *Clvm) (*Program, error) {
	h, err := o.Height()
	if err != nil {
		return nil, err
	}
	return c.AssertHeightAbsolute(h)
}

func (o *AssertBeforeSecondsRelative) clvmAlloc(c *Clvm) (*Program, error) {
	s, err := o.Seconds()
	if err != nil {
		return nil, err
	}
	return c.AssertBeforeSecondsRelative(s)
}

func (o *AssertBeforeSecondsAbsolute) clvmAlloc(c *Clvm) (*Program, error) {
	s, err := o.Seconds()
	if err != nil {
		return nil, err
	}
	return c.AssertBeforeSecondsAbsolute(s)
}

func (o *AssertBeforeHeightRelative) clvmAlloc(c *Clvm) (*Program, error) {
	h, err := o.Height()
	if err != nil {
		return nil, err
	}
	return c.AssertBeforeHeightRelative(h)
}

func (o *AssertBeforeHeightAbsolute) clvmAlloc(c *Clvm) (*Program, error) {
	h, err := o.Height()
	if err != nil {
		return nil, err
	}
	return c.AssertBeforeHeightAbsolute(h)
}

func (o *AssertMyCoinId) clvmAlloc(c *Clvm) (*Program, error) {
	id, err := o.CoinId()
	if err != nil {
		return nil, err
	}
	return c.AssertMyCoinId(id)
}

func (o *AssertMyParentId) clvmAlloc(c *Clvm) (*Program, error) {
	id, err := o.ParentId()
	if err != nil {
		return nil, err
	}
	return c.AssertMyParentId(id)
}

func (o *AssertMyPuzzleHash) clvmAlloc(c *Clvm) (*Program, error) {
	ph, err := o.PuzzleHash()
	if err != nil {
		return nil, err
	}
	return c.AssertMyPuzzleHash(ph)
}

func (o *AssertMyAmount) clvmAlloc(c *Clvm) (*Program, error) {
	amt, err := o.Amount()
	if err != nil {
		return nil, err
	}
	return c.AssertMyAmount(amt)
}

func (o *AssertMyBirthSeconds) clvmAlloc(c *Clvm) (*Program, error) {
	s, err := o.Seconds()
	if err != nil {
		return nil, err
	}
	return c.AssertMyBirthSeconds(s)
}

func (o *AssertMyBirthHeight) clvmAlloc(c *Clvm) (*Program, error) {
	h, err := o.Height()
	if err != nil {
		return nil, err
	}
	return c.AssertMyBirthHeight(h)
}

func (o *AssertEphemeral) clvmAlloc(c *Clvm) (*Program, error) {
	return c.AssertEphemeral()
}

func (o *SendMessage) clvmAlloc(c *Clvm) (*Program, error) {
	mode, err := o.Mode()
	if err != nil {
		return nil, err
	}
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	data, err := o.Data()
	if err != nil {
		return nil, err
	}
	defer func() {
		for _, d := range data {
			d.Free()
		}
	}()
	return c.SendMessage(mode, msg, data)
}

func (o *ReceiveMessage) clvmAlloc(c *Clvm) (*Program, error) {
	mode, err := o.Mode()
	if err != nil {
		return nil, err
	}
	msg, err := o.Message()
	if err != nil {
		return nil, err
	}
	data, err := o.Data()
	if err != nil {
		return nil, err
	}
	defer func() {
		for _, d := range data {
			d.Free()
		}
	}()
	return c.ReceiveMessage(mode, msg, data)
}

func (o *Softfork) clvmAlloc(c *Clvm) (*Program, error) {
	cost, err := o.Cost()
	if err != nil {
		return nil, err
	}
	rest, err := o.Rest()
	if err != nil {
		return nil, err
	}
	defer rest.Free()
	return c.Softfork(cost, rest)
}

func (o *MeltSingleton) clvmAlloc(c *Clvm) (*Program, error) {
	return c.MeltSingleton()
}

func (o *TransferNft) clvmAlloc(c *Clvm) (*Program, error) {
	lid, err := o.LauncherId()
	if err != nil {
		return nil, err
	}
	tp, err := o.TradePrices()
	if err != nil {
		return nil, err
	}
	defer func() {
		for _, t := range tp {
			t.Free()
		}
	}()
	siph, err := o.SingletonInnerPuzzleHash()
	if err != nil {
		return nil, err
	}
	return c.TransferNft(lid, tp, siph)
}

func (o *RunCatTail) clvmAlloc(c *Clvm) (*Program, error) {
	prog, err := o.Program()
	if err != nil {
		return nil, err
	}
	defer prog.Free()
	sol, err := o.Solution()
	if err != nil {
		return nil, err
	}
	defer sol.Free()
	return c.RunCatTail(prog, sol)
}

func (o *UpdateNftMetadata) clvmAlloc(c *Clvm) (*Program, error) {
	reveal, err := o.UpdaterPuzzleReveal()
	if err != nil {
		return nil, err
	}
	defer reveal.Free()
	sol, err := o.UpdaterSolution()
	if err != nil {
		return nil, err
	}
	defer sol.Free()
	return c.UpdateNftMetadata(reveal, sol)
}

func (o *UpdateDataStoreMerkleRoot) clvmAlloc(c *Clvm) (*Program, error) {
	root, err := o.NewMerkleRoot()
	if err != nil {
		return nil, err
	}
	memos, err := o.Memos()
	if err != nil {
		return nil, err
	}
	return c.UpdateDataStoreMerkleRoot(root, memos)
}

// ── Memo / metadata types ───────────────────────────────────────────────

func (o *NftMetadata) clvmAlloc(c *Clvm) (*Program, error) {
	return c.NftMetadata(o)
}

func (o *MipsMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.MipsMemo(o)
}

func (o *InnerPuzzleMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.InnerPuzzleMemo(o)
}

func (o *RestrictionMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.RestrictionMemo(o)
}

func (o *WrapperMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.WrapperMemo(o)
}

func (o *Force1of2RestrictedVariableMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.Force1Of2RestrictedVariableMemo(o)
}

func (o *MemoKind) clvmAlloc(c *Clvm) (*Program, error) {
	return c.MemoKind(o)
}

func (o *MemberMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.MemberMemo(o)
}

func (o *MofNMemo) clvmAlloc(c *Clvm) (*Program, error) {
	return c.MOfNMemo(o)
}

func (o *OptionMetadata) clvmAlloc(c *Clvm) (*Program, error) {
	return c.OptionMetadata(o)
}

func (o *NotarizedPayment) clvmAlloc(c *Clvm) (*Program, error) {
	return c.NotarizedPayment(o)
}

func (o *Payment) clvmAlloc(c *Clvm) (*Program, error) {
	return c.Payment(o)
}
