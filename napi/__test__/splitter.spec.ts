import test from "ava";

import {
  BlsPair,
  Clvm,
  Coin,
  Constants,
  Simulator,
  Splitter,
  SplitterShare,
  standardPuzzleHash,
} from "../index.js";

test("splits a coin between recipients", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1003n);
  const bob = standardPuzzleHash(BlsPair.fromSeed(1n).pk);
  const carol = standardPuzzleHash(BlsPair.fromSeed(2n).pk);

  const splitter = new Splitter([
    new SplitterShare(bob, 80n),
    new SplitterShare(carol, 20n),
  ]);

  t.is(splitter.totalShares(), 100n);

  const payouts = splitter.payouts(1003n);
  t.deepEqual(
    payouts.map((payout) => payout.amount),
    [802n, 201n],
  );

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([clvm.createCoin(splitter.puzzleHash(), 1003n)]),
  );

  const coin = new Coin(alice.coin.coinId(), splitter.puzzleHash(), 1003n);
  const spend = splitter.innerSpend(clvm, coin.amount);

  const parsed = spend.puzzle.puzzle().parseSplitter();
  t.is(parsed?.totalShares(), 100n);
  t.deepEqual(parsed?.puzzleHash(), splitter.puzzleHash());

  clvm.spendCoin(coin, spend);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  for (const payout of payouts) {
    const child = new Coin(coin.coinId(), payout.puzzleHash, payout.amount);
    t.truthy(sim.coinState(child.coinId()));
  }

  t.is(Constants.splitterHash().length, 32);
});

test("rejects invalid payout schemes", (t) => {
  const recipient = standardPuzzleHash(BlsPair.fromSeed(1n).pk);

  t.throws(() => new Splitter([]));
  t.throws(() => new Splitter([new SplitterShare(recipient, 0n)]));
  t.throws(
    () =>
      new Splitter([
        new SplitterShare(recipient, 1n),
        new SplitterShare(recipient, 1n),
      ]),
  );
});
