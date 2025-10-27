import test from "ava";
import {
  Action,
  BlsPair,
  catPuzzleHash,
  Clvm,
  Coin,
  Delta,
  Deltas,
  Id,
  Outputs,
  selectCoins,
  Simulator,
  Spends,
  standardPuzzleHash,
} from "..";

class Wallet {
  pair: BlsPair;
  puzzleHash: Uint8Array;

  constructor(index: bigint) {
    this.pair = BlsPair.fromSeed(index);
    this.puzzleHash = standardPuzzleHash(this.pair.pk);
  }

  addXch(sim: Simulator, amount: bigint) {
    sim.newCoin(this.puzzleHash, amount);
  }

  fetchXch(sim: Simulator) {
    return sim.unspentCoins(this.puzzleHash, false);
  }

  fetchCatCoins(sim: Simulator, assetId: Uint8Array) {
    return sim.unspentCoins(catPuzzleHash(assetId, this.puzzleHash), false);
  }

  fetchCat(sim: Simulator, coin: Coin) {
    const parentSpend = sim.coinSpend(coin.parentCoinInfo);
    if (!parentSpend) throw new Error("Parent spend not found");

    const clvm = new Clvm();
    const puzzle = clvm.deserialize(parentSpend.puzzleReveal).puzzle();
    const solution = clvm.deserialize(parentSpend.solution);
    const children = puzzle.parseChildCats(parentSpend.coin, solution) ?? [];
    const cat = children.find((cat) => cat.coin.coinId() === coin.coinId());
    if (!cat) throw new Error("Cat not found");

    return cat;
  }

  balance(sim: Simulator, id: Id) {
    const existing = id.asExisting();

    if (id.isXch()) {
      return this.fetchXch(sim).reduce((acc, coin) => acc + coin.amount, 0n);
    } else if (existing) {
      return this.fetchCatCoins(sim, existing).reduce(
        (acc, coin) => acc + coin.amount,
        0n
      );
    } else {
      return 0n;
    }
  }

  selectCoins(sim: Simulator, spends: Spends, actions: Action[]) {
    const deltas = Deltas.fromActions(actions);

    for (const id of deltas.ids()) {
      const delta = deltas.get(id) ?? new Delta(0n, 0n);

      let required = delta.output - delta.input;

      if (required < 0n) {
        required = 0n;
      }

      if (deltas.isNeeded(id) && required === 0n) {
        required = 1n;
      }

      if (required === 0n) {
        continue;
      }

      const existing = id.asExisting();

      if (id.isXch()) {
        const coins = this.fetchXch(sim);

        for (const selectedCoin of selectCoins(coins, required)) {
          spends.addXch(selectedCoin);
        }
      } else if (existing) {
        const coins = this.fetchCatCoins(sim, existing);

        for (const selectedCoin of selectCoins(coins, required)) {
          spends.addCat(this.fetchCat(sim, selectedCoin));
        }
      }
    }
  }

  spend(sim: Simulator, clvm: Clvm, actions: Action[]): Outputs {
    // Create a Spends object and insert coins we want to spend
    const spends = new Spends(clvm, this.puzzleHash);
    this.selectCoins(sim, spends, actions);

    // Apply actions and finish the proposed spends with the deltas
    const deltas = spends.apply(actions);
    const finished = spends.prepare(deltas);

    // Use the p2 puzzles to calculate the actual spends
    for (const spend of finished.pendingSpends()) {
      finished.insert(
        spend.coin().coinId(),
        clvm.standardSpend(
          this.pair.pk,
          clvm.delegatedSpend(spend.conditions())
        )
      );
    }

    // Finalize everything
    const outputs = finished.spend();
    sim.spendCoins(clvm.coinSpends(), [this.pair.sk]);

    return outputs;
  }
}

test("create a coin spend with the action system", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(0n);
  const bob = new Wallet(1n);

  alice.addXch(sim, 1000n);

  // Send 250 mojos to Bob
  alice.spend(sim, clvm, [Action.send(Id.xch(), bob.puzzleHash, 250n)]);

  // Make sure that Bob can spend his new coin
  bob.spend(sim, clvm, [Action.send(Id.xch(), alice.puzzleHash, 250n)]);

  // And Alice got her change back automatically
  for (let i = 0; i < 10; i++) {
    alice.spend(sim, clvm, [Action.send(Id.xch(), alice.puzzleHash, 750n)]);
  }

  // Alice has a total of 1000 mojos since Bob sent the 250 mojos back
  alice.spend(sim, clvm, [Action.send(Id.xch(), alice.puzzleHash, 1000n)]);

  // However, Alice cannot spend money she doesn't have
  t.throws(() => {
    alice.spend(sim, clvm, [Action.send(Id.xch(), alice.puzzleHash, 1001n)]);
  });
});
