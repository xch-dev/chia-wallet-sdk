import test from "ava";
import { Action, Clvm, Coin, Simulator, Spends } from "..";

test("create a coin spend with the action system", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1000n);
  const bob = sim.bls(0n);

  // Create a Spends object and insert coins we want to spend
  const spends = new Spends(clvm, alice.puzzleHash);
  spends.addXch(alice.coin);

  // Apply actions and finish the proposed spends with the deltas
  const deltas = spends.apply([Action.sendXch(bob.puzzleHash, 250n)]);
  const finished = spends.prepare(deltas);

  // Use the p2 puzzles to calculate the actual spends
  for (const spend of finished.pendingSpends()) {
    finished.insert(
      spend.coin().coinId(),
      clvm.standardSpend(alice.pk, clvm.delegatedSpend(spend.conditions()))
    );
  }

  // Finalize everything
  finished.spend();
  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  // Make sure that Bob can spend his new coin
  const output = new Coin(alice.coin.coinId(), bob.puzzleHash, 250n);

  clvm.spendStandardCoin(
    output,
    bob.pk,
    clvm.delegatedSpend([clvm.createCoin(alice.puzzleHash, 250n)])
  );

  sim.spendCoins(clvm.coinSpends(), [bob.sk]);

  // And Alice got her change back automatically
  const change = new Coin(alice.coin.coinId(), alice.puzzleHash, 750n);

  clvm.spendStandardCoin(
    change,
    alice.pk,
    clvm.delegatedSpend([clvm.createCoin(alice.puzzleHash, 750n)])
  );

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.pass();
});
