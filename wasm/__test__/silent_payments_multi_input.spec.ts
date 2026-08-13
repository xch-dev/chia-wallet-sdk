// wasm/__test__/silent_payments_multi_input.spec.ts
//
// wasm multi-input SP send + scan-from-tweaks E2E.
//
// Mirrors
// napi/__test__/silent_payments_multi_input.spec.ts structurally; the only
// differences are imports from `../pkg` and the setPanicHook() call at module
// load per wasm-pack convention.
//
// TweakData is constructed by SilentPayments.tweakDataFromBlockSpends
// over sim.blockSpends(h) + sim.blockOutputs(h) (the
// facade additions), driven by a 2-coin SP send whose driver-side
// gate is satisfied by spends.prepare(deltas, Relation.assertConcurrent())
// (the extended signature + opaque-handle binding).

import test from "ava";
import {
  Action,
  Clvm,
  LabelRegistry,
  Mnemonic,
  Relation,
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

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

test("wasm: multi-input SP send -> tweak_data_from_block_spends -> scan", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const recipient = SilentPaymentKeys.fromMnemonic(new Mnemonic(TV1_MNEMONIC));
  const recipientAddress = recipient.unlabeledAddress(
    SilentPaymentNetwork.Testnet,
  );

  // Two non-ephemeral XCH coins with different BLS pairs. The Relation
  // cycle binding ties them together so the receiver scanner can re-group
  // them via Pass 2b SCC over opcode-64 AssertConcurrentSpend edges.
  //
  // withSilentPaymentKeys synthesizes the registered RAW key via
  // deriveSynthetic internally, so each coin must live at its SYNTHETIC puzzle
  // hash for the raw-key registration to round-trip through the runtime guard.
  const sender1 = sim.bls(500n);
  const sender2 = sim.bls(500n);
  const sender1SyntheticPk = sender1.pk.deriveSynthetic();
  const sender2SyntheticPk = sender2.pk.deriveSynthetic();
  const sender1SyntheticSk = sender1.sk.deriveSynthetic();
  const sender2SyntheticSk = sender2.sk.deriveSynthetic();
  const sender1Ph = standardPuzzleHash(sender1SyntheticPk);
  const sender2Ph = standardPuzzleHash(sender2SyntheticPk);
  const sender1Coin = sim.newCoin(sender1Ph, 500n);
  const sender2Coin = sim.newCoin(sender2Ph, 500n);
  const heightBefore = sim.height();

  const spends = new Spends(clvm, sender1Ph);
  spends.addXch(sender1Coin);
  spends.addXch(sender2Coin);

  const actions = [
    Action.silentPaymentSend(recipientAddress, 700n, undefined),
  ];

  // Register RAW keys — the facade synthesizes deriveSynthetic(...) internally.
  spends.withSilentPaymentKeys(
    [
      new SilentPaymentRegisteredKey(sender1Ph, sender1.pk),
      new SilentPaymentRegisteredKey(sender2Ph, sender2.pk),
    ],
    [
      new SilentPaymentRegisteredSecretKey(sender1Ph, sender1.sk),
      new SilentPaymentRegisteredSecretKey(sender2Ph, sender2.sk),
    ],
  );

  const deltas = spends.apply(actions);

  // Pass Relation.assertConcurrent() so the driver-side gate
  // (non_ephemeral_xch_count >= 2) is satisfied. Without it,
  // DriverError::SilentPaymentRequiresInputBinding fires inside prepare().
  const finished = spends.prepare(deltas, Relation.assertConcurrent());

  // Each coin is curried over its SYNTHETIC key; spend + sign with the
  // synthetic key pair.
  for (const pending of finished.pendingSpends()) {
    const pendingPh = pending.coin().puzzleHash;
    const isS1 = bytesEqual(pendingPh, sender1Ph);
    const syntheticPk = isS1 ? sender1SyntheticPk : sender2SyntheticPk;
    finished.insert(
      pending.coin().coinId(),
      clvm.standardSpend(
        syntheticPk,
        clvm.delegatedSpend(pending.conditions()),
      ),
    );
  }
  finished.spend();

  sim.spendCoins(clvm.coinSpends(), [sender1SyntheticSk, sender2SyntheticSk]);

  // Entry point: drive TweakData construction through the new
  // helper, not the older Simulator.tweakDataFromBlock path. The
  // Simulator facade exposes block_spends / block_outputs.
  const blockSpends = sim.blockSpends(heightBefore);
  const blockAdditions = sim.blockOutputs(heightBefore);
  const tweakData = SilentPayments.tweakDataFromBlockSpends(
    blockSpends,
    blockAdditions,
  );
  t.is(
    tweakData.tweakPoints.length,
    3,
    // Additive ScanBlock model (tweak_data_from_block_spends): a 2-input
    // concurrent SP send emits 2 Pass-1 singletons + 1 Pass-2 SCC aggregate
    // = 3 candidate tweak_points. Only the SCC-aggregate point matches the
    // sender-derived input_hash, so the scanner still detects exactly one
    // output (asserted below). Mirrors the Rust inline test
    // block_tweak_data.rs::same_ph_multi_input_round_trip_via_concurrent_spend.
    "additive model: 2 Pass-1 singletons + 1 Pass-2 SCC aggregate",
  );

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
  t.is(detections[0].k, 0, "first output at this scan_pk -> k=0");
  // wasm bindy emits `Option<u32>` as `number | undefined` (not null).
  t.is(detections[0].label, undefined, "unlabeled detection -> label is undefined");
  t.is(detections[0].amount, 700n, "multi-input SP output amount round-trips");
});
