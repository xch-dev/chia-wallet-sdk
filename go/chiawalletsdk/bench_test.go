package chiawalletsdk_test

import (
	"crypto/sha256"
	"fmt"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// Ported from the benchmarks for the Go bindings in #370. They measure the cost of calls through
// the bindings, including converting arguments and results.

func BenchmarkSecretKeyFromSeed(b *testing.B) {
	value := seed(0)
	for i := 0; i < b.N; i++ {
		must(sdk.SecretKeyFromSeed(value)).Destroy()
	}
}

func BenchmarkSecretKeyToPublicKey(b *testing.B) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sk.PublicKey()).Destroy()
	}
}

func BenchmarkBlsSign(b *testing.B) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	message := []byte("benchmark message")

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sk.Sign(message)).Destroy()
	}
}

func BenchmarkBlsVerify(b *testing.B) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	message := []byte("benchmark message")
	sig := must(sk.Sign(message))
	defer sig.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if !must(pk.Verify(message, sig)) {
			b.Fatal("expected the signature to verify")
		}
	}
}

func BenchmarkBlsSignatureAggregate2(b *testing.B) {
	sk1 := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk1.Destroy()

	sk2 := must(sdk.SecretKeyFromSeed(seed(1)))
	defer sk2.Destroy()

	signatures := []*sdk.Signature{must(sk1.Sign([]byte("msg1"))), must(sk2.Sign([]byte("msg2")))}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.SignatureAggregate(signatures)).Destroy()
	}
}

func BenchmarkBlsPublicKeyAggregate2(b *testing.B) {
	sk1 := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk1.Destroy()

	sk2 := must(sdk.SecretKeyFromSeed(seed(1)))
	defer sk2.Destroy()

	publicKeys := []*sdk.PublicKey{must(sk1.PublicKey()), must(sk2.PublicKey())}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.PublicKeyAggregate(publicKeys)).Destroy()
	}
}

func BenchmarkDeriveHardened(b *testing.B) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sk.DeriveHardened(uint32(i))).Destroy()
	}
}

func BenchmarkDeriveUnhardened(b *testing.B) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sk.DeriveUnhardened(uint32(i))).Destroy()
	}
}

func BenchmarkDeriveSynthetic(b *testing.B) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sk.DeriveSynthetic()).Destroy()
	}
}

func BenchmarkK1Sign(b *testing.B) {
	skBytes := sha256.Sum256([]byte("bench k1"))
	sk := must(sdk.K1SecretKeyFromBytes(skBytes[:]))
	defer sk.Destroy()

	hash := sha256.Sum256([]byte("bench msg"))

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sk.SignPrehashed(hash[:])).Destroy()
	}
}

func BenchmarkK1Verify(b *testing.B) {
	skBytes := sha256.Sum256([]byte("bench k1"))
	sk := must(sdk.K1SecretKeyFromBytes(skBytes[:]))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	hash := sha256.Sum256([]byte("bench msg"))
	sig := must(sk.SignPrehashed(hash[:]))
	defer sig.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if !must(pk.VerifyPrehashed(hash[:], sig)) {
			b.Fatal("expected the signature to verify")
		}
	}
}

func BenchmarkMnemonicGenerate24(b *testing.B) {
	for i := 0; i < b.N; i++ {
		must(sdk.MnemonicGenerate(true)).Destroy()
	}
}

func BenchmarkMnemonicToSeed(b *testing.B) {
	mnemonic := must(sdk.MnemonicGenerate(true))
	defer mnemonic.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(mnemonic.ToSeed(""))
	}
}

func BenchmarkMnemonicVerify(b *testing.B) {
	mnemonic := must(sdk.MnemonicGenerate(true))
	defer mnemonic.Destroy()

	phrase := must(mnemonic.ToString())

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.MnemonicVerify(phrase))
	}
}

func BenchmarkAddressEncode(b *testing.B) {
	address := must(sdk.NewAddress(make([]byte, 32), "xch"))
	defer address.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(address.Encode())
	}
}

func BenchmarkAddressDecode(b *testing.B) {
	address := must(sdk.NewAddress(make([]byte, 32), "xch"))
	defer address.Destroy()

	encoded := must(address.Encode())

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.AddressDecode(encoded)).Destroy()
	}
}

func BenchmarkCoinCreate(b *testing.B) {
	parent := make([]byte, 32)
	puzzleHash := make([]byte, 32)

	for i := 0; i < b.N; i++ {
		must(sdk.NewCoin(parent, puzzleHash, 1000)).Destroy()
	}
}

func BenchmarkCoinId(b *testing.B) {
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1000))
	defer coin.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(coin.CoinId())
	}
}

func BenchmarkClvmPair(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	nilProgram := must(clvm.Nil())
	defer nilProgram.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(clvm.Pair(nilProgram, nilProgram)).Destroy()
	}
}

func BenchmarkProgramSerialize(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	pair := must(clvm.Pair(must(clvm.Nil()), must(clvm.Nil())))
	defer pair.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(pair.Serialize())
	}
}

func BenchmarkProgramDeserialize(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	data := []byte{0xff, 0x80, 0x80}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(clvm.Deserialize(data)).Destroy()
	}
}

func BenchmarkProgramTreeHash(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	nilProgram := must(clvm.Nil())
	defer nilProgram.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(nilProgram.TreeHash())
	}
}

func BenchmarkClvmCreateCoin(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	puzzleHash := make([]byte, 32)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(clvm.CreateCoin(puzzleHash, 1000, nil)).Destroy()
	}
}

func BenchmarkDelegatedSpend(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	conditions := []*sdk.Program{must(clvm.CreateCoin(make([]byte, 32), 1000, nil))}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(clvm.DelegatedSpend(conditions)).Destroy()
	}
}

func BenchmarkStandardSpend(b *testing.B) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	defer sk.Destroy()

	pk := must(sk.PublicKey())
	defer pk.Destroy()

	delegated := must(clvm.DelegatedSpend([]*sdk.Program{must(clvm.CreateCoin(make([]byte, 32), 1000, nil))}))
	defer delegated.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(clvm.StandardSpend(pk, delegated)).Destroy()
	}
}

// newSpendBundle returns a spend bundle with a single coin spend.
func newSpendBundle() *sdk.SpendBundle {
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 100))
	coinSpend := must(sdk.NewCoinSpend(coin, []byte{0x01}, []byte{0x80}))
	return must(sdk.NewSpendBundle([]*sdk.CoinSpend{coinSpend}, must(sdk.SignatureInfinity())))
}

func BenchmarkSpendBundleSerialize(b *testing.B) {
	bundle := newSpendBundle()
	defer bundle.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(bundle.ToBytes())
	}
}

func BenchmarkSpendBundleDeserialize(b *testing.B) {
	bundle := newSpendBundle()
	defer bundle.Destroy()

	data := must(bundle.ToBytes())

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.SpendBundleFromBytes(data)).Destroy()
	}
}

func BenchmarkOfferEncode(b *testing.B) {
	bundle := newSpendBundle()
	defer bundle.Destroy()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.EncodeOffer(bundle))
	}
}

func BenchmarkOfferDecode(b *testing.B) {
	bundle := newSpendBundle()
	defer bundle.Destroy()

	offer := must(sdk.EncodeOffer(bundle))

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		must(sdk.DecodeOffer(offer)).Destroy()
	}
}

func BenchmarkXchSpendAndConfirm(b *testing.B) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		pair := must(sim.Bls(1_000_000))
		b.StartTimer()

		clvm := must(sdk.NewClvm())
		delegated := must(clvm.DelegatedSpend([]*sdk.Program{
			must(clvm.CreateCoin(pair.PuzzleHash(), 999_900, nil)),
			must(clvm.ReserveFee(100)),
		}))

		if err := clvm.SpendStandardCoin(pair.Coin(), pair.Pk(), delegated); err != nil {
			b.Fatal(err)
		}

		if err := sim.SpendCoins(must(clvm.CoinSpends()), []*sdk.SecretKey{pair.Sk()}); err != nil {
			b.Fatal(err)
		}

		clvm.Destroy()
		pair.Destroy()
	}
}

func BenchmarkMultiCoinConsolidation(b *testing.B) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		pairs := make([]*sdk.BlsPairWithCoin, 5)
		secretKeys := make([]*sdk.SecretKey, 5)
		for j := range pairs {
			pairs[j] = must(sim.Bls(200_000))
			secretKeys[j] = pairs[j].Sk()
		}
		b.StartTimer()

		clvm := must(sdk.NewClvm())
		firstCoinID := must(pairs[0].Coin().CoinId())

		for j, pair := range pairs {
			var conditions []*sdk.Program
			if j == 0 {
				conditions = []*sdk.Program{
					must(clvm.CreateCoin(pair.PuzzleHash(), 999_900, nil)),
					must(clvm.ReserveFee(100)),
				}
			} else {
				conditions = []*sdk.Program{must(clvm.AssertConcurrentSpend(firstCoinID))}
			}

			delegated := must(clvm.DelegatedSpend(conditions))
			if err := clvm.SpendStandardCoin(pair.Coin(), pair.Pk(), delegated); err != nil {
				b.Fatal(err)
			}
		}

		if err := sim.SpendCoins(must(clvm.CoinSpends()), secretKeys); err != nil {
			b.Fatal(err)
		}

		clvm.Destroy()
		for _, pair := range pairs {
			pair.Destroy()
		}
	}
}

// benchmarkActions measures building, signing and confirming the actions returned by build, which
// is given a fresh pair whose coin has the given amount.
func benchmarkActions(b *testing.B, amount uint64, build func(clvm *sdk.Clvm, pair *sdk.BlsPairWithCoin) []*sdk.Action) {
	sim := must(sdk.NewSimulator())
	defer sim.Destroy()

	for i := 0; i < b.N; i++ {
		b.StopTimer()
		pair := must(sim.Bls(amount))
		b.StartTimer()

		clvm := must(sdk.NewClvm())
		spendActions(b, sim, clvm, pair, build(clvm, pair)).Destroy()

		clvm.Destroy()
		pair.Destroy()
	}
}

func BenchmarkBatchXchSend(b *testing.B) {
	for _, n := range []int{1, 10, 100} {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			benchmarkActions(b, uint64(n)*100_000+100, func(_ *sdk.Clvm, _ *sdk.BlsPairWithCoin) []*sdk.Action {
				xch := must(sdk.IdXch())
				actions := []*sdk.Action{must(sdk.ActionFee(100))}

				for j := 0; j < n; j++ {
					destination := []byte{byte(j), byte(j >> 8)}
					destination = append(destination, make([]byte, 30)...)
					actions = append(actions, must(sdk.ActionSend(xch, destination, 100_000, nil)))
				}

				return actions
			})
		})
	}
}

func BenchmarkBatchCatIssuance(b *testing.B) {
	for _, n := range []int{1, 10, 25} {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			// Each CAT has a different amount, since identical issuances from the same coin would
			// create the same eve coin
			total := uint64(n*1000+n*(n-1)/2) + 100
			benchmarkActions(b, total, func(_ *sdk.Clvm, pair *sdk.BlsPairWithCoin) []*sdk.Action {
				hiddenPuzzleHash := pair.PuzzleHash()
				actions := []*sdk.Action{must(sdk.ActionFee(100))}

				for j := 0; j < n; j++ {
					actions = append(actions, must(sdk.ActionSingleIssueCat(&hiddenPuzzleHash, uint64(1000+j))))
				}

				return actions
			})
		})
	}
}

func BenchmarkBatchNftMint(b *testing.B) {
	for _, n := range []int{1, 10, 25} {
		b.Run(fmt.Sprint(n), func(b *testing.B) {
			benchmarkActions(b, uint64(n)+100, func(clvm *sdk.Clvm, pair *sdk.BlsPairWithCoin) []*sdk.Action {
				updaterHash := must(must(clvm.NftMetadataUpdaterDefault()).TreeHash())
				actions := []*sdk.Action{must(sdk.ActionFee(100))}

				for j := 0; j < n; j++ {
					metadata := must(sdk.NewNftMetadata(uint64(j+1), uint64(n), []string{"https://example.com/nft.png"}, nil, nil, nil, nil, nil))
					program := must(clvm.NftMetadata(metadata))
					actions = append(actions, must(sdk.ActionMintNft(clvm, program, updaterHash, pair.PuzzleHash(), 0, 1, nil)))
					metadata.Destroy()
				}

				return actions
			})
		})
	}
}
