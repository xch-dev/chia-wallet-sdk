package chiawalletsdk

import (
	"crypto/sha256"
	"testing"
)

// ── BLS Cryptography ────────────────────────────────────────────────────

func BenchmarkSecretKeyFromSeed(b *testing.B) {
	seed := make([]byte, 32)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		sk, _ := NewSecretKeyFromSeed(seed)
		sk.Free()
	}
}

func BenchmarkSecretKeyToPublicKey(b *testing.B) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pk, _ := sk.PublicKey()
		pk.Free()
	}
}

func BenchmarkBlsSign(b *testing.B) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	msg := []byte("benchmark message")
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		sig, _ := sk.Sign(msg)
		sig.Free()
	}
}

func BenchmarkBlsVerify(b *testing.B) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	pk, _ := sk.PublicKey()
	defer pk.Free()
	msg := []byte("benchmark message")
	sig, _ := sk.Sign(msg)
	defer sig.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pk.Verify(msg, sig)
	}
}

func BenchmarkBlsSignatureAggregate2(b *testing.B) {
	sk1, _ := NewSecretKeyFromSeed(make([]byte, 32))
	defer sk1.Free()
	sk2, _ := NewSecretKeyFromSeed(append(make([]byte, 31), 1))
	defer sk2.Free()
	sig1, _ := sk1.Sign([]byte("msg1"))
	defer sig1.Free()
	sig2, _ := sk2.Sign([]byte("msg2"))
	defer sig2.Free()
	sigs := []*Signature{sig1, sig2}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		agg, _ := NewSignatureAggregate(sigs)
		agg.Free()
	}
}

func BenchmarkBlsPublicKeyAggregate2(b *testing.B) {
	sk1, _ := NewSecretKeyFromSeed(make([]byte, 32))
	defer sk1.Free()
	sk2, _ := NewSecretKeyFromSeed(append(make([]byte, 31), 1))
	defer sk2.Free()
	pk1, _ := sk1.PublicKey()
	defer pk1.Free()
	pk2, _ := sk2.PublicKey()
	defer pk2.Free()
	pks := []*PublicKey{pk1, pk2}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		agg, _ := NewPublicKeyAggregate(pks)
		agg.Free()
	}
}

func BenchmarkDeriveHardened(b *testing.B) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		child, _ := sk.DeriveHardened(uint32(i))
		child.Free()
	}
}

func BenchmarkDeriveUnhardened(b *testing.B) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		child, _ := sk.DeriveUnhardened(uint32(i))
		child.Free()
	}
}

func BenchmarkDeriveSynthetic(b *testing.B) {
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		synth, _ := sk.DeriveSynthetic()
		synth.Free()
	}
}

// ── K1 (secp256k1) ─────────────────────────────────────────────────────

func BenchmarkK1Sign(b *testing.B) {
	skBytes := sha256.Sum256([]byte("bench k1"))
	sk, _ := NewK1SecretKeyFromBytes(skBytes[:])
	defer sk.Free()
	msgHash := sha256.Sum256([]byte("bench msg"))
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		sig, _ := sk.SignPrehashed(msgHash[:])
		sig.Free()
	}
}

func BenchmarkK1Verify(b *testing.B) {
	skBytes := sha256.Sum256([]byte("bench k1"))
	sk, _ := NewK1SecretKeyFromBytes(skBytes[:])
	defer sk.Free()
	pk, _ := sk.PublicKey()
	defer pk.Free()
	msgHash := sha256.Sum256([]byte("bench msg"))
	sig, _ := sk.SignPrehashed(msgHash[:])
	defer sig.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pk.VerifyPrehashed(msgHash[:], sig)
	}
}

// ── Mnemonic ────────────────────────────────────────────────────────────

func BenchmarkMnemonicGenerate24(b *testing.B) {
	for i := 0; i < b.N; i++ {
		m, _ := NewMnemonicGenerate(true)
		m.Free()
	}
}

func BenchmarkMnemonicToSeed(b *testing.B) {
	m, _ := NewMnemonicGenerate(true)
	defer m.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		m.ToSeed("")
	}
}

func BenchmarkMnemonicVerify(b *testing.B) {
	m, _ := NewMnemonicGenerate(true)
	defer m.Free()
	s, _ := m.String()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		MnemonicVerify(s)
	}
}

// ── Address ─────────────────────────────────────────────────────────────

func BenchmarkAddressEncode(b *testing.B) {
	ph := make([]byte, 32)
	addr, _ := NewAddress(ph, "xch")
	defer addr.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		addr.Encode()
	}
}

func BenchmarkAddressDecode(b *testing.B) {
	ph := make([]byte, 32)
	addr, _ := NewAddress(ph, "xch")
	defer addr.Free()
	encoded, _ := addr.Encode()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		a, _ := NewAddressDecode(encoded)
		a.Free()
	}
}

// ── Coin ────────────────────────────────────────────────────────────────

func BenchmarkCoinCreate(b *testing.B) {
	parent := make([]byte, 32)
	ph := make([]byte, 32)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		c, _ := NewCoin(parent, ph, 1000)
		c.Free()
	}
}

func BenchmarkCoinId(b *testing.B) {
	parent := make([]byte, 32)
	ph := make([]byte, 32)
	c, _ := NewCoin(parent, ph, 1000)
	defer c.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		c.CoinId()
	}
}

// ── CLVM / Program ──────────────────────────────────────────────────────

func BenchmarkClvmPair(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	nilp, _ := clvm.Nil()
	defer nilp.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p, _ := clvm.Pair(nilp, nilp)
		p.Free()
	}
}

func BenchmarkProgramSerialize(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	nilp, _ := clvm.Nil()
	defer nilp.Free()
	pair, _ := clvm.Pair(nilp, nilp)
	defer pair.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pair.Serialize()
	}
}

func BenchmarkProgramDeserialize(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	data := []byte{0xff, 0x80, 0x80}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p, _ := clvm.Deserialize(data)
		p.Free()
	}
}

func BenchmarkProgramTreeHash(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	nilp, _ := clvm.Nil()
	defer nilp.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		nilp.TreeHash()
	}
}

func BenchmarkClvmCreateCoin(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	ph := make([]byte, 32)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p, _ := clvm.CreateCoin(ph, 1000, nil)
		p.Free()
	}
}

// ── Conditions ──────────────────────────────────────────────────────────

func BenchmarkDelegatedSpend(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	ph := make([]byte, 32)
	cc, _ := clvm.CreateCoin(ph, 1000, nil)
	defer cc.Free()
	conditions := []*Program{cc}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		s, _ := clvm.DelegatedSpend(conditions)
		s.Free()
	}
}

func BenchmarkStandardSpend(b *testing.B) {
	clvm, _ := NewClvm()
	defer clvm.Free()
	seed := make([]byte, 32)
	sk, _ := NewSecretKeyFromSeed(seed)
	defer sk.Free()
	pk, _ := sk.PublicKey()
	defer pk.Free()
	ph := make([]byte, 32)
	cc, _ := clvm.CreateCoin(ph, 1000, nil)
	defer cc.Free()
	delegated, _ := clvm.DelegatedSpend([]*Program{cc})
	defer delegated.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		s, _ := clvm.StandardSpend(pk, delegated)
		s.Free()
	}
}

// ── SpendBundle ─────────────────────────────────────────────────────────

func BenchmarkSpendBundleSerialize(b *testing.B) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()
	cs, _ := NewCoinSpend(coin, []byte{0x01}, []byte{0x80})
	defer cs.Free()
	sig, _ := NewSignatureInfinity()
	defer sig.Free()
	sb, _ := NewSpendBundle([]*CoinSpend{cs}, sig)
	defer sb.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		sb.Bytes()
	}
}

func BenchmarkSpendBundleDeserialize(b *testing.B) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()
	cs, _ := NewCoinSpend(coin, []byte{0x01}, []byte{0x80})
	defer cs.Free()
	sig, _ := NewSignatureInfinity()
	defer sig.Free()
	sb, _ := NewSpendBundle([]*CoinSpend{cs}, sig)
	defer sb.Free()
	data, _ := sb.Bytes()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		sb2, _ := NewSpendBundleFromBytes(data)
		sb2.Free()
	}
}

// ── Offer Encoding ──────────────────────────────────────────────────────

func BenchmarkOfferEncode(b *testing.B) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()
	cs, _ := NewCoinSpend(coin, []byte{0x01}, []byte{0x80})
	defer cs.Free()
	sig, _ := NewSignatureInfinity()
	defer sig.Free()
	sb, _ := NewSpendBundle([]*CoinSpend{cs}, sig)
	defer sb.Free()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		EncodeOffer(sb)
	}
}

func BenchmarkOfferDecode(b *testing.B) {
	coin, _ := NewCoin(make([]byte, 32), make([]byte, 32), 100)
	defer coin.Free()
	cs, _ := NewCoinSpend(coin, []byte{0x01}, []byte{0x80})
	defer cs.Free()
	sig, _ := NewSignatureInfinity()
	defer sig.Free()
	sb, _ := NewSpendBundle([]*CoinSpend{cs}, sig)
	defer sb.Free()
	encoded, _ := EncodeOffer(sb)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		sb2, _ := DecodeOffer(encoded)
		sb2.Free()
	}
}

// ── End-to-end: XCH spend (low-level API) ───────────────────────────────

func BenchmarkXchSpendAndConfirm(b *testing.B) {
	sim, _ := NewSimulator()
	defer sim.Free()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		pair, _ := sim.Bls(1_000_000)
		sk, _ := pair.Sk()
		pk, _ := pair.Pk()
		coin, _ := pair.Coin()
		puzzleHash, _ := pair.PuzzleHash()
		b.StartTimer()

		clvm, _ := NewClvm()
		cc, _ := clvm.CreateCoin(puzzleHash, 999_900, nil)
		fee, _ := clvm.ReserveFee(100)
		delegated, _ := clvm.DelegatedSpend([]*Program{cc, fee})
		spend, _ := clvm.StandardSpend(pk, delegated)
		clvm.SpendCoin(coin, spend)
		coinSpends, _ := clvm.CoinSpends()
		sim.SpendCoins(coinSpends, []*SecretKey{sk})

		for _, cs := range coinSpends {
			cs.Free()
		}
		spend.Free()
		delegated.Free()
		fee.Free()
		cc.Free()
		clvm.Free()
		coin.Free()
		pk.Free()
		sk.Free()
		pair.Free()
	}
}

// ── End-to-end: multi-coin consolidation (low-level API) ────────────────

func BenchmarkMultiCoinConsolidation(b *testing.B) {
	sim, _ := NewSimulator()
	defer sim.Free()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		pairs := make([]*BlsPairWithCoin, 5)
		sks := make([]*SecretKey, 5)
		for j := 0; j < 5; j++ {
			pairs[j], _ = sim.Bls(200_000)
			sks[j], _ = pairs[j].Sk()
		}
		b.StartTimer()

		destPh, _ := pairs[0].PuzzleHash()
		clvm, _ := NewClvm()

		for j := 0; j < 5; j++ {
			pk, _ := pairs[j].Pk()
			coin, _ := pairs[j].Coin()

			var conditions []*Program
			if j == 0 {
				cc, _ := clvm.CreateCoin(destPh, 999_900, nil)
				fee, _ := clvm.ReserveFee(100)
				conditions = []*Program{cc, fee}
			} else {
				coinId, _ := pairs[0].Coin()
				cid, _ := coinId.CoinId()
				assert, _ := clvm.AssertConcurrentSpend(cid)
				conditions = []*Program{assert}
				coinId.Free()
			}
			delegated, _ := clvm.DelegatedSpend(conditions)
			spend, _ := clvm.StandardSpend(pk, delegated)
			clvm.SpendCoin(coin, spend)

			for _, c := range conditions {
				c.Free()
			}
			spend.Free()
			delegated.Free()
			pk.Free()
			coin.Free()
		}

		coinSpends, _ := clvm.CoinSpends()
		sim.SpendCoins(coinSpends, sks)

		for _, cs := range coinSpends {
			cs.Free()
		}
		clvm.Free()
		for j := 0; j < 5; j++ {
			sks[j].Free()
			pairs[j].Free()
		}
	}
}

// ── Scaled workloads (Action/Spends API) ────────────────────────────────
// These benchmarks use the recommended high-level API and test scaling.

func benchmarkBatchXchSend(b *testing.B, n int) {
	sim, _ := NewSimulator()
	defer sim.Free()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		totalAmount := uint64(n*100_000 + 100)
		pair, _ := sim.Bls(totalAmount)
		sk, _ := pair.Sk()
		coin, _ := pair.Coin()
		puzzleHash, _ := pair.PuzzleHash()
		b.StartTimer()

		clvm, _ := NewClvm()
		spends, _ := NewSpends(clvm, puzzleHash)
		spends.AddXch(coin)

		actions := make([]*Action, 0, n+1)
		xchId, _ := NewIdXch()
		for j := 0; j < n; j++ {
			destPh := make([]byte, 32)
			destPh[0] = byte(j)
			destPh[1] = byte(j >> 8)
			send, _ := NewActionSend(xchId, destPh, 100_000, nil)
			actions = append(actions, send)
		}
		fee, _ := NewActionFee(100)
		actions = append(actions, fee)

		deltas, _ := spends.Apply(actions)
		finished, _ := spends.Prepare(deltas)
		outputs, _ := finished.Spend()
		coinSpends, _ := clvm.CoinSpends()
		sim.SpendCoins(coinSpends, []*SecretKey{sk})

		for _, cs := range coinSpends {
			cs.Free()
		}
		outputs.Free()
		finished.Free()
		deltas.Free()
		for _, a := range actions {
			a.Free()
		}
		xchId.Free()
		spends.Free()
		clvm.Free()
		coin.Free()
		sk.Free()
		pair.Free()
	}
}

func BenchmarkBatchXchSend_1(b *testing.B)   { benchmarkBatchXchSend(b, 1) }
func BenchmarkBatchXchSend_10(b *testing.B)  { benchmarkBatchXchSend(b, 10) }
func BenchmarkBatchXchSend_100(b *testing.B) { benchmarkBatchXchSend(b, 100) }

func benchmarkBatchCatIssuance(b *testing.B, n int) {
	sim, _ := NewSimulator()
	defer sim.Free()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		totalAmount := uint64(n*1000 + 100)
		pair, _ := sim.Bls(totalAmount)
		sk, _ := pair.Sk()
		coin, _ := pair.Coin()
		puzzleHash, _ := pair.PuzzleHash()
		b.StartTimer()

		clvm, _ := NewClvm()
		spends, _ := NewSpends(clvm, puzzleHash)
		spends.AddXch(coin)

		actions := make([]*Action, 0, n+1)
		for j := 0; j < n; j++ {
			issueCat, _ := NewActionSingleIssueCat(puzzleHash, 1000)
			actions = append(actions, issueCat)
		}
		fee, _ := NewActionFee(100)
		actions = append(actions, fee)

		deltas, _ := spends.Apply(actions)
		finished, _ := spends.Prepare(deltas)
		outputs, _ := finished.Spend()
		coinSpends, _ := clvm.CoinSpends()
		sim.SpendCoins(coinSpends, []*SecretKey{sk})

		for _, cs := range coinSpends {
			cs.Free()
		}
		outputs.Free()
		finished.Free()
		deltas.Free()
		for _, a := range actions {
			a.Free()
		}
		spends.Free()
		clvm.Free()
		coin.Free()
		sk.Free()
		pair.Free()
	}
}

func BenchmarkBatchCatIssuance_1(b *testing.B)  { benchmarkBatchCatIssuance(b, 1) }
func BenchmarkBatchCatIssuance_10(b *testing.B) { benchmarkBatchCatIssuance(b, 10) }
func BenchmarkBatchCatIssuance_25(b *testing.B) { benchmarkBatchCatIssuance(b, 25) }

func benchmarkBatchNftMint(b *testing.B, n int) {
	sim, _ := NewSimulator()
	defer sim.Free()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		totalAmount := uint64(n + 100)
		pair, _ := sim.Bls(totalAmount)
		sk, _ := pair.Sk()
		coin, _ := pair.Coin()
		puzzleHash, _ := pair.PuzzleHash()
		b.StartTimer()

		clvm, _ := NewClvm()
		spends, _ := NewSpends(clvm, puzzleHash)
		spends.AddXch(coin)

		updater, _ := clvm.NftMetadataUpdaterDefault()
		updaterHash, _ := updater.TreeHash()

		actions := make([]*Action, 0, n+1)
		for j := 0; j < n; j++ {
			metadata, _ := NewNftMetadata(
				uint64(j+1), uint64(n),
				[]string{"https://example.com/nft.png"},
				nil, nil, nil, nil, nil,
			)
			metadataProgram, _ := clvm.NftMetadata(metadata)
			mint, _ := NewActionMintNft(clvm, metadataProgram, updaterHash, puzzleHash, 0, 1, nil)
			actions = append(actions, mint)
			metadataProgram.Free()
			metadata.Free()
		}
		fee, _ := NewActionFee(100)
		actions = append(actions, fee)

		deltas, _ := spends.Apply(actions)
		finished, _ := spends.Prepare(deltas)
		outputs, _ := finished.Spend()
		coinSpends, _ := clvm.CoinSpends()
		sim.SpendCoins(coinSpends, []*SecretKey{sk})

		for _, cs := range coinSpends {
			cs.Free()
		}
		outputs.Free()
		finished.Free()
		deltas.Free()
		for _, a := range actions {
			a.Free()
		}
		updater.Free()
		spends.Free()
		clvm.Free()
		coin.Free()
		sk.Free()
		pair.Free()
	}
}

func BenchmarkBatchNftMint_1(b *testing.B)  { benchmarkBatchNftMint(b, 1) }
func BenchmarkBatchNftMint_10(b *testing.B) { benchmarkBatchNftMint(b, 10) }
func BenchmarkBatchNftMint_25(b *testing.B) { benchmarkBatchNftMint(b, 25) }
