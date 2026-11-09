package chiawalletsdk_test

import (
	"bytes"
	"crypto/sha256"
	"errors"
	"strings"
	"sync"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// Ported from the Go bindings in #370, adapted to the UniFFI-generated API.

// seed returns a 32 byte seed whose first byte is the given value.
func seed(first byte) []byte {
	value := make([]byte, 32)
	value[0] = first
	return value
}

func TestSecretKeyFromSeed(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	if pkBytes := must(pk.ToBytes()); len(pkBytes) != 48 {
		t.Fatalf("expected 48 bytes, got %d", len(pkBytes))
	}
}

func TestSecretKeyRoundtrip(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(42)))
	defer sk.Destroy()

	skBytes := must(sk.ToBytes())
	if len(skBytes) != 32 {
		t.Fatalf("expected 32 bytes, got %d", len(skBytes))
	}

	parsed := must(sdk.SecretKeyFromBytes(skBytes))
	defer parsed.Destroy()

	if !bytes.Equal(must(parsed.ToBytes()), skBytes) {
		t.Fatal("expected the secret key bytes to roundtrip")
	}
}

func TestSecretKeyDerivation(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	hardened := must(sk.DeriveHardened(0))
	defer hardened.Destroy()

	unhardened := must(sk.DeriveUnhardened(0))
	defer unhardened.Destroy()

	synthetic := must(sk.DeriveSynthetic())
	defer synthetic.Destroy()

	keys := [][]byte{
		must(sk.ToBytes()),
		must(hardened.ToBytes()),
		must(unhardened.ToBytes()),
		must(synthetic.ToBytes()),
	}

	for i := range keys {
		for j := i + 1; j < len(keys); j++ {
			if bytes.Equal(keys[i], keys[j]) {
				t.Fatalf("expected keys %d and %d to differ", i, j)
			}
		}
	}

	path := must(sk.DeriveHardenedPath([]uint32{0}))
	defer path.Destroy()

	if !bytes.Equal(must(path.ToBytes()), keys[1]) {
		t.Fatal("expected a single step path to match DeriveHardened")
	}
}

func TestPublicKeyUnhardenedDerivation(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	childPk := must(pk.DeriveUnhardened(0))
	defer childPk.Destroy()

	childSk := must(sk.DeriveUnhardened(0))
	defer childSk.Destroy()

	childPkFromSk := must(childSk.PublicKey())
	defer childPkFromSk.Destroy()

	if !bytes.Equal(must(childPk.ToBytes()), must(childPkFromSk.ToBytes())) {
		t.Fatal("expected public and secret key derivation to match")
	}
}

func TestBlsSignAndVerify(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(1)))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	message := []byte("hello chia")
	sig := must(sk.Sign(message))
	defer sig.Destroy()

	if sigBytes := must(sig.ToBytes()); len(sigBytes) != 96 {
		t.Fatalf("expected a 96 byte signature, got %d", len(sigBytes))
	}

	if !must(pk.Verify(message, sig)) {
		t.Fatal("expected the signature to verify")
	}

	if must(pk.Verify([]byte("wrong message"), sig)) {
		t.Fatal("expected the signature not to verify with the wrong message")
	}

	parsed := must(sdk.SignatureFromBytes(must(sig.ToBytes())))
	defer parsed.Destroy()

	if !bytes.Equal(must(parsed.ToBytes()), must(sig.ToBytes())) {
		t.Fatal("expected the signature bytes to roundtrip")
	}
}

func TestBlsAggregation(t *testing.T) {
	sk1 := must(sdk.SecretKeyFromSeed(seed(1)))
	defer sk1.Destroy()

	sk2 := must(sdk.SecretKeyFromSeed(seed(2)))
	defer sk2.Destroy()

	pk1 := must(sk1.PublicKey())
	defer pk1.Destroy()

	pk2 := must(sk2.PublicKey())
	defer pk2.Destroy()

	sig1 := must(sk1.Sign([]byte("msg1")))
	defer sig1.Destroy()

	sig2 := must(sk2.Sign([]byte("msg2")))
	defer sig2.Destroy()

	aggSig := must(sdk.SignatureAggregate([]*sdk.Signature{sig1, sig2}))
	defer aggSig.Destroy()

	if !must(aggSig.IsValid()) {
		t.Fatal("expected the aggregate signature to be valid")
	}

	valid := must(sdk.PublicKeyAggregateVerify(
		[]*sdk.PublicKey{pk1, pk2},
		[][]byte{[]byte("msg1"), []byte("msg2")},
		aggSig,
	))
	if !valid {
		t.Fatal("expected the aggregate signature to verify")
	}

	aggPk := must(sdk.PublicKeyAggregate([]*sdk.PublicKey{pk1, pk2}))
	defer aggPk.Destroy()

	if len(must(aggPk.ToBytes())) != 48 || must(aggPk.IsInfinity()) {
		t.Fatal("expected a 48 byte aggregate public key that isn't infinity")
	}
}

func TestInfinity(t *testing.T) {
	pk := must(sdk.PublicKeyInfinity())
	defer pk.Destroy()

	if !must(pk.IsInfinity()) {
		t.Fatal("expected the infinity public key to be infinity")
	}

	sig := must(sdk.SignatureInfinity())
	defer sig.Destroy()

	if !must(sig.IsInfinity()) {
		t.Fatal("expected the infinity signature to be infinity")
	}
}

func TestPublicKeyFingerprint(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	clone := pk.Clone()
	defer clone.Destroy()

	fingerprint := must(pk.Fingerprint())
	if fingerprint == 0 {
		t.Fatal("expected a non-zero fingerprint")
	}

	if must(clone.Fingerprint()) != fingerprint {
		t.Fatal("expected the clone to have the same fingerprint")
	}
}

func TestK1SignAndVerify(t *testing.T) {
	skBytes := sha256.Sum256([]byte("k1 sign test"))
	sk := must(sdk.K1SecretKeyFromBytes(skBytes[:]))
	defer sk.Destroy()

	if !bytes.Equal(must(sk.ToBytes()), skBytes[:]) {
		t.Fatal("expected the K1 secret key bytes to roundtrip")
	}

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	if pkBytes := must(pk.ToBytes()); len(pkBytes) != 33 {
		t.Fatalf("expected a 33 byte compressed public key, got %d", len(pkBytes))
	}

	if must(pk.Fingerprint()) == 0 {
		t.Fatal("expected a non-zero fingerprint")
	}

	hash := sha256.Sum256([]byte("hello k1"))
	sig := must(sk.SignPrehashed(hash[:]))
	defer sig.Destroy()

	if sigBytes := must(sig.ToBytes()); len(sigBytes) != 64 {
		t.Fatalf("expected a 64 byte signature, got %d", len(sigBytes))
	}

	if !must(pk.VerifyPrehashed(hash[:], sig)) {
		t.Fatal("expected the signature to verify")
	}

	wrong := sha256.Sum256([]byte("wrong"))
	if must(pk.VerifyPrehashed(wrong[:], sig)) {
		t.Fatal("expected the signature not to verify with the wrong hash")
	}
}

func TestMnemonicGenerate(t *testing.T) {
	for _, c := range []struct {
		use24   bool
		words   int
		entropy int
	}{
		{true, 24, 32},
		{false, 12, 16},
	} {
		mnemonic := must(sdk.MnemonicGenerate(c.use24))
		phrase := must(mnemonic.ToString())

		if words := strings.Fields(phrase); len(words) != c.words {
			t.Fatalf("expected %d words, got %d", c.words, len(words))
		}

		if !must(sdk.MnemonicVerify(phrase)) {
			t.Fatalf("expected %q to be valid", phrase)
		}

		entropy := must(mnemonic.ToEntropy())
		if len(entropy) != c.entropy {
			t.Fatalf("expected %d bytes of entropy, got %d", c.entropy, len(entropy))
		}

		fromEntropy := must(sdk.MnemonicFromEntropy(entropy))
		if must(fromEntropy.ToString()) != phrase {
			t.Fatal("expected the mnemonic to roundtrip through its entropy")
		}

		fromEntropy.Destroy()
		mnemonic.Destroy()
	}
}

func TestMnemonicToSeed(t *testing.T) {
	mnemonic := must(sdk.MnemonicGenerate(true))
	defer mnemonic.Destroy()

	seed := must(mnemonic.ToSeed(""))
	if len(seed) != 64 {
		t.Fatalf("expected a 64 byte seed, got %d", len(seed))
	}

	if bytes.Equal(must(mnemonic.ToSeed("password")), seed) {
		t.Fatal("expected a password to change the seed")
	}
}

func TestAddressTestnetPrefix(t *testing.T) {
	address := must(sdk.NewAddress(make([]byte, 32), "txch"))
	defer address.Destroy()

	if encoded := must(address.Encode()); !strings.HasPrefix(encoded, "txch1") {
		t.Fatalf("expected a txch address, got %s", encoded)
	}
}

func TestCoinFields(t *testing.T) {
	parent := seed(1)
	puzzleHash := seed(2)

	coin := must(sdk.NewCoin(parent, puzzleHash, 1000))
	defer coin.Destroy()

	if !bytes.Equal(coin.ParentCoinInfo(), parent) || !bytes.Equal(coin.PuzzleHash(), puzzleHash) || coin.Amount() != 1000 {
		t.Fatal("unexpected coin fields")
	}

	check(t, coin.SetParentCoinInfo(seed(0xff)))
	check(t, coin.SetPuzzleHash(seed(0xee)))

	if coin.ParentCoinInfo()[0] != 0xff || coin.PuzzleHash()[0] != 0xee {
		t.Fatal("expected the setters to take effect")
	}

	if err := coin.SetPuzzleHash(make([]byte, 31)); err == nil {
		t.Fatal("expected an error for a 31 byte puzzle hash")
	}

	same := must(sdk.NewCoin(seed(0xff), seed(0xee), 1000))
	defer same.Destroy()

	other := must(sdk.NewCoin(seed(0xff), seed(0xee), 2000))
	defer other.Destroy()

	if !bytes.Equal(must(coin.CoinId()), must(same.CoinId())) {
		t.Fatal("expected identical coins to have the same id")
	}

	if bytes.Equal(must(coin.CoinId()), must(other.CoinId())) {
		t.Fatal("expected different amounts to give different ids")
	}
}

func TestCoinStateOptionalHeights(t *testing.T) {
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 100))
	defer coin.Destroy()

	state := must(sdk.NewCoinState(coin, nil, nil))
	defer state.Destroy()

	if state.SpentHeight() != nil || state.CreatedHeight() != nil {
		t.Fatal("expected no heights")
	}

	height := uint32(100)
	check(t, state.SetCreatedHeight(&height))

	// The setter copies the value, so changing the variable afterwards has no effect
	height = 200

	if created := state.CreatedHeight(); created == nil || *created != 100 {
		t.Fatalf("expected a created height of 100, got %v", created)
	}

	check(t, state.SetCreatedHeight(nil))

	if state.CreatedHeight() != nil {
		t.Fatal("expected the created height to be cleared")
	}
}

func TestCoinSpendAndSpendBundle(t *testing.T) {
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 100))
	defer coin.Destroy()

	coinSpend := must(sdk.NewCoinSpend(coin, []byte{0x01}, []byte{0x80}))
	defer coinSpend.Destroy()

	if coinSpend.Coin().Amount() != 100 ||
		!bytes.Equal(coinSpend.PuzzleReveal(), []byte{0x01}) ||
		!bytes.Equal(coinSpend.Solution(), []byte{0x80}) {
		t.Fatal("unexpected coin spend fields")
	}

	signature := must(sdk.SignatureInfinity())
	defer signature.Destroy()

	bundle := must(sdk.NewSpendBundle([]*sdk.CoinSpend{coinSpend}, signature))
	defer bundle.Destroy()

	if len(bundle.CoinSpends()) != 1 {
		t.Fatalf("expected 1 coin spend, got %d", len(bundle.CoinSpends()))
	}

	encoded := must(bundle.ToBytes())
	parsed := must(sdk.SpendBundleFromBytes(encoded))
	defer parsed.Destroy()

	if !bytes.Equal(must(parsed.Hash()), must(bundle.Hash())) {
		t.Fatal("expected the spend bundle to roundtrip")
	}

	offer := must(sdk.EncodeOffer(bundle))
	if !strings.HasPrefix(offer, "offer1") {
		t.Fatalf("unexpected offer %s", offer)
	}

	decoded := must(sdk.DecodeOffer(offer))
	defer decoded.Destroy()

	if !bytes.Equal(must(decoded.Hash()), must(bundle.Hash())) {
		t.Fatal("expected the offer to roundtrip")
	}
}

func TestClvmSerialization(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	nilProgram := must(clvm.Nil())
	if !bytes.Equal(must(nilProgram.Serialize()), []byte{0x80}) {
		t.Fatal("expected nil to serialize to 80")
	}

	pair := must(clvm.Pair(nilProgram, nilProgram))
	if !bytes.Equal(must(pair.Serialize()), []byte{0xff, 0x80, 0x80}) {
		t.Fatal("expected (() . ()) to serialize to ff8080")
	}

	parsed := must(clvm.Parse("42"))
	if value := must(parsed.ToInt()); value == nil || *value != "42" {
		t.Fatalf("expected 42, got %v", value)
	}

	deserialized := must(clvm.Deserialize([]byte{0xff, 0x80, 0x80}))
	if !bytes.Equal(must(deserialized.TreeHash()), must(pair.TreeHash())) {
		t.Fatal("expected the deserialized program to match")
	}

	if _, err := clvm.Deserialize([]byte{0xff}); err == nil {
		t.Fatal("expected an error for a truncated program")
	}
}

func TestClvmList(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	list := must(clvm.List([]*sdk.Program{
		must(clvm.Int("1")),
		must(clvm.List([]*sdk.Program{must(clvm.Int("2")), must(clvm.Int("3"))})),
	}))
	defer list.Destroy()

	items := must(list.ToList())
	if items == nil || len(*items) != 2 {
		t.Fatal("expected a list of 2 items")
	}

	inner := must((*items)[1].ToList())
	if inner == nil || len(*inner) != 2 {
		t.Fatal("expected the second item to be a list of 2 items")
	}

	if value := must((*inner)[1].ToInt()); value == nil || *value != "3" {
		t.Fatalf("expected 3, got %v", value)
	}

	empty := must(clvm.List(nil))
	if !must(empty.IsNull()) {
		t.Fatal("expected an empty list to be nil")
	}

	atom := must(clvm.Int("256"))
	if must(atom.ToList()) != nil {
		t.Fatal("expected an atom not to be a list")
	}

	// Length is the length of an atom in bytes, so it fails for a list
	if length := must(atom.Length()); length != 2 {
		t.Fatalf("expected a 2 byte atom, got %d", length)
	}

	if _, err := list.Length(); err == nil {
		t.Fatal("expected an error for the length of a list")
	}
}

func TestConditions(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	id := make([]byte, 32)

	conditions := map[string]func() (*sdk.Program, error){
		"CreateCoin":               func() (*sdk.Program, error) { return clvm.CreateCoin(id, 100, nil) },
		"ReserveFee":               func() (*sdk.Program, error) { return clvm.ReserveFee(50) },
		"AssertCoinAnnouncement":   func() (*sdk.Program, error) { return clvm.AssertCoinAnnouncement(id) },
		"AssertPuzzleAnnouncement": func() (*sdk.Program, error) { return clvm.AssertPuzzleAnnouncement(id) },
		"CreateCoinAnnouncement":   func() (*sdk.Program, error) { return clvm.CreateCoinAnnouncement([]byte("hello")) },
		"CreatePuzzleAnnouncement": func() (*sdk.Program, error) { return clvm.CreatePuzzleAnnouncement([]byte("world")) },
		"AssertHeightAbsolute":     func() (*sdk.Program, error) { return clvm.AssertHeightAbsolute(100) },
		"AssertHeightRelative":     func() (*sdk.Program, error) { return clvm.AssertHeightRelative(10) },
		"AssertSecondsAbsolute":    func() (*sdk.Program, error) { return clvm.AssertSecondsAbsolute(1_000_000) },
		"AssertSecondsRelative":    func() (*sdk.Program, error) { return clvm.AssertSecondsRelative(60) },
		"Remark":                   func() (*sdk.Program, error) { return clvm.Remark(must(clvm.Nil())) },
	}

	for name, build := range conditions {
		program, err := build()
		if err != nil {
			t.Errorf("%s: %v", name, err)
			continue
		}

		if !must(program.IsPair()) {
			t.Errorf("%s: expected a condition list", name)
		}
	}

	if _, err := clvm.CreateCoin(make([]byte, 31), 100, nil); err == nil {
		t.Fatal("expected an error for a 31 byte puzzle hash")
	}

	memos := must(clvm.List([]*sdk.Program{must(clvm.Atom(id))}))
	createCoin := must(clvm.CreateCoin(id, 100, &memos))

	parsed := must(createCoin.ParseCreateCoin())
	if parsed == nil {
		t.Fatal("expected to parse the create coin condition")
	}

	if parsedMemos := (*parsed).Memos(); parsedMemos == nil || !bytes.Equal(must((*parsedMemos).TreeHash()), must(memos.TreeHash())) {
		t.Fatal("expected the memos to roundtrip")
	}

	if must(must(clvm.ReserveFee(50)).ParseCreateCoin()) != nil {
		t.Fatal("expected a reserve fee not to parse as a create coin")
	}
}

func TestConstants(t *testing.T) {
	puzzle := must(sdk.ConstantsBlsMember())
	hash := must(sdk.ConstantsBlsMemberHash())

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	if !bytes.Equal(must(must(clvm.Deserialize(puzzle)).TreeHash()), hash) {
		t.Fatal("expected the BLS member puzzle to match its hash")
	}
}

func TestSimulatorState(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	if height := must(sim.Height()); height != 0 {
		t.Fatalf("expected an initial height of 0, got %d", height)
	}

	if len(must(sim.HeaderHash())) != 32 {
		t.Fatal("expected a 32 byte header hash")
	}

	timestamp := must(sim.NextTimestamp())
	check(t, sim.PassTime(60))

	if next := must(sim.NextTimestamp()); next != timestamp+60 {
		t.Fatalf("expected timestamp %d, got %d", timestamp+60, next)
	}

	sim1 := must(sdk.SimulatorWithSeed(42))
	defer sim1.Destroy()

	sim2 := must(sdk.SimulatorWithSeed(42))
	defer sim2.Destroy()

	if !bytes.Equal(must(sim1.HeaderHash()), must(sim2.HeaderHash())) {
		t.Fatal("expected simulators with the same seed to match")
	}
}

func TestSimulatorCoins(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	created := must(sim.NewCoin(seed(0x42), 1_000_000))
	defer created.Destroy()

	if created.Amount() != 1_000_000 || !bytes.Equal(created.PuzzleHash(), seed(0x42)) {
		t.Fatal("unexpected simulator coin")
	}

	inserted := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 500))
	defer inserted.Destroy()

	check(t, sim.InsertCoin(inserted))

	state := must(sim.CoinState(must(inserted.CoinId())))
	if state == nil || (*state).Coin().Amount() != 500 {
		t.Fatal("expected the inserted coin's state")
	}

	if missing := must(sim.CoinState(make([]byte, 32))); missing != nil {
		t.Fatal("expected no state for an unknown coin")
	}

	pair := must(sim.Bls(2_000_000_000_000))
	defer pair.Destroy()

	if pair.Coin().Amount() != 2_000_000_000_000 || len(pair.PuzzleHash()) != 32 {
		t.Fatal("unexpected BLS pair")
	}
}

func TestSimulatorStandardSpend(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(sim.Bls(1_000_000))
	defer pair.Destroy()

	coin := pair.Coin()
	delegated := must(clvm.DelegatedSpend([]*sdk.Program{must(clvm.CreateCoin(seed(0xaa), 1_000_000, nil))}))
	spend := must(clvm.StandardSpend(pair.Pk(), delegated))
	check(t, clvm.SpendCoin(coin, spend))
	check(t, sim.SpendCoins(must(clvm.CoinSpends()), []*sdk.SecretKey{pair.Sk()}))

	coinID := must(coin.CoinId())
	state := must(sim.CoinState(coinID))
	if state == nil || (*state).SpentHeight() == nil {
		t.Fatal("expected the coin to be spent")
	}

	children := must(sim.Children(coinID))
	if len(children) != 1 || children[0].Coin().Amount() != 1_000_000 {
		t.Fatalf("expected a single child coin, got %d", len(children))
	}
}

func TestSimulatorRejectsMissingSignature(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(sim.Bls(1000))
	defer pair.Destroy()

	spend := must(clvm.DelegatedSpend([]*sdk.Program{must(clvm.CreateCoin(pair.PuzzleHash(), 1000, nil))}))
	check(t, clvm.SpendStandardCoin(pair.Coin(), pair.Pk(), spend))

	err := sim.SpendCoins(must(clvm.CoinSpends()), nil)
	if err == nil {
		t.Fatal("expected the spend to fail without a signature")
	}

	var chiaErr *sdk.ChiaError
	if !errors.As(err, &chiaErr) || !errors.Is(err, sdk.ErrChiaErrorError) {
		t.Fatalf("expected a ChiaError, got %T: %v", err, err)
	}
}

func TestFullNodeSimulatorRpc(t *testing.T) {
	sim := must(sdk.NewFullNodeSimulator())
	defer sim.Destroy()

	server := must(sim.StartServer())

	rpc := must(sdk.NewRpcClient(server.Url()))
	defer rpc.Destroy()

	response := must(rpc.GetCoinRecordsByHint(make([]byte, 32), nil, nil, nil, nil))
	defer response.Destroy()

	if !response.Success() {
		t.Fatalf("expected the request to succeed, got %v", response.Error())
	}

	if records := response.CoinRecords(); records == nil || len(*records) != 0 {
		t.Fatal("expected an empty list of coin records")
	}

	if cursor := response.NextCursor(); cursor != nil {
		t.Fatalf("expected no pagination cursor, got %q", *cursor)
	}

	// Closing the server twice is allowed
	check(t, server.Close())
	check(t, server.Close())
}

func TestDelegatedPuzzleOracle(t *testing.T) {
	puzzle := must(sdk.DelegatedPuzzleNewOracle(make([]byte, 32), 42))
	defer puzzle.Destroy()

	oracle := must(puzzle.ToOracle())
	if oracle == nil || (*oracle).OracleFee() != 42 {
		t.Fatal("expected an oracle with a fee of 42")
	}

	if admin := must(puzzle.ToAdmin()); admin != nil {
		t.Fatal("expected an oracle not to be an admin")
	}
}

func TestUriKind(t *testing.T) {
	for _, kind := range []sdk.UriKind{sdk.UriKindData, sdk.UriKindMetadata, sdk.UriKindLicense} {
		update := must(sdk.NewMetadataUpdate(kind, "https://example.com"))
		if update.Kind() != kind {
			t.Fatalf("expected kind %d, got %d", kind, update.Kind())
		}
		update.Destroy()
	}
}

func TestIdsAndDeltas(t *testing.T) {
	xch := must(sdk.IdXch())
	defer xch.Destroy()

	if !must(xch.IsXch()) {
		t.Fatal("expected the XCH id to be XCH")
	}

	newID := must(sdk.NewId(7))
	defer newID.Destroy()

	if index := must(newID.AsNew()); index == nil || *index != 7 {
		t.Fatalf("expected new id 7, got %v", index)
	}

	existing := must(sdk.IdExisting(seed(0x42)))
	defer existing.Destroy()

	if assetID := must(existing.AsExisting()); assetID == nil || !bytes.Equal(*assetID, seed(0x42)) {
		t.Fatal("expected the existing asset id")
	}

	if must(existing.Equals(xch)) {
		t.Fatal("expected different ids not to be equal")
	}

	delta := must(sdk.NewDelta("1000", "500"))
	defer delta.Destroy()

	clone := delta.Clone()
	defer clone.Destroy()

	if clone.Input() != "1000" || clone.Output() != "500" {
		t.Fatalf("unexpected delta %s -> %s", clone.Input(), clone.Output())
	}
}

// signPendingSpends spends every pending coin with the standard puzzle, using the given key.
func signPendingSpends(tb testing.TB, clvm *sdk.Clvm, finished *sdk.FinishedSpends, pk *sdk.PublicKey) {
	tb.Helper()

	for _, pending := range must(finished.PendingSpends()) {
		delegated := must(clvm.DelegatedSpend(must(pending.Conditions())))
		spend := must(clvm.StandardSpend(pk, delegated))

		if err := finished.Insert(must(must(pending.Coin()).CoinId()), spend); err != nil {
			tb.Fatal(err)
		}
	}
}

// spendActions applies the actions to the pair's coin, signs and confirms the spends, and returns
// the outputs.
func spendActions(tb testing.TB, sim *sdk.Simulator, clvm *sdk.Clvm, pair *sdk.BlsPairWithCoin, actions []*sdk.Action) *sdk.Outputs {
	tb.Helper()

	spends := must(sdk.NewSpends(clvm, pair.PuzzleHash()))
	defer spends.Destroy()

	if err := spends.AddXch(pair.Coin()); err != nil {
		tb.Fatal(err)
	}

	deltas := must(spends.Apply(actions))
	finished := must(spends.Prepare(deltas, nil))
	signPendingSpends(tb, clvm, finished, pair.Pk())

	outputs := must(finished.Spend())

	if err := sim.SpendCoins(must(clvm.CoinSpends()), []*sdk.SecretKey{pair.Sk()}); err != nil {
		tb.Fatal(err)
	}

	return outputs
}

func TestActionXchSend(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(sim.Bls(1_000_000))
	defer pair.Destroy()

	destination := seed(0xbb)
	outputs := spendActions(t, sim, clvm, pair, []*sdk.Action{
		must(sdk.ActionSend(must(sdk.IdXch()), destination, 500_000, nil)),
		must(sdk.ActionFee(100)),
	})
	defer outputs.Destroy()

	if fee := must(outputs.Fee()); fee != 100 {
		t.Fatalf("expected a fee of 100, got %d", fee)
	}

	if len(must(outputs.Xch())) == 0 {
		t.Fatal("expected XCH outputs")
	}

	coins := must(sim.UnspentCoins(destination, false))
	if len(coins) != 1 || coins[0].Amount() != 500_000 {
		t.Fatalf("expected a single 500000 mojo coin at the destination, got %d coins", len(coins))
	}

	change := must(sim.UnspentCoins(pair.PuzzleHash(), false))
	if len(change) != 1 || change[0].Amount() != 499_900 {
		t.Fatalf("expected 499900 mojos of change, got %d coins", len(change))
	}
}

func TestActionCatIssuance(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(sim.Bls(1_000_000))
	defer pair.Destroy()

	hiddenPuzzleHash := pair.PuzzleHash()
	outputs := spendActions(t, sim, clvm, pair, []*sdk.Action{
		must(sdk.ActionSingleIssueCat(&hiddenPuzzleHash, 10_000)),
		must(sdk.ActionFee(100)),
	})
	defer outputs.Destroy()

	ids := must(outputs.Cats())
	if len(ids) != 1 {
		t.Fatalf("expected 1 CAT, got %d", len(ids))
	}

	cats := must(outputs.Cat(ids[0]))
	if len(cats) != 1 || cats[0].Coin().Amount() != 10_000 {
		t.Fatalf("expected a single 10000 mojo CAT, got %d", len(cats))
	}

	coinID := must(cats[0].Coin().CoinId())
	if state := must(sim.CoinState(coinID)); state == nil || (*state).SpentHeight() != nil {
		t.Fatal("expected the CAT coin to be unspent on chain")
	}
}

func TestActionNftMint(t *testing.T) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(sim.Bls(1_000_000))
	defer pair.Destroy()

	metadata := must(sdk.NewNftMetadata(
		1,
		1,
		[]string{"https://example.com/nft.png"},
		nil,
		[]string{"https://example.com/metadata.json"},
		nil,
		[]string{},
		nil,
	))
	defer metadata.Destroy()

	if uris := metadata.DataUris(); len(uris) != 1 || uris[0] != "https://example.com/nft.png" {
		t.Fatalf("unexpected data uris %v", uris)
	}

	if uris := metadata.MetadataUris(); len(uris) != 1 || uris[0] != "https://example.com/metadata.json" {
		t.Fatalf("unexpected metadata uris %v", uris)
	}

	if uris := metadata.LicenseUris(); len(uris) != 0 {
		t.Fatalf("expected no license uris, got %v", uris)
	}

	if metadata.DataHash() != nil {
		t.Fatal("expected no data hash")
	}

	metadataProgram := must(clvm.NftMetadata(metadata))

	parsed := must(metadataProgram.ParseNftMetadata())
	if parsed == nil || (*parsed).EditionNumber() != 1 || len((*parsed).DataUris()) != 1 {
		t.Fatal("expected the metadata to roundtrip through CLVM")
	}

	updaterHash := must(must(clvm.NftMetadataUpdaterDefault()).TreeHash())

	outputs := spendActions(t, sim, clvm, pair, []*sdk.Action{
		must(sdk.ActionMintNft(clvm, metadataProgram, updaterHash, pair.PuzzleHash(), 0, 1, nil)),
		must(sdk.ActionFee(100)),
	})
	defer outputs.Destroy()

	ids := must(outputs.Nfts())
	if len(ids) != 1 {
		t.Fatalf("expected 1 NFT, got %d", len(ids))
	}

	nft := must(outputs.Nft(ids[0]))
	defer nft.Destroy()

	if nft.Coin().Amount() != 1 {
		t.Fatalf("expected a 1 mojo NFT, got %d", nft.Coin().Amount())
	}
}

func TestInvalidInputs(t *testing.T) {
	cases := map[string]func() error{
		"10 byte public key": func() error { _, err := sdk.PublicKeyFromBytes(make([]byte, 10)); return err },
		"10 byte secret key": func() error { _, err := sdk.SecretKeyFromBytes(make([]byte, 10)); return err },
		"10 byte signature":  func() error { _, err := sdk.SignatureFromBytes(make([]byte, 10)); return err },
		"invalid mnemonic":   func() error { _, err := sdk.NewMnemonic("not a valid mnemonic phrase at all"); return err },
		"invalid address":    func() error { _, err := sdk.AddressDecode("notavalidaddress"); return err },
		"invalid offer":      func() error { _, err := sdk.DecodeOffer("not_a_valid_offer_string"); return err },
	}

	for name, run := range cases {
		if err := run(); err == nil {
			t.Errorf("%s: expected an error", name)
		} else if !errors.Is(err, sdk.ErrChiaErrorError) {
			t.Errorf("%s: expected a ChiaError, got %T: %v", name, err, err)
		}
	}

	if must(sdk.MnemonicVerify("not a valid mnemonic phrase at all")) {
		t.Fatal("expected an invalid mnemonic not to verify")
	}
}

func TestShortSeedIsAnError(t *testing.T) {
	t.Skip("SecretKey::from_seed in chia-sdk-bindings (bls.rs) passes seeds under 32 bytes to chia-bls, " +
		"which panics on an assertion instead of returning an error")

	for _, value := range [][]byte{nil, make([]byte, 16)} {
		if _, err := sdk.SecretKeyFromSeed(value); err == nil {
			t.Fatalf("expected an error for a %d byte seed", len(value))
		}
	}
}

func TestDestroyIsIdempotent(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	sk.Destroy()
	sk.Destroy()

	var wg sync.WaitGroup
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 100))

	for range 10 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			coin.Destroy()
		}()
	}

	wg.Wait()
}

// recoverDestroyed runs the function, and reports whether it panicked because an object it used
// had already been destroyed. Any other panic is passed on.
func recoverDestroyed(run func()) (destroyed bool) {
	defer func() {
		if r := recover(); r != nil {
			if err, ok := r.(error); ok && strings.Contains(err.Error(), "already been destroyed") {
				destroyed = true
				return
			}
			panic(r)
		}
	}()

	run()
	return false
}

func TestUseAfterDestroyPanics(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	sk.Destroy()

	if !recoverDestroyed(func() { _, _ = sk.ToBytes() }) {
		t.Fatal("expected a method call on a destroyed object to panic")
	}

	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1))
	coin.Destroy()

	if !recoverDestroyed(func() { _, _ = sdk.NewCoinSpend(coin, nil, nil) }) {
		t.Fatal("expected passing a destroyed object as an argument to panic")
	}

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	program := must(clvm.Nil())
	program.Destroy()

	if !recoverDestroyed(func() { _, _ = clvm.CreateCoin(make([]byte, 32), 1, &program) }) {
		t.Fatal("expected a destroyed optional argument to panic, not be treated as absent")
	}

	if !recoverDestroyed(func() { _, _ = clvm.List([]*sdk.Program{program}) }) {
		t.Fatal("expected a destroyed list element to panic")
	}
}

// Destroying an object while another goroutine uses it must either let the call finish or make
// it panic with a clear error, and never free the object while a call is using it.
func TestConcurrentDestroyWhileUsing(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	for range 100 {
		sk := must(sdk.SecretKeyFromSeed(seed(99)))
		coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1))
		program := must(clvm.Nil())

		start := make(chan struct{})
		var wg sync.WaitGroup

		wg.Add(4)
		go func() {
			defer wg.Done()
			<-start
			sk.Destroy()
			coin.Destroy()
			program.Destroy()
		}()
		go func() {
			defer wg.Done()
			<-start
			recoverDestroyed(func() {
				if value, err := sk.ToBytes(); err != nil || len(value) != 32 {
					t.Errorf("unexpected result %x, %v", value, err)
				}
			})
		}()
		go func() {
			defer wg.Done()
			<-start
			recoverDestroyed(func() {
				if spend, err := sdk.NewCoinSpend(coin, []byte{0x80}, []byte{0x80}); err != nil {
					t.Error(err)
				} else {
					spend.Destroy()
				}
			})
		}()
		go func() {
			defer wg.Done()
			<-start
			// The receiver also appears twice in its own argument list
			recoverDestroyed(func() {
				if curried, err := program.Curry([]*sdk.Program{program, program}); err != nil {
					t.Error(err)
				} else {
					curried.Destroy()
				}
			})
		}()

		close(start)
		wg.Wait()
	}
}

func TestConcurrentMethodCalls(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(42)))
	defer sk.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	program := must(clvm.Pair(must(clvm.Nil()), must(clvm.Nil())))
	defer program.Destroy()

	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1))
	defer coin.Destroy()

	expectedKey := must(sk.ToBytes())
	expectedHash := must(program.TreeHash())

	var wg sync.WaitGroup
	for range 16 {
		wg.Add(1)
		go func() {
			defer wg.Done()

			for range 50 {
				clone := sk.Clone()
				value, err := clone.ToBytes()
				clone.Destroy()
				if err != nil || !bytes.Equal(value, expectedKey) {
					t.Errorf("unexpected key %x, %v", value, err)
					return
				}

				hash, err := program.TreeHash()
				if err != nil || !bytes.Equal(hash, expectedHash) {
					t.Errorf("unexpected tree hash %x, %v", hash, err)
					return
				}

				if err := coin.SetAmount(2); err != nil {
					t.Error(err)
					return
				}

				spend, err := sdk.NewCoinSpend(coin, nil, nil)
				if err != nil {
					t.Error(err)
					return
				}
				spend.Destroy()

				nilProgram, err := clvm.Nil()
				if err != nil {
					t.Error(err)
					return
				}
				nilProgram.Destroy()
			}
		}()
	}

	wg.Wait()
}
