// wasm/__test__/silent_payments.spec.ts
//
// wasm raw-key SP send + scan-from-tweaks E2E, plus the raw-key contract.
//
// Mirrors napi/__test__/silent_payments_e2e.spec.ts
// structurally; the only differences are imports from `../pkg` and the
// setPanicHook() call at module load per wasm-pack convention (matches
// wasm/__test__/wasm.spec.ts:14 precedent).
//
// TweakData is constructed on the Rust side (via Simulator.tweakDataFromBlock)
// and crossed the FFI boundary unchanged. This is the first runtime test of
// `Vec<chia_bls::PublicKey>` marshaling on TweakData.tweakPoints across the
// wasm-bindgen FFI.
//
// withSilentPaymentKeys accepts RAW
// PublicKey/SecretKey and synthesizes the synthetic key internally via
// deriveSynthetic (default hidden puzzle). The happy path builds the sender
// coin at the SYNTHETIC puzzle hash so a raw-key registration round-trips; the
// negative test proves a wrong key surfaces the typed
// SilentPaymentKeyNotSynthetic error across the wasm FFI (runtime guard backstop).
//
// Cross-language coverage is scoped to the unlabeled flow; the labeled
// detection branch is exercised by the Rust-side E2E tests in
// crates/chia-sdk-driver/tests/silent_payments_e2e.rs (test_simulator_e2e_labeled)
// against the CHIP-0057 test vectors.

import test from "ava";
import {
  Action,
  Clvm,
  LabelRegistry,
  Mnemonic,
  setPanicHook,
  SilentPaymentKeys,
  SilentPaymentNetwork,
  SilentPaymentRegisteredKey,
  SilentPaymentRegisteredSecretKey,
  SilentPayments,
  Simulator,
  Spends,
  standardPuzzleHash,
} from "../pkg";

setPanicHook();

const TV1_MNEMONIC =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const K_MAX_DEFAULT = 2400;

test("wasm: raw-key SP send + scan-from-tweaks E2E", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  // Recipient: deterministic mnemonic so the test is reproducible.
  const recipient = SilentPaymentKeys.fromMnemonic(new Mnemonic(TV1_MNEMONIC));
  const recipientAddress = recipient.unlabeledAddress(
    SilentPaymentNetwork.Testnet,
  );

  // Sender: fresh BLS pair from simulator (used only for its key pair).
  const sender = sim.bls(1_000n);

  // withSilentPaymentKeys synthesizes the registered RAW key via
  // deriveSynthetic internally, so the sender coin must live at the SYNTHETIC
  // puzzle hash for the raw-key registration to round-trip through the runtime guard.
  const senderSyntheticPk = sender.pk.deriveSynthetic();
  const senderSyntheticSk = sender.sk.deriveSynthetic();
  const senderPh = standardPuzzleHash(senderSyntheticPk);
  const senderCoin = sim.newCoin(senderPh, 1_000n);
  const heightBefore = sim.height();

  // Build the SP send via the dedicated Action.silentPaymentSend +
  // withSilentPaymentKeys path.
  const spends = new Spends(clvm, senderPh);
  spends.addXch(senderCoin);

  const actions = [
    Action.silentPaymentSend(recipientAddress, 100n, undefined),
  ];

  // Register RAW SP keys (wrapper-class form — bindy doesn't
  // marshal Vec<(K,V)> directly across wasm-bindgen either). The facade
  // synthesizes deriveSynthetic(sender.pk/sk) internally.
  spends.withSilentPaymentKeys(
    [new SilentPaymentRegisteredKey(senderPh, sender.pk)],
    [new SilentPaymentRegisteredSecretKey(senderPh, sender.sk)],
  );

  const deltas = spends.apply(actions);
  const finished = spends.prepare(deltas);

  // Standard-puzzle-spend the sender's XCH input. The coin is curried over the
  // SYNTHETIC key, so spend + sign with the synthetic key pair.
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
  // crosses the wasm FFI boundary in any test. Each tweak point must be
  // a valid PublicKey we can serialize back to bytes.
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
  // wasm bindy emits `Option<u32>` as `number | undefined` (not null).
  t.is(detections[0].label, undefined, "unlabeled detection → label is undefined");
  t.is(detections[0].amount, 100n, "amount round-trips");

  // derive_synthetic + standard-puzzle-spend the
  // detected coin from TypeScript-on-wasm.
  const onetimeSk = detections[0].onetimeSk;
  const syntheticSk = onetimeSk.deriveSynthetic();
  const syntheticPk = syntheticSk.publicKey();

  const detectedCoinId = detections[0].coinId;
  const detectedAmount = detections[0].amount;
  const detectedCoinState = sim.coinState(detectedCoinId);
  t.not(detectedCoinState, undefined, "detected coin is in simulator state");
  t.is(
    detectedCoinState?.spentHeight,
    undefined,
    "detected coin is unspent before follow-on spend",
  );

  // Build conditions to spend the detected coin back to the sender, with a
  // 1-mojo fee. Use a fresh Clvm allocator for the follow-on spend.
  const followClvm = new Clvm();
  const conditions = [
    followClvm.createCoin(sender.puzzleHash, detectedAmount - 1n, undefined),
    followClvm.reserveFee(1n),
  ];
  const delegatedSpend = followClvm.delegatedSpend(conditions);
  const standardSpend = followClvm.standardSpend(syntheticPk, delegatedSpend);
  followClvm.spendCoin(detectedCoinState!.coin, standardSpend);

  sim.spendCoins(followClvm.coinSpends(), [syntheticSk]);

  const afterSpend = sim.coinState(detectedCoinId);
  t.not(
    afterSpend?.spentHeight,
    undefined,
    "detected SP coin successfully spent",
  );
});

test("wasm: raw key against a non-synthetic coin surfaces SilentPaymentKeyNotSynthetic", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const recipient = SilentPaymentKeys.fromMnemonic(new Mnemonic(TV1_MNEMONIC));
  const recipientAddress = recipient.unlabeledAddress(
    SilentPaymentNetwork.Testnet,
  );

  // Sender coin is curried over the RAW pk (StandardArgs::curry_tree_hash(pk)),
  // i.e. sim.bls() treats sender.pk AS the curried key. Registering RAW
  // sender.pk makes the facade synthesize deriveSynthetic(sender.pk), whose
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
  // error crosses the wasm FFI boundary as a thrown error, and NO spend bundle
  // is produced.
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
