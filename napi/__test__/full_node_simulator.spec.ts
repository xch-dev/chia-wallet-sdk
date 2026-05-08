import test from "ava";

import {
  bytesEqual,
  FullNodeSimulator,
  SecretKey,
  standardPuzzleHash,
} from "../index.js";

test("full node simulator exposes prefarm rewards", (t) => {
  const sim = new FullNodeSimulator();
  const prefarmPuzzleHash = sim.getPrefarmPuzzleHash();

  t.true(bytesEqual(sim.getFarmingPh(), prefarmPuzzleHash));

  const prefarmRecords =
    sim.getCoinRecordsByPuzzleHash(prefarmPuzzleHash).coinRecords ?? [];
  t.is(prefarmRecords.length, 2);
  t.true(prefarmRecords.every((record) => record.coinbase));
  t.true(prefarmRecords.every((record) => !record.spent));
  t.is(
    prefarmRecords.reduce((sum, record) => sum + record.coin.amount, 0n),
    21_000_000_000_000_000_000n
  );

  const genesis = sim.getBlockRecordByHeight(0).blockRecord!;
  t.is(genesis.rewardClaimsIncorporated?.length, 2);
  t.true(
    genesis.rewardClaimsIncorporated!.every((coin) =>
      bytesEqual(coin.puzzleHash, prefarmPuzzleHash)
    )
  );
});

test("full node simulator derives prefarm from explicit secret key", (t) => {
  const rootSecretKey = SecretKey.fromSeed(Buffer.alloc(32, 42));
  const expectedPrefarmSecretKey = rootSecretKey
    .deriveHardenedPath([12381, 8444, 2, 1])
    .deriveSynthetic();
  const expectedPrefarmPuzzleHash = standardPuzzleHash(
    expectedPrefarmSecretKey.publicKey()
  );

  const sim = new FullNodeSimulator(rootSecretKey);
  const derivedPrefarmSecretKey = sim
    .getMasterSecretKey()
    .deriveHardenedPath([12381, 8444, 2, 1])
    .deriveSynthetic();
  t.true(
    bytesEqual(
      derivedPrefarmSecretKey.toBytes(),
      expectedPrefarmSecretKey.toBytes()
    )
  );
  t.true(bytesEqual(sim.getPrefarmPuzzleHash(), expectedPrefarmPuzzleHash));
});

test("full node simulator includes farmed rewards in block records", (t) => {
  const sim = FullNodeSimulator.withSeed(123n);
  const block = sim.farmBlock(1)[0];

  t.is(block.rewardClaimsIncorporated?.length, 1);
  const reward = block.rewardClaimsIncorporated![0];
  t.is(reward.amount, 2_000_000_000_000n);
  t.true(bytesEqual(reward.puzzleHash, sim.getPrefarmPuzzleHash()));

  const rewardRecord = sim.getCoinRecordByName(reward.coinId()).coinRecord;
  t.truthy(rewardRecord);
  t.true(rewardRecord!.coinbase);
  t.false(rewardRecord!.spent);
});

test("full node simulator autofarm defaults on and can be toggled", (t) => {
  const sim = new FullNodeSimulator();
  t.true(sim.getAutofarm());

  sim.setAutofarm(false);
  t.false(sim.getAutofarm());

  sim.setAutofarm(true);
  t.true(sim.getAutofarm());
});
