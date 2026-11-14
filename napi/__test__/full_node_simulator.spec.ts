import test from "ava";

import {
  bytesEqual,
  Clvm,
  CoinSpend,
  FullNodeSimulator,
  RpcClient,
  SecretKey,
  Signature,
  SpendBundle,
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
    21_000_000_000_000_000_000n,
  );

  const genesis = sim.getBlockRecordByHeight(0).blockRecord!;
  t.is(genesis.rewardClaimsIncorporated?.length, 2);
  t.true(
    genesis.rewardClaimsIncorporated!.every((coin) =>
      bytesEqual(coin.puzzleHash, prefarmPuzzleHash),
    ),
  );
});

test("full node simulator derives prefarm from explicit secret key", (t) => {
  const rootSecretKey = SecretKey.fromSeed(Buffer.alloc(32, 42));
  const expectedPrefarmSecretKey = rootSecretKey
    .deriveHardenedPath([12381, 8444, 2, 1])
    .deriveSynthetic();
  const expectedPrefarmPuzzleHash = standardPuzzleHash(
    expectedPrefarmSecretKey.publicKey(),
  );

  const sim = FullNodeSimulator.withSecretKey(rootSecretKey);
  const derivedPrefarmSecretKey = sim
    .getMasterSecretKey()
    .deriveHardenedPath([12381, 8444, 2, 1])
    .deriveSynthetic();
  t.true(
    bytesEqual(
      derivedPrefarmSecretKey.toBytes(),
      expectedPrefarmSecretKey.toBytes(),
    ),
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

test("full node simulator push tx waits for manual farming", (t) => {
  const sim = new FullNodeSimulator();
  const clvm = new Clvm();
  const puzzle = clvm.parse("1");
  const puzzleHash = puzzle.treeHash();
  const coin = sim.newCoin(puzzleHash, 100n);

  const spendBundle = new SpendBundle(
    [
      new CoinSpend(
        coin,
        puzzle.serialize(),
        clvm
          .parse(`((51 0x${Buffer.from(puzzleHash).toString("hex")} 99))`)
          .serialize(),
      ),
    ],
    Signature.infinity(),
  );

  t.true(sim.pushTx(spendBundle).success);
  t.is(sim.getBlockchainState().blockchainState?.mempoolSize, 1);

  sim.farmBlock(1);
  t.is(sim.getBlockchainState().blockchainState?.mempoolSize, 0);
});

test("full node simulator validates signatures with the genesis challenge", (t) => {
  const sim = new FullNodeSimulator();
  const genesisChallenge = new Uint8Array(32).fill(7);
  const clvm = new Clvm();
  const puzzle = clvm.parse("1");
  const coin = sim.newCoin(puzzle.treeHash(), 0n);

  const secretKey = SecretKey.fromSeed(new Uint8Array(32).fill(1));
  const publicKey = Buffer.from(secretKey.publicKey().toBytes()).toString(
    "hex",
  );
  const message = Buffer.from("hello");

  // AGG_SIG_ME signs the message, the coin id and the genesis challenge.
  const spendBundle = new SpendBundle(
    [
      new CoinSpend(
        coin,
        puzzle.serialize(),
        clvm
          .parse(`((50 0x${publicKey} 0x${message.toString("hex")}))`)
          .serialize(),
      ),
    ],
    secretKey.sign(Buffer.concat([message, coin.coinId(), genesisChallenge])),
  );

  t.false(sim.pushTx(spendBundle).success);

  sim.setGenesisChallenge(genesisChallenge);
  t.true(bytesEqual(sim.getAggsigAdditionalData(), genesisChallenge));
  t.true(bytesEqual(sim.getNetworkInfo().genesisChallenge!, genesisChallenge));
  t.true(sim.pushTx(spendBundle).success);
});

test("full node simulator drops transactions whose expiry has passed", (t) => {
  const sim = new FullNodeSimulator();
  const clvm = new Clvm();
  const puzzle = clvm.parse("1");
  const coin = sim.newCoin(puzzle.treeHash(), 0n);

  // ASSERT_BEFORE_SECONDS_ABSOLUTE one minute from the next block.
  const expiresAt = sim.nextTimestamp() + 60n;
  const spendBundle = new SpendBundle(
    [
      new CoinSpend(
        coin,
        puzzle.serialize(),
        clvm.parse(`((85 ${expiresAt}))`).serialize(),
      ),
    ],
    Signature.infinity(),
  );
  t.true(sim.pushTx(spendBundle).success);

  t.throws(() => sim.setNextTimestamp(0n));
  sim.passTime(60n);
  t.is(sim.nextTimestamp(), expiresAt);
  sim.farmBlock(1);

  t.is(sim.getBlockchainState().blockchainState?.mempoolSize, 0);
  t.false(sim.getCoinRecordByName(coin.coinId()).coinRecord!.spent);
});

test("full node simulator can serve rpc over http", async (t) => {
  const sim = new FullNodeSimulator();
  const server = await sim.startServer();

  try {
    const rpc = new RpcClient(server.url);
    const networkInfo = await rpc.getNetworkInfo();
    t.true(networkInfo.success);
    t.is(networkInfo.networkName, "simulator0");

    const clvm = new Clvm();
    const puzzle = clvm.parse("1");
    const puzzleHash = puzzle.treeHash();
    const coin = sim.newCoin(puzzleHash, 100n);

    const spendBundle = new SpendBundle(
      [
        new CoinSpend(
          coin,
          puzzle.serialize(),
          clvm
            .parse(`((51 0x${Buffer.from(puzzleHash).toString("hex")} 99))`)
            .serialize(),
        ),
      ],
      Signature.infinity(),
    );

    const pushResponse = await rpc.pushTx(spendBundle);
    t.true(pushResponse.success, pushResponse.error ?? undefined);

    const state = await rpc.getBlockchainState();
    t.is(state.blockchainState?.mempoolSize, 1);
  } finally {
    server.close();
  }
});

test("full node simulator can dump and restore state", (t) => {
  const sim = FullNodeSimulator.withSeed(123n);
  sim.farmBlock(2);
  const state = sim.dumpState();
  const peak = sim.headerHash();

  const expected = FullNodeSimulator.withSeed(999n);
  expected.restoreState(state);
  const expectedNext = expected.farmBlock(1)[0].headerHash;

  sim.farmBlock(1);
  t.false(bytesEqual(sim.headerHash(), peak));

  sim.restoreState(state);
  t.true(bytesEqual(sim.headerHash(), peak));
  t.deepEqual(sim.drainEvents(), []);
  t.true(bytesEqual(sim.farmBlock(1)[0].headerHash, expectedNext));
  t.is(sim.getBlockchainState().blockchainState?.mempoolSize, 0);
});
