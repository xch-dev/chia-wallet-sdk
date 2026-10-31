import test from "ava";

import {
  BlsPair,
  Clvm,
  Coin,
  Constants,
  PlotNft,
  PoolSingletonState,
  PoolState,
  Simulator,
  poolRewardParentId,
  standardPuzzleHash,
  toHex,
} from "../index.js";

// The simulator validates spends with the testnet11 constants
const GENESIS_CHALLENGE = Buffer.from(
  "37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615",
  "hex",
);

const REWARD = 1750000000000n;
const DELAY = 604800n;

function launch(sim: Simulator, clvm: Clvm, state: PoolState) {
  const alice = sim.bls(1n);
  const mint = clvm.mintPlotNft(
    alice.coin.coinId(),
    state,
    DELAY,
    alice.puzzleHash,
    GENESIS_CHALLENGE,
  );

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend(mint.parentConditions),
  );

  const coinSpends = clvm.coinSpends();
  const launcherSpend = coinSpends.find(
    (coinSpend) =>
      toHex(coinSpend.coin.puzzleHash) ===
      toHex(Constants.singletonLauncherHash()),
  )!;

  sim.spendCoins(coinSpends, [alice.sk]);

  return {
    plotNft: mint.plotNft,
    launcherSpend,
    delayedPuzzleHash: alice.puzzleHash,
  };
}

function reward(sim: Simulator, plotNft: PlotNft, height: number) {
  const coin = new Coin(
    poolRewardParentId(GENESIS_CHALLENGE, height),
    plotNft.info.p2PuzzleHash(),
    REWARD,
  );
  sim.insertCoin(coin);
  return coin;
}

test("full plot NFT lifecycle", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const owner = BlsPair.fromSeed(1n);
  const selfTarget = standardPuzzleHash(owner.pk);
  const poolTarget = standardPuzzleHash(BlsPair.fromSeed(2n).pk);

  const { plotNft: launched, launcherSpend } = launch(
    sim,
    clvm,
    PoolState.selfPooling(selfTarget, owner.pk),
  );

  t.is(launched.info.poolState.state, PoolSingletonState.SelfPooling);
  t.true(launched.info.poolState.isWaitingRoom());
  t.is(toHex(launched.coin.puzzleHash), toHex(launched.info.puzzleHash()));

  const parsed = PlotNft.parseLauncher(
    launcherSpend.coin,
    clvm.deserialize(launcherSpend.solution),
    GENESIS_CHALLENGE,
  );
  t.is(toHex(parsed!.coin.coinId()), toHex(launched.coin.coinId()));

  // Claiming while self pooling
  const selfReward = reward(sim, launched, 100);
  const claim = launched.claimRewards(clvm, [selfReward]);
  sim.spendCoins(clvm.coinSpends(), []);
  t.truthy(
    sim.coinState(
      new Coin(launched.coin.coinId(), selfTarget, REWARD).coinId(),
    ),
  );

  // Joining a pool
  const url = "https://pool.example.com";
  const pooling = claim.plotNft.joinPool(clvm, poolTarget, url, 5);
  sim.spendCoins(clvm.coinSpends(), [owner.sk]);
  t.is(pooling.info.poolState.state, PoolSingletonState.FarmingToPool);
  t.is(pooling.info.poolState.poolUrl, url);
  t.is(
    toHex(pooling.info.p2PuzzleHash()),
    toHex(launched.info.p2PuzzleHash()),
  );

  // Rewards go to the pool now
  const poolClaim = pooling.claimRewards(clvm, [reward(sim, pooling, 101)]);
  sim.spendCoins(clvm.coinSpends(), []);
  t.truthy(
    sim.coinState(
      new Coin(pooling.coin.coinId(), poolTarget, REWARD).coinId(),
    ),
  );

  // Syncing from the spend gives the same plot NFT
  const leaving = poolClaim.plotNft.leavePool(clvm);
  const leaveSpends = clvm.coinSpends();
  sim.spendCoins(leaveSpends, [owner.sk]);
  t.is(leaving.info.poolState.state, PoolSingletonState.LeavingPool);

  const synced = poolClaim.plotNft.parseChild(
    clvm.deserialize(leaveSpends[0].solution),
  );
  t.is(toHex(synced!.coin.coinId()), toHex(leaving.coin.coinId()));
  t.is(synced!.info.poolState.poolUrl, url);

  // The waiting room lock is enforced
  leaving.selfPool(clvm, selfTarget);
  t.throws(() => sim.spendCoins(clvm.coinSpends(), [owner.sk]));

  for (let i = 0; i < 5; i++) {
    sim.createBlock();
  }

  const done = leaving.selfPool(clvm, selfTarget);
  sim.spendCoins(clvm.coinSpends(), [owner.sk]);
  t.is(done.info.poolState.state, PoolSingletonState.SelfPooling);
  t.truthy(sim.coinState(done.coin.coinId()));
});

test("rejects invalid plot NFT operations", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const owner = BlsPair.fromSeed(1n);
  const target = standardPuzzleHash(owner.pk);

  const { plotNft } = launch(
    sim,
    clvm,
    PoolState.selfPooling(target, owner.pk),
  );

  t.throws(() => plotNft.leavePool(clvm));
  t.throws(() => plotNft.selfPool(clvm, target));
  t.throws(() => plotNft.joinPool(clvm, target, "https://a.b", 4));
  t.throws(() => plotNft.claimRewards(clvm, []));
  t.throws(() =>
    plotNft.claimRewards(clvm, [
      new Coin(Buffer.alloc(32, 1), plotNft.info.p2PuzzleHash(), REWARD),
    ]),
  );

  t.is(Constants.poolMemberInnerpuzzleHash().length, 32);
});

test("delayed claim", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const owner = BlsPair.fromSeed(1n);

  const { plotNft, delayedPuzzleHash } = launch(
    sim,
    clvm,
    PoolState.selfPooling(standardPuzzleHash(owner.pk), owner.pk),
  );

  const coin = sim.newCoin(plotNft.info.p2PuzzleHash(), 1234n);
  sim.createBlock();

  plotNft.info.claimDelayed(clvm, coin);
  t.throws(() => sim.spendCoins(clvm.coinSpends(), []));

  sim.passTime(DELAY);
  const swept = plotNft.info.claimDelayed(clvm, coin);
  sim.spendCoins(clvm.coinSpends(), []);

  t.is(toHex(swept.puzzleHash), toHex(delayedPuzzleHash));
  t.truthy(sim.coinState(swept.coinId()));
});
