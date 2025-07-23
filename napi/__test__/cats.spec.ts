import test from "ava";

import {
  Cat,
  CatInfo,
  CatSpend,
  Clvm,
  Coin,
  Constants,
  Program,
  Simulator,
} from "../index.js";

test("issues and spends a cat", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);

  const tail = clvm.nil();
  const assetId = tail.treeHash();
  const catInfo = new CatInfo(assetId, null, alice.puzzleHash);

  // Issue a CAT
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([clvm.createCoin(catInfo.puzzleHash(), 1n)])
  );

  const eve = new Cat(
    new Coin(alice.coin.coinId(), catInfo.puzzleHash(), 1n),
    null,
    catInfo
  );

  clvm.spendCats([
    new CatSpend(
      eve,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([
          clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
          clvm.runCatTail(tail, clvm.nil()),
        ])
      )
    ),
  ]);

  // Spend the CAT
  const cat = eve.child(alice.puzzleHash, 1n);

  clvm.spendCats([
    new CatSpend(
      cat,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([
          clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
        ])
      )
    ),
  ]);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(true);
});

test("issues and melts a cat", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1000n);

  const tail = clvm.nil();
  const assetId = tail.treeHash();
  const catInfo = new CatInfo(assetId, null, alice.puzzleHash);

  // Issue a CAT
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([clvm.createCoin(catInfo.puzzleHash(), 1000n)])
  );

  const eve = new Cat(
    new Coin(alice.coin.coinId(), catInfo.puzzleHash(), 1000n),
    null,
    catInfo
  );

  const cats = clvm.spendCats([
    new CatSpend(
      eve,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([
          clvm.runCatTail(tail, clvm.nil()),
          clvm.createCoin(alice.puzzleHash, 300n),
          clvm.createCoin(alice.puzzleHash, 500n),
          clvm.createCoin(alice.puzzleHash, 200n),
        ])
      )
    ),
  ]);

  // Spend the CAT
  clvm.spendCats(
    cats.map((cat, i) => {
      const conditions: Program[] = [];

      if (i === 1) {
        conditions.push(clvm.runCatTail(tail, clvm.nil()));
      }

      return new CatSpend(
        cat,
        clvm.standardSpend(alice.pk, clvm.delegatedSpend(conditions))
      );
    })
  );

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(true);
});

test("issues and spends a revocable cat", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);
  const bob = sim.bls(1n);

  const tail = clvm.nil();
  const assetId = tail.treeHash();
  const catInfo = new CatInfo(assetId, bob.puzzleHash, alice.puzzleHash);

  // Issue a CAT
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([clvm.createCoin(catInfo.puzzleHash(), 1n)])
  );

  const eve = new Cat(
    new Coin(alice.coin.coinId(), catInfo.puzzleHash(), 1n),
    null,
    catInfo
  );

  // Spend the CAT
  clvm.spendCats([
    new CatSpend(
      eve,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([
          clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
          clvm.runCatTail(tail, clvm.nil()),
        ])
      )
    ),
  ]);

  // Revoke the CAT
  const cat = eve.child(alice.puzzleHash, 1n);

  const [output] = clvm.spendCats([
    CatSpend.revoke(
      cat,
      clvm.standardSpend(
        bob.pk,
        clvm.delegatedSpend([
          clvm.createCoin(bob.puzzleHash, 1n, clvm.alloc([bob.puzzleHash])),
        ])
      )
    ),
  ]);

  sim.spendCoins(clvm.coinSpends(), [alice.sk, bob.sk]);

  // No longer revocable
  t.is(output.info.hiddenPuzzleHash, null);
});

test("parses a cat puzzle", (t) => {
  const clvm = new Clvm();

  const settlementPuzzle = clvm.deserialize(Constants.settlementPayment());
  const catPuzzle = clvm.deserialize(Constants.catPuzzle());

  const assetId = Buffer.from("00".repeat(32), "hex");

  const puzzle = catPuzzle
    .curry([
      clvm.alloc(Constants.catPuzzleHash()),
      clvm.alloc(assetId),
      settlementPuzzle,
    ])
    .puzzle();

  const parsed = puzzle.parseCat();
  if (!parsed?.p2Puzzle) throw new Error("Failed to parse cat");

  t.is(parsed.info.assetId.toString("hex"), assetId.toString("hex"));
  t.is(parsed.info.hiddenPuzzleHash, null);
  t.is(
    parsed.info.p2PuzzleHash.toString("hex"),
    Constants.settlementPaymentHash().toString("hex")
  );
  t.is(
    parsed.p2Puzzle?.program.treeHash().toString("hex"),
    Constants.settlementPaymentHash().toString("hex")
  );
});
