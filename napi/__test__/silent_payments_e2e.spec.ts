// napi/__test__/silent_payments_e2e.spec.ts
//
// napi unlabeled SP send + scan-from-tweaks E2E, plus the raw-key contract.
//
// Exercises full FFI fidelity: TweakData is constructed on the
// Rust side and crossed the FFI boundary unchanged. This is the first runtime
// test of Vec<chia_bls::PublicKey> marshaling on TweakData.tweakPoints across
// napi.
//
// withSilentPaymentKeys accepts RAW
// PublicKey/SecretKey and synthesizes the synthetic key internally via
// derive_synthetic (default hidden puzzle). The happy path below builds the
// sender coin at the SYNTHETIC puzzle hash so a raw-key registration
// round-trips; the negative test proves a wrong (raw-against-a-raw-curried-coin)
// key surfaces the typed SilentPaymentKeyNotSynthetic error across the FFI
// boundary (runtime guard backstop).
//
// Cross-language coverage is scoped to the unlabeled flow; the labeled
// detection branch is exercised by the Rust-side E2E tests in
// crates/chia-sdk-driver/tests/silent_payments_e2e.rs (test_simulator_e2e_labeled).

import test from "ava";
import {
  Action,
  Clvm,
  LabelRegistry,
  Mnemonic,
  SilentPaymentKeys,
  SilentPaymentNetwork,
  SilentPaymentRegisteredKey,
  SilentPaymentRegisteredSecretKey,
  SilentPayments,
  Simulator,
  Spends,
  standardPuzzleHash,
} from "..";

const TV1_MNEMONIC =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const K_MAX_DEFAULT = 2400;

test("napi: raw-key SP send + scan-from-tweaks E2E", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  // Recipient: deterministic mnemonic so the test is reproducible
  // (matches the silent_payments.spec.ts fixture).
  const recipient = SilentPaymentKeys.fromMnemonic(new Mnemonic(TV1_MNEMONIC));
  const recipientAddress = recipient.unlabeledAddress(
    SilentPaymentNetwork.Testnet,
  );

  // Sender: fresh BLS pair from simulator (used only for its key pair).
  const sender = sim.bls(1_000n);
  const heightBefore = sim.height();

  // withSilentPaymentKeys synthesizes the registered RAW key via
  // derive_synthetic internally. For the raw-key registration to round-trip
  // through the runtime guard, the sender coin must live at the SYNTHETIC puzzle hash
  // (curry_tree_hash(derive_synthetic(sender.pk))), NOT the raw puzzle hash.
  const senderSyntheticPk = sender.pk.deriveSynthetic();
  const senderSyntheticSk = sender.sk.deriveSynthetic();
  const senderPh = standardPuzzleHash(senderSyntheticPk);
  const senderCoin = sim.newCoin(senderPh, 1_000n);

  // Build the SP send via the dedicated Action.silentPaymentSend +
  // withSilentPaymentKeys path.
  const spends = new Spends(clvm, senderPh);
  spends.addXch(senderCoin);

  const actions = [
    Action.silentPaymentSend(recipientAddress, 100n, undefined),
  ];

  // Register RAW SP keys BEFORE apply (with_silent_payment_keys must precede
  // finish-time SP processing inside prepare()). Wrapper-class form per
  // bindy doesn't marshal Vec<(K,V)> directly. The facade
  // synthesizes derive_synthetic(sender.pk/sk) internally.
  spends.withSilentPaymentKeys(
    [new SilentPaymentRegisteredKey(senderPh, sender.pk)],
    [new SilentPaymentRegisteredSecretKey(senderPh, sender.sk)],
  );

  const deltas = spends.apply(actions);
  const finished = spends.prepare(deltas);

  // Standard-puzzle-spend the sender's XCH input. The coin is curried over the
  // SYNTHETIC key, so spend with the synthetic key and sign with the synthetic
  // sk (mirrors the follow-on detected-coin spend below).
  for (const pending of finished.pendingSpends()) {
    finished.insert(
      pending.coin().coinId(),
      clvm.standardSpend(
        senderSyntheticPk,
        clvm.delegatedSpend(pending.conditions()),
      ),
    );
  }
  finished.spend();

  // Farm the block.
  sim.spendCoins(clvm.coinSpends(), [senderSyntheticSk]);

  // Extract TweakData via the bindings helper.
  // THIS IS THE NEW FFI SURFACE.
  const tweakData = sim.tweakDataFromBlock(heightBefore);
  t.is(
    tweakData.tweakPoints.length,
    1,
    "one SP transaction → one tweak_point",
  );
  t.true(
    tweakData.outputs.length >= 1,
    "at least the recipient's output is in OutputMeta list",
  );
  // Vec<PublicKey> runtime marshaling proof — the first time this Vec
  // crosses the FFI boundary in any test. Each tweak point must be a
  // valid PublicKey we can serialize back to bytes.
  for (const tp of tweakData.tweakPoints) {
    t.is(tp.toBytes().length, 48, "each tweak_point round-trips as 48 bytes");
  }

  // Scan — Vec<PublicKey> marshaling check fires here on
  // tweakData.tweakPoints access inside the scanner.
  const labels = new LabelRegistry();
  const detections = SilentPayments.scanFromTweaks(
    recipient.scanSk(),
    recipient.spendSk(),
    recipient.spendPk(),
    tweakData,
    labels,
    K_MAX_DEFAULT,
  );

  t.is(detections.length, 1, "scanner finds exactly one SP output");
  t.is(detections[0].k, 0, "first output at this scan_pk → k=0");
  t.is(detections[0].label, null, "unlabeled detection → label is null");
  t.is(detections[0].amount, 100n, "amount round-trips");

  // derive_synthetic + standard-puzzle-spend the
  // detected coin from TypeScript. This is the strongest reading of
  // "Vec<PublicKey> marshals correctly" — the cross-language client
  // not only reads tweak_points but can complete the full
  // send → farm → extract → scan → SPEND round-trip.
  const onetimeSk = detections[0].onetimeSk;
  const syntheticSk = onetimeSk.deriveSynthetic();
  const syntheticPk = syntheticSk.publicKey();

  const detectedCoinId = detections[0].coinId;
  const detectedAmount = detections[0].amount;
  const detectedCoinState = sim.coinState(detectedCoinId);
  t.not(detectedCoinState, null, "detected coin is in simulator state");
  t.is(
    detectedCoinState?.spentHeight,
    null,
    "detected coin is unspent before follow-on spend",
  );

  // Build conditions to spend the detected coin back to the sender, with a
  // 1-mojo fee. Reuse the same Clvm allocator (action_system.spec.ts pattern).
  const followClvm = new Clvm();
  const conditions = [
    followClvm.createCoin(senderPh, detectedAmount - 1n, null),
    followClvm.reserveFee(1n),
  ];
  const delegatedSpend = followClvm.delegatedSpend(conditions);
  const standardSpend = followClvm.standardSpend(syntheticPk, delegatedSpend);
  const detectedCoin = detectedCoinState!.coin;
  followClvm.spendCoin(detectedCoin, standardSpend);

  sim.spendCoins(followClvm.coinSpends(), [syntheticSk]);

  const afterSpend = sim.coinState(detectedCoinId);
  t.not(afterSpend?.spentHeight, null, "detected SP coin successfully spent");
});

test("napi: raw key against a non-synthetic coin surfaces SilentPaymentKeyNotSynthetic", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const recipient = SilentPaymentKeys.fromMnemonic(new Mnemonic(TV1_MNEMONIC));
  const recipientAddress = recipient.unlabeledAddress(
    SilentPaymentNetwork.Testnet,
  );

  // Sender coin is curried over the RAW pk (StandardArgs::curry_tree_hash(pk)),
  // i.e. sim.bls() treats sender.pk AS the curried key. Registering RAW
  // sender.pk makes the facade synthesize derive_synthetic(sender.pk), whose
  // curry_tree_hash != sender.puzzleHash — so the runtime guard must fire inside
  // prepare() before any spend bundle is produced.
  const sender = sim.bls(1_000n);

  const spends = new Spends(clvm, sender.puzzleHash);
  spends.addXch(sender.coin);

  const actions = [
    Action.silentPaymentSend(recipientAddress, 100n, undefined),
  ];

  spends.withSilentPaymentKeys(
    [new SilentPaymentRegisteredKey(sender.puzzleHash, sender.pk)],
    [new SilentPaymentRegisteredSecretKey(sender.puzzleHash, sender.sk)],
  );

  const deltas = spends.apply(actions);

  // the runtime guard fires inside prepare() — the typed SilentPaymentKeyNotSynthetic
  // error crosses the FFI boundary as a thrown error, and NO spend bundle is
  // produced.
  t.throws(
    () => {
      spends.prepare(deltas);
    },
    { message: /key not synthetic/i },
    "raw key against a raw-curried coin must surface the typed error",
  );

  // No spend bundle was produced.
  t.is(clvm.coinSpends().length, 0, "no coin spends produced on the failed path");
});
