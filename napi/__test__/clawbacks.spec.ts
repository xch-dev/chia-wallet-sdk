import test from "ava";

import {
  BlsPair,
  ClawbackV2,
  Clvm,
  Coin,
  Simulator,
  standardPuzzleHash,
} from "../index.js";

test("test clawback v2 (sender spend)", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);
  const bob = BlsPair.fromSeed(42n);
  const bobPuzzleHash = standardPuzzleHash(bob.pk);

  const clawback = new ClawbackV2(
    alice.puzzleHash,
    bobPuzzleHash,
    5n,
    1n,
    false
  );

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(
        clawback.puzzleHash(),
        1n,
        clvm.alloc([clawback.memo(clvm)])
      ),
    ])
  );

  const clawbackCoin = new Coin(alice.coin.coinId(), clawback.puzzleHash(), 1n);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  const clawbackSpend = clawback.senderSpend(
    clvm.standardSpend(
      alice.pk,
      clvm.delegatedSpend([clvm.createCoin(alice.puzzleHash, 1n)])
    )
  );
  clvm.spendCoin(clawbackCoin, clawbackSpend);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(true);
});

test("test clawback v2 (receiver spend)", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);
  const bob = BlsPair.fromSeed(42n);
  const bobPuzzleHash = standardPuzzleHash(bob.pk);

  const clawback = new ClawbackV2(
    alice.puzzleHash,
    bobPuzzleHash,
    5n,
    1n,
    false
  );

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(
        clawback.puzzleHash(),
        1n,
        clvm.alloc([clawback.memo(clvm)])
      ),
    ])
  );

  const clawbackCoin = new Coin(alice.coin.coinId(), clawback.puzzleHash(), 1n);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);
  sim.passTime(10n);

  const clawbackSpend = clawback.receiverSpend(
    clvm.standardSpend(
      bob.pk,
      clvm.delegatedSpend([clvm.createCoin(bobPuzzleHash, 1n)])
    )
  );
  clvm.spendCoin(clawbackCoin, clawbackSpend);

  sim.spendCoins(clvm.coinSpends(), [bob.sk]);

  t.true(true);
});

test("test clawback v2 (push through)", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);
  const bob = BlsPair.fromSeed(42n);
  const bobPuzzleHash = standardPuzzleHash(bob.pk);

  const clawback = new ClawbackV2(
    alice.puzzleHash,
    bobPuzzleHash,
    5n,
    1n,
    false
  );

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(
        clawback.puzzleHash(),
        1n,
        clvm.alloc([clawback.memo(clvm)])
      ),
    ])
  );

  const clawbackCoin = new Coin(alice.coin.coinId(), clawback.puzzleHash(), 1n);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);
  sim.passTime(10n);

  const clawbackSpend = clawback.pushThroughSpend(clvm);
  clvm.spendCoin(clawbackCoin, clawbackSpend);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});
