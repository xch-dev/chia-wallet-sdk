package chiawalletsdk

import (
	"bytes"
	"crypto/sha256"
	"sync"
	"testing"
)

// ── BLS Cryptography ────────────────────────────────────────────────────

func TestSecretKeyFromSeed(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Free()

	pk, err := sk.PublicKey()
	if err != nil {
		t.Fatalf("PublicKey: %v", err)
	}
	defer pk.Free()

	pkBytes, err := pk.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	if len(pkBytes) != 48 {
		t.Fatalf("expected 48 bytes, got %d", len(pkBytes))
	}
}

func TestSecretKeyRoundtrip(t *testing.T) {
	seed := make([]byte, 32)
	seed[0] = 42
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Free()

	skBytes, err := sk.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	if len(skBytes) != 32 {
		t.Fatalf("expected 32 bytes, got %d", len(skBytes))
	}

	sk2, err := NewSecretKeyFromBytes(skBytes)
	if err != nil {
		t.Fatalf("NewSecretKeyFromBytes: %v", err)
	}
	defer sk2.Free()

	skBytes2, err := sk2.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	if !bytes.Equal(skBytes, skBytes2) {
		t.Fatal("secret key bytes should match after roundtrip")
	}
}

func TestSecretKeyDerivation(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Free()

	// Hardened derivation
	child, err := sk.DeriveHardened(0)
	if err != nil {
		t.Fatalf("DeriveHardened: %v", err)
	}
	defer child.Free()

	childBytes, err := child.Bytes()
	if err != nil {
		t.Fatalf("child ToBytes: %v", err)
	}
	skBytes, err := sk.Bytes()
	if err != nil {
		t.Fatalf("sk ToBytes: %v", err)
	}
	if bytes.Equal(childBytes, skBytes) {
		t.Fatal("derived key should differ from parent")
	}

	// Unhardened derivation
	child2, err := sk.DeriveUnhardened(0)
	if err != nil {
		t.Fatalf("DeriveUnhardened: %v", err)
	}
	defer child2.Free()

	child2Bytes, err := child2.Bytes()
	if err != nil {
		t.Fatalf("child2 ToBytes: %v", err)
	}
	if bytes.Equal(childBytes, child2Bytes) {
		t.Fatal("hardened and unhardened derived keys should differ")
	}
}

func TestSyntheticKeyDerivation(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Free()

	synth, err := sk.DeriveSynthetic()
	if err != nil {
		t.Fatalf("DeriveSynthetic: %v", err)
	}
	defer synth.Free()

	synthBytes, err := synth.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	skBytes, err := sk.Bytes()
	if err != nil {
		t.Fatalf("sk ToBytes: %v", err)
	}
	if bytes.Equal(synthBytes, skBytes) {
		t.Fatal("synthetic key should differ from original")
	}
}

func TestBlsSignAndVerify(t *testing.T) {
	seed := make([]byte, 32)
	seed[0] = 1
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Free()

	pk, err := sk.PublicKey()
	if err != nil {
		t.Fatalf("PublicKey: %v", err)
	}
	defer pk.Free()

	message := []byte("hello chia")
	sig, err := sk.Sign(message)
	if err != nil {
		t.Fatalf("Sign: %v", err)
	}
	defer sig.Free()

	sigBytes, err := sig.Bytes()
	if err != nil {
		t.Fatalf("sig ToBytes: %v", err)
	}
	if len(sigBytes) != 96 {
		t.Fatalf("expected 96 byte signature, got %d", len(sigBytes))
	}

	valid, err := pk.Verify(message, sig)
	if err != nil {
		t.Fatalf("Verify: %v", err)
	}
	if !valid {
		t.Fatal("signature should be valid")
	}

	// Verify with wrong message fails
	valid, err = pk.Verify([]byte("wrong message"), sig)
	if err != nil {
		t.Fatalf("Verify wrong: %v", err)
	}
	if valid {
		t.Fatal("signature should not verify with wrong message")
	}
}

func TestSignatureAggregation(t *testing.T) {
	seed1 := make([]byte, 32)
	seed1[0] = 1
	seed2 := make([]byte, 32)
	seed2[0] = 2

	sk1, err := NewSecretKeyFromSeed(seed1)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed(1): %v", err)
	}
	defer sk1.Free()

	sk2, err := NewSecretKeyFromSeed(seed2)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed(2): %v", err)
	}
	defer sk2.Free()

	sig1, err := sk1.Sign([]byte("msg1"))
	if err != nil {
		t.Fatalf("Sign(1): %v", err)
	}
	defer sig1.Free()

	sig2, err := sk2.Sign([]byte("msg2"))
	if err != nil {
		t.Fatalf("Sign(2): %v", err)
	}
	defer sig2.Free()

	aggSig, err := NewSignatureAggregate([]*Signature{sig1, sig2})
	if err != nil {
		t.Fatalf("NewSignatureAggregate: %v", err)
	}
	defer aggSig.Free()

	aggSigBytes, err := aggSig.Bytes()
	if err != nil {
		t.Fatalf("aggSig ToBytes: %v", err)
	}
	if len(aggSigBytes) != 96 {
		t.Fatalf("expected 96 byte aggregate signature, got %d", len(aggSigBytes))
	}

	isValid, err := aggSig.IsValid()
	if err != nil {
		t.Fatalf("IsValid: %v", err)
	}
	if !isValid {
		t.Fatal("aggregate signature should be valid")
	}
}

func TestPublicKeyAggregation(t *testing.T) {
	seed1 := make([]byte, 32)
	seed1[0] = 10
	seed2 := make([]byte, 32)
	seed2[0] = 20

	sk1, _ := NewSecretKeyFromSeed(seed1)
	defer sk1.Free()
	sk2, _ := NewSecretKeyFromSeed(seed2)
	defer sk2.Free()

	pk1, _ := sk1.PublicKey()
	defer pk1.Free()
	pk2, _ := sk2.PublicKey()
	defer pk2.Free()

	aggPk, err := NewPublicKeyAggregate([]*PublicKey{pk1, pk2})
	if err != nil {
		t.Fatalf("NewPublicKeyAggregate: %v", err)
	}
	defer aggPk.Free()

	aggBytes, err := aggPk.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	if len(aggBytes) != 48 {
		t.Fatalf("expected 48 bytes, got %d", len(aggBytes))
	}

	isInf, err := aggPk.IsInfinity()
	if err != nil {
		t.Fatalf("IsInfinity: %v", err)
	}
	if isInf {
		t.Fatal("aggregated key should not be infinity")
	}
}

func TestPublicKeyInfinity(t *testing.T) {
	pk, err := NewPublicKeyInfinity()
	if err != nil {
		t.Fatalf("NewPublicKeyInfinity: %v", err)
	}
	defer pk.Free()

	isInf, err := pk.IsInfinity()
	if err != nil {
		t.Fatalf("IsInfinity: %v", err)
	}
	if !isInf {
		t.Fatal("infinity key should report as infinity")
	}
}

func TestSignatureInfinity(t *testing.T) {
	sig, err := NewSignatureInfinity()
	if err != nil {
		t.Fatalf("NewSignatureInfinity: %v", err)
	}
	defer sig.Free()

	isInf, err := sig.IsInfinity()
	if err != nil {
		t.Fatalf("IsInfinity: %v", err)
	}
	if !isInf {
		t.Fatal("infinity signature should report as infinity")
	}
}

func TestPublicKeyFingerprint(t *testing.T) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()

	pk, _ := sk.PublicKey()
	defer pk.Free()

	fp, err := pk.Fingerprint()
	if err != nil {
		t.Fatalf("Fingerprint: %v", err)
	}
	if fp == 0 {
		t.Fatal("fingerprint should not be zero for non-trivial key")
	}

	// Same seed should give same fingerprint
	sk2, _ := NewSecretKeyFromSeed(seed)
	defer sk2.Free()
	pk2, _ := sk2.PublicKey()
	defer pk2.Free()

	fp2, err := pk2.Fingerprint()
	if err != nil {
		t.Fatalf("Fingerprint2: %v", err)
	}
	if fp != fp2 {
		t.Fatal("same key should give same fingerprint")
	}
}

func TestPublicKeyUnhardenedDerivation(t *testing.T) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()

	pk, _ := sk.PublicKey()
	defer pk.Free()

	childPk, err := pk.DeriveUnhardened(0)
	if err != nil {
		t.Fatalf("DeriveUnhardened: %v", err)
	}
	defer childPk.Free()

	// Derive same path via secret key
	childSk, _ := sk.DeriveUnhardened(0)
	defer childSk.Free()
	childPkFromSk, _ := childSk.PublicKey()
	defer childPkFromSk.Free()

	pkBytes, _ := childPk.Bytes()
	pkFromSkBytes, _ := childPkFromSk.Bytes()
	if !bytes.Equal(pkBytes, pkFromSkBytes) {
		t.Fatal("unhardened PK derivation should match SK derivation")
	}
}

func TestSignatureRoundtrip(t *testing.T) {
	seed := make([]byte, 32)
	seed[0] = 5
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()

	sig, _ := sk.Sign([]byte("test"))
	defer sig.Free()

	sigBytes, _ := sig.Bytes()

	sig2, err := NewSignatureFromBytes(sigBytes)
	if err != nil {
		t.Fatalf("NewSignatureFromBytes: %v", err)
	}
	defer sig2.Free()

	sigBytes2, _ := sig2.Bytes()
	if !bytes.Equal(sigBytes, sigBytes2) {
		t.Fatal("signature bytes should survive roundtrip")
	}
}

// ── K1 (secp256k1) Cryptography ─────────────────────────────────────────

func TestK1KeyPair(t *testing.T) {
	// 32-byte private key (must be valid for secp256k1)
	skBytes := sha256.Sum256([]byte("k1 test key"))
	sk, err := NewK1SecretKeyFromBytes(skBytes[:])
	if err != nil {
		t.Fatalf("NewK1SecretKeyFromBytes: %v", err)
	}
	defer sk.Free()

	pk, err := sk.PublicKey()
	if err != nil {
		t.Fatalf("PublicKey: %v", err)
	}
	defer pk.Free()

	pkBytes, err := pk.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	if len(pkBytes) != 33 {
		t.Fatalf("expected 33 byte compressed K1 pubkey, got %d", len(pkBytes))
	}

	fp, err := pk.Fingerprint()
	if err != nil {
		t.Fatalf("Fingerprint: %v", err)
	}
	if fp == 0 {
		t.Fatal("fingerprint should not be zero")
	}
}

func TestK1SignAndVerify(t *testing.T) {
	skBytes := sha256.Sum256([]byte("k1 sign test"))
	sk, _ := NewK1SecretKeyFromBytes(skBytes[:])
	defer sk.Free()

	pk, _ := sk.PublicKey()
	defer pk.Free()

	// K1 uses prehashed messages
	msgHash := sha256.Sum256([]byte("hello k1"))
	sig, err := sk.SignPrehashed(msgHash[:])
	if err != nil {
		t.Fatalf("SignPrehashed: %v", err)
	}
	defer sig.Free()

	sigBytes, err := sig.Bytes()
	if err != nil {
		t.Fatalf("sig ToBytes: %v", err)
	}
	if len(sigBytes) != 64 {
		t.Fatalf("expected 64 byte K1 signature, got %d", len(sigBytes))
	}

	valid, err := pk.VerifyPrehashed(msgHash[:], sig)
	if err != nil {
		t.Fatalf("VerifyPrehashed: %v", err)
	}
	if !valid {
		t.Fatal("K1 signature should verify")
	}

	// Wrong message
	wrongHash := sha256.Sum256([]byte("wrong"))
	valid, err = pk.VerifyPrehashed(wrongHash[:], sig)
	if err != nil {
		t.Fatalf("VerifyPrehashed wrong: %v", err)
	}
	if valid {
		t.Fatal("K1 signature should not verify with wrong hash")
	}
}

func TestK1KeyRoundtrip(t *testing.T) {
	skBytes := sha256.Sum256([]byte("k1 roundtrip"))
	sk, _ := NewK1SecretKeyFromBytes(skBytes[:])
	defer sk.Free()

	exported, _ := sk.Bytes()
	sk2, err := NewK1SecretKeyFromBytes(exported)
	if err != nil {
		t.Fatalf("NewK1SecretKeyFromBytes roundtrip: %v", err)
	}
	defer sk2.Free()

	exported2, _ := sk2.Bytes()
	if !bytes.Equal(exported, exported2) {
		t.Fatal("K1 secret key roundtrip failed")
	}
}

// ── Mnemonic ────────────────────────────────────────────────────────────

func TestMnemonic24Words(t *testing.T) {
	mnemonic, err := NewMnemonicGenerate(true)
	if err != nil {
		t.Fatalf("NewMnemonicGenerate(24): %v", err)
	}
	defer mnemonic.Free()

	s, err := mnemonic.String()
	if err != nil {
		t.Fatalf("String: %v", err)
	}

	words := splitWords(s)
	if len(words) != 24 {
		t.Fatalf("expected 24 words, got %d", len(words))
	}
}

func TestMnemonic12Words(t *testing.T) {
	mnemonic, err := NewMnemonicGenerate(false)
	if err != nil {
		t.Fatalf("NewMnemonicGenerate(12): %v", err)
	}
	defer mnemonic.Free()

	s, err := mnemonic.String()
	if err != nil {
		t.Fatalf("String: %v", err)
	}

	words := splitWords(s)
	if len(words) != 12 {
		t.Fatalf("expected 12 words, got %d", len(words))
	}
}

func TestMnemonicRoundtrip(t *testing.T) {
	mnemonic, _ := NewMnemonicGenerate(true)
	defer mnemonic.Free()

	s, _ := mnemonic.String()

	ok, err := MnemonicVerify(s)
	if err != nil {
		t.Fatalf("MnemonicVerify: %v", err)
	}
	if !ok {
		t.Fatal("generated mnemonic should be valid")
	}

	// Parse it back
	m2, err := MnemonicNew(s)
	if err != nil {
		t.Fatalf("MnemonicNew: %v", err)
	}
	defer m2.Free()

	s2, _ := m2.String()
	if s != s2 {
		t.Fatal("mnemonic roundtrip failed")
	}
}

func TestMnemonicToSeed(t *testing.T) {
	mnemonic, _ := NewMnemonicGenerate(true)
	defer mnemonic.Free()

	seed, err := mnemonic.ToSeed("")
	if err != nil {
		t.Fatalf("ToSeed: %v", err)
	}
	if len(seed) != 64 {
		t.Fatalf("expected 64 byte seed, got %d", len(seed))
	}

	// Same mnemonic with password gives different seed
	seedWithPw, err := mnemonic.ToSeed("password")
	if err != nil {
		t.Fatalf("ToSeed with password: %v", err)
	}
	if bytes.Equal(seed, seedWithPw) {
		t.Fatal("seed with password should differ from seed without")
	}
}

func TestMnemonicEntropy(t *testing.T) {
	mnemonic, _ := NewMnemonicGenerate(true)
	defer mnemonic.Free()

	entropy, err := mnemonic.Entropy()
	if err != nil {
		t.Fatalf("Entropy: %v", err)
	}
	if len(entropy) != 32 {
		t.Fatalf("expected 32 byte entropy for 24 words, got %d", len(entropy))
	}

	// Recreate from entropy
	m2, err := NewMnemonicFromEntropy(entropy)
	if err != nil {
		t.Fatalf("NewMnemonicFromEntropy: %v", err)
	}
	defer m2.Free()

	s1, _ := mnemonic.String()
	s2, _ := m2.String()
	if s1 != s2 {
		t.Fatal("mnemonic from entropy should match original")
	}
}

func TestMnemonicInvalid(t *testing.T) {
	ok, err := MnemonicVerify("not a valid mnemonic phrase at all")
	if err != nil {
		t.Fatalf("MnemonicVerify: %v", err)
	}
	if ok {
		t.Fatal("invalid mnemonic should not verify")
	}
}

// ── Address ─────────────────────────────────────────────────────────────

func TestAddressRoundtrip(t *testing.T) {
	puzzleHash := make([]byte, 32)
	puzzleHash[0] = 0xab
	puzzleHash[31] = 0xcd

	addr, err := NewAddress(puzzleHash, "xch")
	if err != nil {
		t.Fatalf("NewAddress: %v", err)
	}
	defer addr.Free()

	encoded, err := addr.Encode()
	if err != nil {
		t.Fatalf("Encode: %v", err)
	}
	if encoded == "" {
		t.Fatal("encoded address should not be empty")
	}

	decoded, err := NewAddressDecode(encoded)
	if err != nil {
		t.Fatalf("NewAddressDecode: %v", err)
	}
	defer decoded.Free()

	hash, _ := decoded.PuzzleHash()
	if !bytes.Equal(hash, puzzleHash) {
		t.Fatal("puzzle hash should survive address roundtrip")
	}

	prefix, _ := decoded.Prefix()
	if prefix != "xch" {
		t.Fatalf("expected prefix 'xch', got '%s'", prefix)
	}
}

func TestAddressTestnetPrefix(t *testing.T) {
	puzzleHash := make([]byte, 32)
	addr, _ := NewAddress(puzzleHash, "txch")
	defer addr.Free()

	encoded, _ := addr.Encode()
	if len(encoded) < 4 || encoded[:4] != "txch" {
		t.Fatalf("expected txch prefix, got: %s", encoded[:10])
	}
}

// ── Coin ────────────────────────────────────────────────────────────────

func TestCoinCreation(t *testing.T) {
	parentId := make([]byte, 32)
	parentId[0] = 1
	puzzleHash := make([]byte, 32)
	puzzleHash[0] = 2

	coin, err := NewCoin(parentId, puzzleHash, 1000)
	if err != nil {
		t.Fatalf("NewCoin: %v", err)
	}
	defer coin.Free()

	gotParent, _ := coin.ParentCoinInfo()
	if !bytes.Equal(gotParent, parentId) {
		t.Fatal("parent coin info mismatch")
	}

	gotPh, _ := coin.PuzzleHash()
	if !bytes.Equal(gotPh, puzzleHash) {
		t.Fatal("puzzle hash mismatch")
	}

	gotAmount, _ := coin.Amount()
	if gotAmount != 1000 {
		t.Fatalf("expected amount 1000, got %d", gotAmount)
	}

	coinId, err := coin.CoinId()
	if err != nil {
		t.Fatalf("CoinId: %v", err)
	}
	if len(coinId) != 32 {
		t.Fatalf("expected 32 byte coin id, got %d", len(coinId))
	}
}

func TestCoinSetters(t *testing.T) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 0)
	defer coin.Free()

	newParent := make([]byte, 32)
	newParent[0] = 0xff
	coin.SetParentCoinInfo(newParent)

	newPh := make([]byte, 32)
	newPh[0] = 0xee
	coin.SetPuzzleHash(newPh)

	coin.SetAmount(9999)

	gotParent, _ := coin.ParentCoinInfo()
	if gotParent[0] != 0xff {
		t.Fatal("SetParentCoinInfo did not take effect")
	}

	gotPh, _ := coin.PuzzleHash()
	if gotPh[0] != 0xee {
		t.Fatal("SetPuzzleHash did not take effect")
	}

	gotAmount, _ := coin.Amount()
	if gotAmount != 9999 {
		t.Fatal("SetAmount did not take effect")
	}
}

func TestCoinClone(t *testing.T) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 500)
	defer coin.Free()

	clone, err := coin.Clone()
	if err != nil {
		t.Fatalf("Clone: %v", err)
	}
	defer clone.Free()

	origId, _ := coin.CoinId()
	cloneId, _ := clone.CoinId()
	if !bytes.Equal(origId, cloneId) {
		t.Fatal("cloned coin should have same id")
	}

	// Mutating clone should not affect original
	clone.SetAmount(999)
	origAmt, _ := coin.Amount()
	cloneAmt, _ := clone.Amount()
	if origAmt == cloneAmt {
		t.Fatal("clone mutation should not affect original")
	}
}

func TestCoinIdDeterministic(t *testing.T) {
	parent := make([]byte, 32)
	ph := make([]byte, 32)

	coin1, _ := NewCoin(parent, ph, 100)
	defer coin1.Free()
	coin2, _ := NewCoin(parent, ph, 100)
	defer coin2.Free()

	id1, _ := coin1.CoinId()
	id2, _ := coin2.CoinId()
	if !bytes.Equal(id1, id2) {
		t.Fatal("identical coins should have identical IDs")
	}

	coin3, _ := NewCoin(parent, ph, 200)
	defer coin3.Free()
	id3, _ := coin3.CoinId()
	if bytes.Equal(id1, id3) {
		t.Fatal("different amounts should give different IDs")
	}
}

// ── CoinState ───────────────────────────────────────────────────────────

func TestCoinState(t *testing.T) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()

	cs, err := NewCoinState(coin, nil, nil)
	if err != nil {
		t.Fatalf("NewCoinState: %v", err)
	}
	defer cs.Free()

	spent, _ := cs.SpentHeight()
	if spent != nil {
		t.Fatal("spent height should be nil")
	}

	created, _ := cs.CreatedHeight()
	if created != nil {
		t.Fatal("created height should be nil")
	}

	// Set values
	h := uint32(100)
	cs.SetCreatedHeight(&h)
	created, _ = cs.CreatedHeight()
	if created == nil || *created != 100 {
		t.Fatal("created height should be 100")
	}
}

// ── CoinSpend and SpendBundle ───────────────────────────────────────────

func TestCoinSpendCreation(t *testing.T) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()

	puzzleReveal := []byte{0x01} // minimal CLVM program
	solution := []byte{0x80}     // nil

	cs, err := NewCoinSpend(coin, puzzleReveal, solution)
	if err != nil {
		t.Fatalf("NewCoinSpend: %v", err)
	}
	defer cs.Free()

	gotCoin, _ := cs.Coin()
	defer gotCoin.Free()

	gotAmt, _ := gotCoin.Amount()
	if gotAmt != 100 {
		t.Fatalf("expected amount 100, got %d", gotAmt)
	}

	pr, _ := cs.PuzzleReveal()
	if !bytes.Equal(pr, []byte{0x01}) {
		t.Fatal("puzzle reveal mismatch")
	}

	sol, _ := cs.Solution()
	if !bytes.Equal(sol, []byte{0x80}) {
		t.Fatal("solution mismatch")
	}
}

func TestSpendBundleCreation(t *testing.T) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()

	cs, _ := NewCoinSpend(coin, []byte{0x01}, []byte{0x80})
	defer cs.Free()

	emptySig, _ := NewSignatureInfinity()
	defer emptySig.Free()

	sb, err := NewSpendBundle([]*CoinSpend{cs}, emptySig)
	if err != nil {
		t.Fatalf("NewSpendBundle: %v", err)
	}
	defer sb.Free()

	spends, _ := sb.CoinSpends()
	if len(spends) != 1 {
		t.Fatalf("expected 1 coin spend, got %d", len(spends))
	}
	for _, s := range spends {
		s.Free()
	}

	// Test serialization roundtrip
	sbBytes, err := sb.Bytes()
	if err != nil {
		t.Fatalf("Bytes: %v", err)
	}
	if len(sbBytes) == 0 {
		t.Fatal("spend bundle bytes should not be empty")
	}

	sb2, err := NewSpendBundleFromBytes(sbBytes)
	if err != nil {
		t.Fatalf("NewSpendBundleFromBytes: %v", err)
	}
	defer sb2.Free()

	spends2, _ := sb2.CoinSpends()
	if len(spends2) != 1 {
		t.Fatalf("roundtrip: expected 1 coin spend, got %d", len(spends2))
	}
	for _, s := range spends2 {
		s.Free()
	}
}

// ── CLVM / Program ──────────────────────────────────────────────────────

func TestClvmBasics(t *testing.T) {
	clvm, err := ClvmNew()
	if err != nil {
		t.Fatalf("ClvmNew: %v", err)
	}
	defer clvm.Free()

	// Nil program
	nilProg, err := clvm.Nil()
	if err != nil {
		t.Fatalf("Nil: %v", err)
	}
	defer nilProg.Free()

	nilBytes, _ := nilProg.Serialize()
	if !bytes.Equal(nilBytes, []byte{0x80}) {
		t.Fatalf("nil program should serialize to 0x80, got %x", nilBytes)
	}
}

func TestClvmParseSexp(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	// Parse a simple atom
	prog, err := clvm.Parse("42")
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	defer prog.Free()

	serialized, _ := prog.Serialize()
	if len(serialized) == 0 {
		t.Fatal("serialized program should not be empty")
	}
}

func TestClvmPair(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	nilProg, _ := clvm.Nil()
	defer nilProg.Free()

	pair, err := clvm.Pair(nilProg, nilProg)
	if err != nil {
		t.Fatalf("Pair: %v", err)
	}
	defer pair.Free()

	pairBytes, _ := pair.Serialize()
	// (nil . nil) = 0xff 0x80 0x80
	if !bytes.Equal(pairBytes, []byte{0xff, 0x80, 0x80}) {
		t.Fatalf("expected ff 80 80, got %x", pairBytes)
	}
}

func TestClvmDeserialize(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	// Deserialize nil
	prog, err := clvm.Deserialize([]byte{0x80})
	if err != nil {
		t.Fatalf("Deserialize: %v", err)
	}
	defer prog.Free()

	serialized, _ := prog.Serialize()
	if !bytes.Equal(serialized, []byte{0x80}) {
		t.Fatal("deserialize/serialize roundtrip failed")
	}
}

func TestProgramTreeHash(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	nilProg, _ := clvm.Nil()
	defer nilProg.Free()

	hash, err := nilProg.TreeHash()
	if err != nil {
		t.Fatalf("TreeHash: %v", err)
	}
	if len(hash) != 32 {
		t.Fatalf("expected 32 byte tree hash, got %d", len(hash))
	}

	// Same program should give same hash
	nilProg2, _ := clvm.Nil()
	defer nilProg2.Free()

	hash2, _ := nilProg2.TreeHash()
	if !bytes.Equal(hash, hash2) {
		t.Fatal("same program should give same tree hash")
	}
}

// ── Simulator ───────────────────────────────────────────────────────────

func TestSimulatorBasics(t *testing.T) {
	sim, err := SimulatorNew()
	if err != nil {
		t.Fatalf("SimulatorNew: %v", err)
	}
	defer sim.Free()

	height, err := sim.Height()
	if err != nil {
		t.Fatalf("Height: %v", err)
	}
	if height != 0 {
		t.Fatalf("initial height should be 0, got %d", height)
	}

	headerHash, err := sim.HeaderHash()
	if err != nil {
		t.Fatalf("HeaderHash: %v", err)
	}
	if len(headerHash) != 32 {
		t.Fatalf("expected 32 byte header hash, got %d", len(headerHash))
	}
}

func TestSimulatorWithSeed(t *testing.T) {
	sim1, _ := NewSimulatorWithSeed(42)
	defer sim1.Free()
	sim2, _ := NewSimulatorWithSeed(42)
	defer sim2.Free()

	h1, _ := sim1.HeaderHash()
	h2, _ := sim2.HeaderHash()
	if !bytes.Equal(h1, h2) {
		t.Fatal("simulators with same seed should have same header hash")
	}
}

func TestSimulatorNewCoin(t *testing.T) {
	sim, _ := SimulatorNew()
	defer sim.Free()

	puzzleHash := make([]byte, 32)
	puzzleHash[0] = 0x42

	coin, err := sim.NewCoin(puzzleHash, 1000000)
	if err != nil {
		t.Fatalf("NewCoin: %v", err)
	}
	defer coin.Free()

	amt, _ := coin.Amount()
	if amt != 1000000 {
		t.Fatalf("expected amount 1000000, got %d", amt)
	}

	ph, _ := coin.PuzzleHash()
	if !bytes.Equal(ph, puzzleHash) {
		t.Fatal("puzzle hash mismatch")
	}
}

func TestSimulatorInsertCoin(t *testing.T) {
	sim, _ := SimulatorNew()
	defer sim.Free()

	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 500)
	defer coin.Free()

	err := sim.InsertCoin(coin)
	if err != nil {
		t.Fatalf("InsertCoin: %v", err)
	}

	coinId, _ := coin.CoinId()
	cs, err := sim.CoinState(coinId)
	if err != nil {
		t.Fatalf("CoinState: %v", err)
	}
	defer cs.Free()

	gotCoin, _ := cs.Coin()
	defer gotCoin.Free()
	gotAmt, _ := gotCoin.Amount()
	if gotAmt != 500 {
		t.Fatalf("expected amount 500, got %d", gotAmt)
	}
}

func TestSimulatorBls(t *testing.T) {
	sim, _ := SimulatorNew()
	defer sim.Free()

	pair, err := sim.Bls(2000000000000)
	if err != nil {
		t.Fatalf("Bls: %v", err)
	}
	defer pair.Free()

	sk, _ := pair.Sk()
	defer sk.Free()

	pk, _ := pair.Pk()
	defer pk.Free()

	coin, _ := pair.Coin()
	defer coin.Free()

	amt, _ := coin.Amount()
	if amt != 2000000000000 {
		t.Fatalf("expected 2000000000000, got %d", amt)
	}

	puzzleHash, _ := pair.PuzzleHash()
	if len(puzzleHash) != 32 {
		t.Fatalf("expected 32 byte puzzle hash, got %d", len(puzzleHash))
	}
}

func TestSimulatorTimestamp(t *testing.T) {
	sim, _ := SimulatorNew()
	defer sim.Free()

	ts, _ := sim.NextTimestamp()

	err := sim.PassTime(60)
	if err != nil {
		t.Fatalf("PassTime: %v", err)
	}

	ts2, _ := sim.NextTimestamp()
	if ts2 != ts+60 {
		t.Fatalf("expected timestamp %d, got %d", ts+60, ts2)
	}
}

// ── Simulator Spending ──────────────────────────────────────────────────

func TestSimulatorSpendXch(t *testing.T) {
	sim, _ := SimulatorNew()
	defer sim.Free()

	// Create a BLS key pair with a coin
	pair, _ := sim.Bls(1000000)
	defer pair.Free()

	sk, _ := pair.Sk()
	defer sk.Free()

	pk, _ := pair.Pk()
	defer pk.Free()

	coin, _ := pair.Coin()
	defer coin.Free()

	clvm, _ := ClvmNew()
	defer clvm.Free()

	// Build a simple spend: send all to a new puzzle hash
	destPh := make([]byte, 32)
	destPh[0] = 0xaa

	createCoin, _ := clvm.CreateCoin(destPh, 1000000, nil)
	defer createCoin.Free()

	// The Bls() pair already uses a synthetic key internally
	delegated, _ := clvm.DelegatedSpend([]*Program{createCoin})
	defer delegated.Free()

	spend, _ := clvm.StandardSpend(pk, delegated)
	defer spend.Free()

	clvm.SpendCoin(coin, spend)

	coinSpends, _ := clvm.CoinSpends()
	defer func() {
		for _, cs := range coinSpends {
			cs.Free()
		}
	}()

	err := sim.SpendCoins(coinSpends, []*SecretKey{sk})
	if err != nil {
		t.Fatalf("SpendCoins: %v", err)
	}

	// Verify the coin was spent
	coinId, _ := coin.CoinId()
	cs, _ := sim.CoinState(coinId)
	defer cs.Free()

	spentHeight, _ := cs.SpentHeight()
	if spentHeight == nil {
		t.Fatal("coin should be spent")
	}

	// Verify the new coin was created
	children, _ := sim.Children(coinId)
	if len(children) == 0 {
		t.Fatal("should have created child coins")
	}
	for _, c := range children {
		c.Free()
	}
}

// ── Action / Spends builder ─────────────────────────────────────────────

func TestActionFee(t *testing.T) {
	action, err := NewActionFee(1000)
	if err != nil {
		t.Fatalf("NewActionFee: %v", err)
	}
	defer action.Free()

	clone, err := action.Clone()
	if err != nil {
		t.Fatalf("Clone: %v", err)
	}
	defer clone.Free()
}

func TestIdTypes(t *testing.T) {
	// XCH ID
	xchId, err := NewIdXch()
	if err != nil {
		t.Fatalf("NewIdXch: %v", err)
	}
	defer xchId.Free()

	// New asset ID
	newId, err := NewIdNew(0)
	if err != nil {
		t.Fatalf("NewIdNew: %v", err)
	}
	defer newId.Free()

	// Existing asset ID
	assetId := make([]byte, 32)
	assetId[0] = 0x42
	existingId, err := NewIdExisting(assetId)
	if err != nil {
		t.Fatalf("NewIdExisting: %v", err)
	}
	defer existingId.Free()
}

func TestDelta(t *testing.T) {
	delta, err := NewDelta(1000, 500)
	if err != nil {
		t.Fatalf("NewDelta: %v", err)
	}
	defer delta.Free()

	clone, _ := delta.Clone()
	defer clone.Free()
}

// ── Enums ───────────────────────────────────────────────────────────────

func TestTransferTypeEnum(t *testing.T) {
	sent, err := NewTransferTypeSent()
	if err != nil {
		t.Fatalf("NewTransferTypeSent: %v", err)
	}
	defer sent.Free()

	val, err := sent.ToInt()
	if err != nil {
		t.Fatalf("ToInt: %v", err)
	}
	if val != TransferTypeValueSent {
		t.Fatalf("expected TransferTypeValueSent (%d), got %d", TransferTypeValueSent, val)
	}

	// Roundtrip
	sent2, err := NewTransferTypeFromInt(val)
	if err != nil {
		t.Fatalf("NewTransferTypeFromInt: %v", err)
	}
	defer sent2.Free()

	val2, _ := sent2.ToInt()
	if val != val2 {
		t.Fatal("enum roundtrip failed")
	}
}

func TestTransferTypeAllVariants(t *testing.T) {
	variants := []struct {
		name  string
		value int
		ctor  func() (*TransferType, error)
	}{
		{"Sent", TransferTypeValueSent, NewTransferTypeSent},
		{"Burned", TransferTypeValueBurned, NewTransferTypeBurned},
		{"Offered", TransferTypeValueOffered, NewTransferTypeOffered},
		{"Received", TransferTypeValueReceived, NewTransferTypeReceived},
		{"Updated", TransferTypeValueUpdated, NewTransferTypeUpdated},
	}

	for _, v := range variants {
		t.Run(v.name, func(t *testing.T) {
			tt, err := v.ctor()
			if err != nil {
				t.Fatalf("New%s: %v", v.name, err)
			}
			defer tt.Free()

			got, _ := tt.ToInt()
			if got != v.value {
				t.Fatalf("expected %d, got %d", v.value, got)
			}
		})
	}
}

// ── Constants ───────────────────────────────────────────────────────────

func TestConstants(t *testing.T) {
	bls, err := ConstantsBlsMember()
	if err != nil {
		t.Fatalf("ConstantsBlsMember: %v", err)
	}
	if len(bls) == 0 {
		t.Fatal("BLS member puzzle should not be empty")
	}

	blsHash, err := ConstantsBlsMemberHash()
	if err != nil {
		t.Fatalf("ConstantsBlsMemberHash: %v", err)
	}
	if len(blsHash) != 32 {
		t.Fatalf("expected 32 byte hash, got %d", len(blsHash))
	}
}

// ── CLVM Conditions ─────────────────────────────────────────────────────

func TestClvmConditions(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	// CreateCoin
	ph := make([]byte, 32)
	cc, err := clvm.CreateCoin(ph, 100, nil)
	if err != nil {
		t.Fatalf("CreateCoin: %v", err)
	}
	defer cc.Free()

	// ReserveFee
	fee, err := clvm.ReserveFee(50)
	if err != nil {
		t.Fatalf("ReserveFee: %v", err)
	}
	defer fee.Free()

	// AssertCoinAnnouncement
	announcementId := make([]byte, 32)
	aca, err := clvm.AssertCoinAnnouncement(announcementId)
	if err != nil {
		t.Fatalf("AssertCoinAnnouncement: %v", err)
	}
	defer aca.Free()

	// AssertPuzzleAnnouncement
	apa, err := clvm.AssertPuzzleAnnouncement(announcementId)
	if err != nil {
		t.Fatalf("AssertPuzzleAnnouncement: %v", err)
	}
	defer apa.Free()

	// CreateCoinAnnouncement
	cca, err := clvm.CreateCoinAnnouncement([]byte("hello"))
	if err != nil {
		t.Fatalf("CreateCoinAnnouncement: %v", err)
	}
	defer cca.Free()

	// CreatePuzzleAnnouncement
	cpa, err := clvm.CreatePuzzleAnnouncement([]byte("world"))
	if err != nil {
		t.Fatalf("CreatePuzzleAnnouncement: %v", err)
	}
	defer cpa.Free()
}

func TestClvmAssertHeight(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	// AssertHeightAbsolute
	aha, err := clvm.AssertHeightAbsolute(100)
	if err != nil {
		t.Fatalf("AssertHeightAbsolute: %v", err)
	}
	defer aha.Free()

	// AssertHeightRelative
	ahr, err := clvm.AssertHeightRelative(10)
	if err != nil {
		t.Fatalf("AssertHeightRelative: %v", err)
	}
	defer ahr.Free()

	// AssertSecondsAbsolute
	asa, err := clvm.AssertSecondsAbsolute(1000000)
	if err != nil {
		t.Fatalf("AssertSecondsAbsolute: %v", err)
	}
	defer asa.Free()

	// AssertSecondsRelative
	asr, err := clvm.AssertSecondsRelative(60)
	if err != nil {
		t.Fatalf("AssertSecondsRelative: %v", err)
	}
	defer asr.Free()
}

func TestClvmRemark(t *testing.T) {
	clvm, _ := ClvmNew()
	defer clvm.Free()

	nilProg, _ := clvm.Nil()
	defer nilProg.Free()

	remark, err := clvm.Remark(nilProg)
	if err != nil {
		t.Fatalf("Remark: %v", err)
	}
	defer remark.Free()

	hash, _ := remark.TreeHash()
	if len(hash) != 32 {
		t.Fatal("remark should have a valid tree hash")
	}
}

// ── Offer Encoding ──────────────────────────────────────────────────────

func TestOfferEncoding(t *testing.T) {
	// Create a minimal spend bundle
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()

	cs, _ := NewCoinSpend(coin, []byte{0x01}, []byte{0x80})
	defer cs.Free()

	emptySig, _ := NewSignatureInfinity()
	defer emptySig.Free()

	sb, _ := NewSpendBundle([]*CoinSpend{cs}, emptySig)
	defer sb.Free()

	encoded, err := EncodeOffer(sb)
	if err != nil {
		t.Fatalf("EncodeOffer: %v", err)
	}
	if encoded == "" {
		t.Fatal("encoded offer should not be empty")
	}

	decoded, err := DecodeOffer(encoded)
	if err != nil {
		t.Fatalf("DecodeOffer: %v", err)
	}
	defer decoded.Free()

	spends, _ := decoded.CoinSpends()
	if len(spends) != 1 {
		t.Fatalf("expected 1 coin spend, got %d", len(spends))
	}
	for _, s := range spends {
		s.Free()
	}
}

// ── Action System: XCH Send ─────────────────────────────────────────────

// buildPendingSpends fills in all pending spends for a FinishedSpends by
// building DelegatedSpend → StandardSpend for each, using the given public key.
func buildPendingSpends(t *testing.T, clvm *Clvm, finished *FinishedSpends, pk *PublicKey) {
	t.Helper()
	pending, err := finished.PendingSpends()
	if err != nil {
		t.Fatalf("PendingSpends: %v", err)
	}
	for _, ps := range pending {
		conditions, _ := ps.Conditions()
		coinObj, _ := ps.Coin()
		coinId, _ := coinObj.CoinId()

		delegated, err := clvm.DelegatedSpend(conditions)
		if err != nil {
			t.Fatalf("DelegatedSpend: %v", err)
		}
		spend, err := clvm.StandardSpend(pk, delegated)
		if err != nil {
			t.Fatalf("StandardSpend: %v", err)
		}
		if err := finished.Insert(coinId, spend); err != nil {
			t.Fatalf("Insert: %v", err)
		}

		for _, c := range conditions {
			c.Free()
		}
		spend.Free()
		delegated.Free()
		coinObj.Free()
		ps.Free()
	}
}

func TestActionXchSend(t *testing.T) {
	sim, err := SimulatorNew()
	if err != nil {
		t.Fatalf("SimulatorNew: %v", err)
	}
	defer sim.Close()

	pair, err := sim.Bls(1_000_000)
	if err != nil {
		t.Fatalf("Bls: %v", err)
	}
	defer pair.Close()

	sk, _ := pair.Sk()
	defer sk.Close()
	pk, _ := pair.Pk()
	defer pk.Close()
	coin, _ := pair.Coin()
	defer coin.Close()
	puzzleHash, _ := pair.PuzzleHash()

	clvm, _ := ClvmNew()
	defer clvm.Close()

	spends, err := SpendsNew(clvm, puzzleHash)
	if err != nil {
		t.Fatalf("SpendsNew: %v", err)
	}
	defer spends.Close()
	spends.AddXch(coin)

	destPh := make([]byte, 32)
	destPh[0] = 0xbb
	xchId, _ := NewIdXch()
	defer xchId.Close()
	send, _ := NewActionSend(xchId, destPh, 500_000, nil)
	defer send.Close()
	fee, _ := NewActionFee(100)
	defer fee.Close()

	deltas, err := spends.Apply([]*Action{send, fee})
	if err != nil {
		t.Fatalf("Apply: %v", err)
	}
	defer deltas.Close()

	finished, err := spends.Prepare(deltas)
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	defer finished.Close()

	buildPendingSpends(t, clvm, finished, pk)

	outputs, err := finished.Spend()
	if err != nil {
		t.Fatalf("Spend: %v", err)
	}
	defer outputs.Close()

	// Verify XCH outputs: should have the 500k send + change
	xchCoins, err := outputs.Xch()
	if err != nil {
		t.Fatalf("Xch: %v", err)
	}
	if len(xchCoins) == 0 {
		t.Fatal("expected XCH output coins")
	}
	for _, c := range xchCoins {
		c.Free()
	}

	// Confirm on simulator
	coinSpends, _ := clvm.CoinSpends()
	defer func() {
		for _, cs := range coinSpends {
			cs.Free()
		}
	}()
	if err := sim.SpendCoins(coinSpends, []*SecretKey{sk}); err != nil {
		t.Fatalf("SpendCoins: %v", err)
	}

	h, _ := sim.Height()
	if h == 0 {
		t.Fatal("expected height > 0 after spending")
	}
}

// ── Action System: CAT Issuance ─────────────────────────────────────────

func TestActionCatIssuance(t *testing.T) {
	sim, err := SimulatorNew()
	if err != nil {
		t.Fatalf("SimulatorNew: %v", err)
	}
	defer sim.Close()

	pair, err := sim.Bls(1_000_000)
	if err != nil {
		t.Fatalf("Bls: %v", err)
	}
	defer pair.Close()

	sk, _ := pair.Sk()
	defer sk.Close()
	pk, _ := pair.Pk()
	defer pk.Close()
	coin, _ := pair.Coin()
	defer coin.Close()
	puzzleHash, _ := pair.PuzzleHash()

	clvm, _ := ClvmNew()
	defer clvm.Close()

	spends, err := SpendsNew(clvm, puzzleHash)
	if err != nil {
		t.Fatalf("SpendsNew: %v", err)
	}
	defer spends.Close()
	spends.AddXch(coin)

	issueCat, err := NewActionSingleIssueCat(puzzleHash, 10_000)
	if err != nil {
		t.Fatalf("NewActionSingleIssueCat: %v", err)
	}
	defer issueCat.Close()
	fee, _ := NewActionFee(100)
	defer fee.Close()

	deltas, err := spends.Apply([]*Action{issueCat, fee})
	if err != nil {
		t.Fatalf("Apply: %v", err)
	}
	defer deltas.Close()

	finished, err := spends.Prepare(deltas)
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	defer finished.Close()

	buildPendingSpends(t, clvm, finished, pk)

	outputs, err := finished.Spend()
	if err != nil {
		t.Fatalf("Spend: %v", err)
	}
	defer outputs.Close()

	// Verify CAT outputs exist
	catIds, err := outputs.Cats()
	if err != nil {
		t.Fatalf("Cats: %v", err)
	}
	if len(catIds) != 1 {
		t.Fatalf("expected 1 CAT asset, got %d", len(catIds))
	}

	cats, err := outputs.Cat(catIds[0])
	if err != nil {
		t.Fatalf("Cat: %v", err)
	}
	if len(cats) == 0 {
		t.Fatal("expected CAT coins")
	}

	// Verify the CAT coin has the right amount
	catCoin, _ := cats[0].Coin()
	defer catCoin.Close()
	catAmount, _ := catCoin.Amount()
	if catAmount != 10_000 {
		t.Fatalf("expected CAT amount 10000, got %d", catAmount)
	}

	for _, c := range cats {
		c.Free()
	}
	for _, id := range catIds {
		id.Free()
	}

	// Confirm on simulator
	coinSpends, _ := clvm.CoinSpends()
	defer func() {
		for _, cs := range coinSpends {
			cs.Free()
		}
	}()
	if err := sim.SpendCoins(coinSpends, []*SecretKey{sk}); err != nil {
		t.Fatalf("SpendCoins: %v", err)
	}
}

func TestActionNftMint(t *testing.T) {
	sim, err := SimulatorNew()
	if err != nil {
		t.Fatalf("SimulatorNew: %v", err)
	}
	defer sim.Close()

	pair, err := sim.Bls(1_000_000)
	if err != nil {
		t.Fatalf("Bls: %v", err)
	}
	defer pair.Close()

	sk, _ := pair.Sk()
	defer sk.Close()
	pk, _ := pair.Pk()
	defer pk.Close()
	coin, _ := pair.Coin()
	defer coin.Close()
	puzzleHash, _ := pair.PuzzleHash()

	clvm, _ := ClvmNew()
	defer clvm.Close()

	// Create NFT metadata with Vec<String> parameters
	metadata, err := NewNftMetadata(
		1,    // edition number
		1,    // edition total
		[]string{"https://example.com/nft.png"}, // data_uris
		nil, // data_hash
		[]string{"https://example.com/metadata.json"}, // metadata_uris
		nil, // metadata_hash
		[]string{}, // license_uris (empty)
		nil, // license_hash
	)
	if err != nil {
		t.Fatalf("NewNftMetadata: %v", err)
	}
	defer metadata.Close()

	// Verify we can read back the string fields
	dataUris, err := metadata.DataUris()
	if err != nil {
		t.Fatalf("DataUris: %v", err)
	}
	if len(dataUris) != 1 || dataUris[0] != "https://example.com/nft.png" {
		t.Fatalf("expected data_uris [https://example.com/nft.png], got %v", dataUris)
	}

	metadataUris, err := metadata.MetadataUris()
	if err != nil {
		t.Fatalf("MetadataUris: %v", err)
	}
	if len(metadataUris) != 1 || metadataUris[0] != "https://example.com/metadata.json" {
		t.Fatalf("expected metadata_uris [https://example.com/metadata.json], got %v", metadataUris)
	}

	licenseUris, err := metadata.LicenseUris()
	if err != nil {
		t.Fatalf("LicenseUris: %v", err)
	}
	if len(licenseUris) != 0 {
		t.Fatalf("expected empty license_uris, got %v", licenseUris)
	}

	// Convert metadata to CLVM program
	metadataProgram, err := clvm.NftMetadata(metadata)
	if err != nil {
		t.Fatalf("NftMetadata: %v", err)
	}
	defer metadataProgram.Close()

	// Get metadata updater puzzle hash
	updater, err := clvm.NftMetadataUpdaterDefault()
	if err != nil {
		t.Fatalf("NftMetadataUpdaterDefault: %v", err)
	}
	defer updater.Close()
	updaterHash, err := updater.TreeHash()
	if err != nil {
		t.Fatalf("TreeHash: %v", err)
	}

	spends, err := SpendsNew(clvm, puzzleHash)
	if err != nil {
		t.Fatalf("SpendsNew: %v", err)
	}
	defer spends.Close()
	spends.AddXch(coin)

	mintNft, err := NewActionMintNft(clvm, metadataProgram, updaterHash, puzzleHash, 0, 1, nil)
	if err != nil {
		t.Fatalf("NewActionMintNft: %v", err)
	}
	defer mintNft.Close()
	fee, _ := NewActionFee(100)
	defer fee.Close()

	deltas, err := spends.Apply([]*Action{mintNft, fee})
	if err != nil {
		t.Fatalf("Apply: %v", err)
	}
	defer deltas.Close()

	finished, err := spends.Prepare(deltas)
	if err != nil {
		t.Fatalf("Prepare: %v", err)
	}
	defer finished.Close()

	buildPendingSpends(t, clvm, finished, pk)

	outputs, err := finished.Spend()
	if err != nil {
		t.Fatalf("Spend: %v", err)
	}
	defer outputs.Close()

	// Verify NFT outputs exist
	nftIds, err := outputs.Nfts()
	if err != nil {
		t.Fatalf("Nfts: %v", err)
	}
	if len(nftIds) != 1 {
		t.Fatalf("expected 1 NFT, got %d", len(nftIds))
	}

	nft, err := outputs.Nft(nftIds[0])
	if err != nil {
		t.Fatalf("Nft: %v", err)
	}
	defer nft.Close()

	for _, id := range nftIds {
		id.Free()
	}

	// Confirm on simulator
	coinSpends, _ := clvm.CoinSpends()
	defer func() {
		for _, cs := range coinSpends {
			cs.Free()
		}
	}()
	if err := sim.SpendCoins(coinSpends, []*SecretKey{sk}); err != nil {
		t.Fatalf("SpendCoins: %v", err)
	}
}

// ── Object Lifecycle ────────────────────────────────────────────────────

func TestFreeNilSafety(t *testing.T) {
	// Free on nil should not panic
	var sk *SecretKey
	sk.Free() // should be no-op

	var pk *PublicKey
	pk.Free()

	var sig *Signature
	sig.Free()

	var coin *Coin
	coin.Free()
}

func TestDoubleFree(t *testing.T) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)

	sk.Free()
	sk.Free() // second free should be no-op
}

func TestCloseIdempotent(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}

	if err := sk.Close(); err != nil {
		t.Fatalf("first Close: %v", err)
	}
	if err := sk.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}

	coin, err := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	if err != nil {
		t.Fatalf("NewCoin: %v", err)
	}
	if err := coin.Close(); err != nil {
		t.Fatalf("coin Close: %v", err)
	}
	if err := coin.Close(); err != nil {
		t.Fatalf("coin double Close: %v", err)
	}
}

func TestCloseNilSafety(t *testing.T) {
	var sk *SecretKey
	if err := sk.Close(); err != nil {
		t.Fatalf("nil SecretKey Close: %v", err)
	}

	var coin *Coin
	if err := coin.Close(); err != nil {
		t.Fatalf("nil Coin Close: %v", err)
	}

	var clvm *Clvm
	if err := clvm.Close(); err != nil {
		t.Fatalf("nil Clvm Close: %v", err)
	}
}

func TestCloseImplementsIoCloser(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}

	// Compile-time check: *SecretKey satisfies io.Closer
	var closer interface{ Close() error } = sk
	if err := closer.Close(); err != nil {
		t.Fatalf("Close via interface: %v", err)
	}
}

// ── Error Paths ─────────────────────────────────────────────────────────

func TestInvalidSeedLength(t *testing.T) {
	// Too short
	_, err := NewSecretKeyFromSeed(make([]byte, 16))
	if err == nil {
		t.Fatal("expected error for 16-byte seed")
	}

	// Empty
	_, err = NewSecretKeyFromSeed(nil)
	if err == nil {
		t.Fatal("expected error for nil seed")
	}
}

func TestInvalidKeyBytes(t *testing.T) {
	// Invalid public key bytes (wrong length)
	_, err := NewPublicKeyFromBytes(make([]byte, 10))
	if err == nil {
		t.Fatal("expected error for 10-byte public key")
	}

	// Invalid secret key bytes (wrong length)
	_, err = NewSecretKeyFromBytes(make([]byte, 10))
	if err == nil {
		t.Fatal("expected error for 10-byte secret key")
	}

	// Invalid signature bytes (wrong length)
	_, err = NewSignatureFromBytes(make([]byte, 10))
	if err == nil {
		t.Fatal("expected error for 10-byte signature")
	}
}

func TestInvalidMnemonic(t *testing.T) {
	_, err := MnemonicNew("not a valid mnemonic phrase at all")
	if err == nil {
		t.Fatal("expected error for invalid mnemonic")
	}
}

func TestInvalidAddress(t *testing.T) {
	_, err := NewAddressDecode("notavalidaddress")
	if err == nil {
		t.Fatal("expected error for invalid address")
	}
}

func TestInvalidOfferDecode(t *testing.T) {
	_, err := DecodeOffer("not_a_valid_offer_string")
	if err == nil {
		t.Fatal("expected error for invalid offer string")
	}
}

// ── Concurrency ─────────────────────────────────────────────────────────

func TestConcurrentMethodCalls(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Close()

	var wg sync.WaitGroup
	for range 20 {
		wg.Go(func() {
			for range 50 {
				b, err := sk.Bytes()
				if err != nil {
					t.Errorf("Bytes: %v", err)
					return
				}
				if len(b) != 32 {
					t.Errorf("expected 32 bytes, got %d", len(b))
					return
				}
			}
		})
	}
	wg.Wait()
}

func TestConcurrentClvmMethodCalls(t *testing.T) {
	clvm, err := ClvmNew()
	if err != nil {
		t.Fatalf("ClvmNew: %v", err)
	}
	defer clvm.Close()

	var wg sync.WaitGroup
	for range 10 {
		wg.Go(func() {
			for range 20 {
				p, err := clvm.Nil()
				if err != nil {
					t.Errorf("Nil: %v", err)
					return
				}
				p.Close()
			}
		})
	}
	wg.Wait()
}

func TestConcurrentCloseWhileUsing(t *testing.T) {
	seed := make([]byte, 32)
	seed[0] = 99
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}

	var wg sync.WaitGroup

	// Goroutine 1: repeatedly read
	wg.Go(func() {
		for range 100 {
			_, _ = sk.Bytes() // may return error after close, that's fine
		}
	})

	// Goroutine 2: close after a few iterations
	wg.Go(func() {
		sk.Close()
	})

	wg.Wait()
}

func TestUseAfterClose(t *testing.T) {
	seed := make([]byte, 32)
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	sk.Close()

	// Should return error, not panic
	_, err = sk.Bytes()
	if err == nil {
		t.Fatal("expected error for use after close")
	}

	_, err = sk.PublicKey()
	if err == nil {
		t.Fatal("expected error for use after close")
	}
}

func TestConcurrentDoubleFree(t *testing.T) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)

	var wg sync.WaitGroup
	for range 10 {
		wg.Go(func() {
			sk.Free()
		})
	}
	wg.Wait()
}

func TestConcurrentClone(t *testing.T) {
	seed := make([]byte, 32)
	seed[0] = 42
	sk, err := NewSecretKeyFromSeed(seed)
	if err != nil {
		t.Fatalf("NewSecretKeyFromSeed: %v", err)
	}
	defer sk.Close()

	var wg sync.WaitGroup
	for range 20 {
		wg.Go(func() {
			clone, err := sk.Clone()
			if err != nil {
				t.Errorf("Clone: %v", err)
				return
			}
			defer clone.Close()
			b, err := clone.Bytes()
			if err != nil {
				t.Errorf("Bytes on clone: %v", err)
				return
			}
			if len(b) != 32 {
				t.Errorf("expected 32 bytes, got %d", len(b))
			}
		})
	}
	wg.Wait()
}

func TestConcurrentProgramMethods(t *testing.T) {
	clvm, err := ClvmNew()
	if err != nil {
		t.Fatalf("ClvmNew: %v", err)
	}
	defer clvm.Close()

	prog, err := clvm.Nil()
	if err != nil {
		t.Fatalf("Nil: %v", err)
	}
	defer prog.Close()

	var wg sync.WaitGroup
	for range 15 {
		wg.Go(func() {
			for range 20 {
				h, err := prog.TreeHash()
				if err != nil {
					t.Errorf("TreeHash: %v", err)
					return
				}
				if len(h) != 32 {
					t.Errorf("expected 32 byte hash, got %d", len(h))
					return
				}
				b, err := prog.Serialize()
				if err != nil {
					t.Errorf("Serialize: %v", err)
					return
				}
				if len(b) == 0 {
					t.Error("expected non-empty serialization")
					return
				}
			}
		})
	}
	wg.Wait()
}

// ── Helpers ─────────────────────────────────────────────────────────────

func splitWords(s string) []string {
	var words []string
	word := ""
	for _, c := range s {
		if c == ' ' {
			if word != "" {
				words = append(words, word)
				word = ""
			}
		} else {
			word += string(c)
		}
	}
	if word != "" {
		words = append(words, word)
	}
	return words
}
