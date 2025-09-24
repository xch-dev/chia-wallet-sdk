import test from "ava";
import { BulletinMessage, bulletinPuzzleHash, Clvm, Simulator } from "..";

test("issues and spends a cat", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(0n);

  const created = clvm.createBulletin(alice.coin.coinId(), alice.puzzleHash, [
    new BulletinMessage(
      "animals/rabbit/vienna-blue",
      "The Vienna Blue rabbit breed originally comes from Austria."
    ),
  ]);

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend(created.parentConditions)
  );

  const bulletinSpend = clvm.standardSpend(
    alice.pk,
    clvm.delegatedSpend(created.bulletin.conditions(clvm))
  );

  created.bulletin.spend(bulletinSpend);

  const coinSpends = clvm.coinSpends();

  sim.spendCoins(coinSpends, [alice.sk]);

  const coinSpend = coinSpends.find((spend) =>
    spend.coin.coinId().equals(created.bulletin.coin.coinId())
  );
  if (!coinSpend) {
    throw new Error("Coin spend not found");
  }

  const puzzle = clvm.deserialize(coinSpend.puzzleReveal).puzzle();
  const solution = clvm.deserialize(coinSpend.solution);

  const bulletin = puzzle.parseBulletin(coinSpend.coin, solution);
  if (!bulletin) {
    throw new Error("Bulletin not found");
  }

  t.is(bulletin.messages.length, created.bulletin.messages.length);

  for (let i = 0; i < bulletin.messages.length; i++) {
    t.is(bulletin.messages[i].topic, created.bulletin.messages[i].topic);
    t.is(bulletin.messages[i].content, created.bulletin.messages[i].content);
  }

  t.is(
    bulletin.coin.coinId().toString("hex"),
    created.bulletin.coin.coinId().toString("hex")
  );

  t.is(
    bulletin.hiddenPuzzleHash.toString("hex"),
    created.bulletin.hiddenPuzzleHash.toString("hex")
  );

  t.is(
    bulletin.coin.puzzleHash.toString("hex"),
    bulletinPuzzleHash(bulletin.hiddenPuzzleHash).toString("hex")
  );
});
