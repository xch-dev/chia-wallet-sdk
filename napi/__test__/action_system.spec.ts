import test from "ava";
import {
  Action,
  BlsPair,
  Cat,
  catPuzzleHash,
  Clvm,
  Coin,
  CoinSpend,
  Constants,
  Delta,
  Deltas,
  Id,
  Nft,
  NftMetadata,
  OptionType,
  Outputs,
  Relation,
  selectCoins,
  Simulator,
  Spend,
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
    const cat = children.find((cat) => cat.coin.coinId().equals(coin.coinId()));
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
        0n,
      );
    } else {
      return 0n;
    }
  }

  selectCoins(
    sim: Simulator,
    spends: Spends,
    actions: Action[],
    reservedNfts: Map<string, Nft>,
  ) {
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
        const assetHex = Buffer.from(existing).toString("hex");
        const reserved = reservedNfts.get(assetHex);
        if (reserved) {
          spends.addNft(reserved);
          reservedNfts.delete(assetHex);
          continue;
        }
        const coins = this.fetchCatCoins(sim, existing);

        for (const selectedCoin of selectCoins(coins, required)) {
          spends.addCat(this.fetchCat(sim, selectedCoin));
        }
      }
    }
  }

  spend(
    sim: Simulator,
    clvm: Clvm,
    actions: Action[],
    extras?: { nfts?: Nft[] },
  ): Outputs {
    // Create a Spends object and insert coins we want to spend
    const spends = new Spends(clvm, this.puzzleHash);
    const reservedNfts = new Map<string, Nft>();
    if (extras?.nfts) {
      for (const nft of extras.nfts) {
        const launcherId = nft.info.launcherId as Uint8Array;
        reservedNfts.set(Buffer.from(launcherId).toString("hex"), nft);
      }
    }
    this.selectCoins(sim, spends, actions, reservedNfts);

    // Apply actions and finish the proposed spends with the deltas
    const deltas = spends.apply(actions);
    const finished = spends.prepare(deltas);

    // Use the p2 puzzles to calculate the actual spends
    for (const spend of finished.pendingSpends()) {
      finished.insert(
        spend.coin().coinId(),
        clvm.standardSpend(
          this.pair.pk,
          clvm.delegatedSpend(spend.conditions()),
        ),
      );
    }

    // Finalize everything
    const outputs = finished.spend();
    sim.spendCoins(clvm.coinSpends(), [this.pair.sk]);

    return outputs;
  }

  // Builds the coin spends for the coins already added to the spends, without submitting them
  build(
    clvm: Clvm,
    spends: Spends,
    actions: Action[],
    relation?: Relation,
  ): { outputs: Outputs; coinSpends: CoinSpend[] } {
    const deltas = spends.apply(actions);
    const finished = spends.prepare(deltas, relation);

    for (const spend of finished.pendingSpends()) {
      finished.insert(
        spend.coin().coinId(),
        clvm.standardSpend(
          this.pair.pk,
          clvm.delegatedSpend(spend.conditions()),
        ),
      );
    }

    const outputs = finished.spend();
    return { outputs, coinSpends: clvm.coinSpends() };
  }

  // Finds the unspent CATs hinted to this wallet, by parsing their parent spends
  hintedCats(sim: Simulator): Cat[] {
    return sim
      .unspentCoins(this.puzzleHash, true)
      .filter((coin) => !coin.puzzleHash.equals(this.puzzleHash))
      .map((coin) => this.fetchCat(sim, coin));
  }

  // Spends the coins that have already been added to the spends, without any coin selection
  finish(
    sim: Simulator,
    clvm: Clvm,
    spends: Spends,
    actions: Action[],
    relation?: Relation,
  ): Outputs {
    const deltas = spends.apply(actions);
    const finished = spends.prepare(deltas, relation);

    for (const spend of finished.pendingSpends()) {
      finished.insert(
        spend.coin().coinId(),
        clvm.standardSpend(
          this.pair.pk,
          clvm.delegatedSpend(spend.conditions()),
        ),
      );
    }

    const outputs = finished.spend();
    sim.spendCoins(clvm.coinSpends(), [this.pair.sk]);

    return outputs;
  }
}

test("send xch", (t) => {
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

test("issue and send a cat", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(0n);
  const bob = new Wallet(1n);

  alice.addXch(sim, 1000n);

  // Issue a CAT
  const outputs = alice.spend(sim, clvm, [Action.singleIssueCat(null, 1000n)]);
  const id = Id.existing(outputs.cat(outputs.cats()[0])[0].info.assetId);

  // Send 250 mojos to Bob
  alice.spend(sim, clvm, [Action.send(id, bob.puzzleHash, 250n)]);

  // Make sure that Bob can spend his new coin
  bob.spend(sim, clvm, [Action.send(id, alice.puzzleHash, 250n)]);

  // And Alice got her change back automatically
  for (let i = 0; i < 10; i++) {
    alice.spend(sim, clvm, [Action.send(id, alice.puzzleHash, 750n)]);
  }

  // Alice has a total of 1000 mojos since Bob sent the 250 mojos back
  alice.spend(sim, clvm, [Action.send(id, alice.puzzleHash, 1000n)]);

  // However, Alice cannot spend money she doesn't have
  t.throws(() => {
    alice.spend(sim, clvm, [Action.send(id, alice.puzzleHash, 1001n)]);
  });
});

test("mint and update nft metadata", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(2n);
  alice.addXch(sim, 2_000n);

  const metadata = new NftMetadata(
    1n,
    1n,
    ["https://example.com/1"],
    null,
    [],
    null,
    [],
    null,
  );

  const mint = Action.mintNft(
    clvm,
    clvm.nftMetadata(metadata),
    Constants.nftMetadataUpdaterDefaultHash(),
    alice.puzzleHash,
    0,
    1n,
    null,
  );

  const metadataUpdate = new Spend(
    clvm.nftMetadataUpdaterDefault(),
    clvm.list([clvm.string("u"), clvm.string("https://example.com/2")]),
  );

  const update = Action.updateNft(Id.new(0n), [metadataUpdate]);

  const outputs = alice.spend(sim, clvm, [mint, update]);

  const nftId = outputs.nfts()[0];
  const nft = outputs.nft(nftId);
  const metadataSource = nft.info.metadata.unparse();

  t.is(outputs.nfts().length, 1);
  t.is(nftId.asNew(), 0n);
  t.truthy(sim.coinState(nft.coin.coinId()));
  t.truthy(metadataSource);
  t.true(metadataSource?.includes("https://example.com/2") ?? false);
});

test("update existing nft metadata", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(3n);
  alice.addXch(sim, 2_000n);

  // Mint the NFT that we will update later
  const metadata = new NftMetadata(
    1n,
    1n,
    ["https://example.com/1"],
    null,
    [],
    null,
    [],
    null,
  );

  const mint = Action.mintNft(
    clvm,
    clvm.nftMetadata(metadata),
    Constants.nftMetadataUpdaterDefaultHash(),
    alice.puzzleHash,
    0,
    1n,
    null,
  );

  const mintOutputs = alice.spend(sim, clvm, [mint]);
  const mintedId = mintOutputs.nfts()[0];
  const mintedNft = mintOutputs.nft(mintedId);

  // Update the metadata using the existing NFT
  const metadataUpdate = new Spend(
    clvm.nftMetadataUpdaterDefault(),
    clvm.list([clvm.string("u"), clvm.string("https://example.com/2")]),
  );

  const update = Action.updateNft(Id.existing(mintedNft.info.launcherId), [
    metadataUpdate,
  ]);

  const outputs = alice.spend(sim, clvm, [update], { nfts: [mintedNft] });

  const updatedNft = outputs.nft(Id.existing(mintedNft.info.launcherId));
  const previousState = sim.coinState(mintedNft.coin.coinId());
  const metadataSource = updatedNft.info.metadata.unparse();

  t.truthy(previousState);
  t.not(previousState?.spentHeight, null);
  t.truthy(sim.coinState(updatedNft.coin.coinId()));
  t.deepEqual(outputs.nfts(), [Id.existing(mintedNft.info.launcherId)]);
  t.truthy(metadataSource);
  t.true(metadataSource?.includes("https://example.com/2") ?? false);
});

test("create, update, send, and melt a did", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(4n);
  const bob = new Wallet(5n);

  alice.addXch(sim, 1n);

  const created = alice.spend(sim, clvm, [Action.createEmptyDid()]);
  t.deepEqual(created.dids(), [Id.new(0n)]);

  const did = created.did(Id.new(0n));
  const id = Id.existing(did.info.launcherId);

  const spends = new Spends(clvm, alice.puzzleHash);
  spends.addDid(did);

  const sent = alice.finish(sim, clvm, spends, [
    Action.updateDid(id, clvm.string("updated"), null, 1n),
    Action.send(id, bob.puzzleHash, 1n, clvm.alloc([bob.puzzleHash])),
  ]);

  const updated = sent.did(id);
  t.deepEqual(updated.info.p2PuzzleHash, bob.puzzleHash);
  t.is(updated.info.numVerificationsRequired, 1n);
  t.is(updated.info.metadata.toString(), "updated");
  t.is(sim.coinState(updated.coin.coinId())?.spentHeight, null);

  // The melted value is returned as change, so an XCH coin must be selected as well
  t.true(Deltas.fromActions([Action.meltSingleton(id, 1n)]).isNeeded(Id.xch()));

  const bobCoin = sim.newCoin(bob.puzzleHash, 0n);
  const meltSpends = new Spends(clvm, bob.puzzleHash);
  meltSpends.addDid(updated);
  meltSpends.addXch(bobCoin);

  const melted = bob.finish(sim, clvm, meltSpends, [
    Action.meltSingleton(id, 1n),
  ]);

  t.deepEqual(melted.dids(), []);
  t.is(bob.balance(sim, Id.xch()), 1n);
});

test("singleton amount mismatch is rejected", (t) => {
  const clvm = new Clvm();
  const alice = new Wallet(6n);

  const spends = new Spends(clvm, alice.puzzleHash);
  spends.addXch(new Coin(new Uint8Array(32), alice.puzzleHash, 10n));

  t.throws(
    () =>
      spends.apply([
        Action.createEmptyDid(),
        Action.send(Id.new(0n), alice.puzzleHash, 3n),
      ]),
    { message: /amount does not match/ },
  );
});

test("insufficient funds are rejected", (t) => {
  const clvm = new Clvm();
  const alice = new Wallet(7n);

  const spends = new Spends(clvm, alice.puzzleHash);
  spends.addXch(new Coin(new Uint8Array(32), alice.puzzleHash, 10n));

  const deltas = spends.apply([
    Action.send(Id.xch(), alice.puzzleHash, 5n),
    Action.fee(6n),
  ]);

  t.throws(() => spends.prepare(deltas), { message: /insufficient/ });
});

test("mint and send an option", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(8n);
  const bob = new Wallet(9n);

  alice.addXch(sim, 10n);

  const minted = alice.spend(sim, clvm, [
    Action.mintOption(
      alice.puzzleHash,
      100n,
      Id.xch(),
      5n,
      OptionType.xch(3n),
      1n,
    ),
  ]);

  t.deepEqual(minted.options(), [Id.new(0n)]);

  const option = minted.option(Id.new(0n));
  const id = Id.existing(option.info.launcherId);
  const underlying = minted
    .xch()
    .find((coin) => coin.coinId().equals(option.info.underlyingCoinId));

  t.truthy(underlying);
  t.is(underlying?.amount, 5n);
  t.is(alice.balance(sim, Id.xch()), 4n);

  const spends = new Spends(clvm, alice.puzzleHash);
  spends.addOption(option);

  const sent = alice.finish(sim, clvm, spends, [
    Action.send(id, bob.puzzleHash, 1n, clvm.alloc([bob.puzzleHash])),
  ]);

  const bobOption = sent.option(id);
  t.deepEqual(bobOption.info.p2PuzzleHash, bob.puzzleHash);
  t.is(sim.coinState(bobOption.coin.coinId())?.spentHeight, null);
});

test("fee accessors and separate change puzzle hash", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(10n);
  const bob = new Wallet(11n);

  alice.addXch(sim, 10n);

  const spends = Spends.withSeparateChangePuzzleHash(
    clvm,
    alice.puzzleHash,
    bob.puzzleHash,
  );

  for (const coin of alice.fetchXch(sim)) {
    spends.addXch(coin);
  }

  const outputs = alice.finish(sim, clvm, spends, [
    Action.burn(Id.xch(), 2n),
    Action.fee(3n),
  ]);

  t.is(outputs.fee(), 3n);
  t.is(outputs.reservedFee(), 3n);
  t.is(alice.balance(sim, Id.xch()), 0n);
  t.is(bob.balance(sim, Id.xch()), 5n);
});

test("assert concurrent relation links every spend", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(12n);
  const bob = new Wallet(13n);

  for (let i = 0; i < 3; i++) {
    alice.addXch(sim, 5n);
  }

  const coins = alice.fetchXch(sim);

  const countConcurrent = (relation?: Relation) => {
    const spends = new Spends(clvm, alice.puzzleHash);

    for (const coin of coins) {
      spends.addXch(coin);
    }

    const deltas = spends.apply([Action.send(Id.xch(), bob.puzzleHash, 12n)]);
    const finished = spends.prepare(deltas, relation);

    return finished
      .pendingSpends()
      .map(
        (spend) =>
          spend
            .conditions()
            .filter((condition) => condition.parseAssertConcurrentSpend())
            .length,
      );
  };

  t.deepEqual(countConcurrent(), [0, 0, 0]);
  t.deepEqual(countConcurrent(Relation.Unrelated), [0, 0, 0]);
  t.deepEqual(countConcurrent(Relation.AssertConcurrent), [1, 1, 1]);

  const spends = new Spends(clvm, alice.puzzleHash);

  for (const coin of coins) {
    spends.addXch(coin);
  }

  alice.finish(
    sim,
    clvm,
    spends,
    [Action.send(Id.xch(), bob.puzzleHash, 12n)],
    Relation.AssertConcurrent,
  );

  t.is(bob.balance(sim, Id.xch()), 12n);
  t.is(alice.balance(sim, Id.xch()), 3n);
});

const settlementPuzzleHash = Constants.settlementPaymentHash();

test("coin announcement relations link spends", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = new Wallet(19n);
  const bob = new Wallet(20n);

  for (let i = 0; i < 6; i++) {
    alice.addXch(sim, 5n);
  }

  const count = (spends: Spends, relation: Relation) => {
    const deltas = spends.apply([Action.send(Id.xch(), bob.puzzleHash, 3n)]);

    return spends
      .prepare(deltas, relation)
      .pendingSpends()
      .map((spend) => {
        const conditions = spend.conditions();
        return [
          conditions.filter((c) => c.parseCreateCoinAnnouncement()).length,
          conditions.filter((c) => c.parseAssertCoinAnnouncement()).length,
        ];
      });
  };

  const coins = alice.fetchXch(sim);
  const ringCoins = coins.slice(0, 3);
  const hubCoins = coins.slice(3);

  const addAll = (selected: Coin[]) => {
    const spends = new Spends(clvm, alice.puzzleHash);
    for (const coin of selected) {
      spends.addXch(coin);
    }
    return spends;
  };

  t.deepEqual(count(addAll(ringCoins), Relation.CoinAnnouncementRing), [
    [1, 1],
    [1, 1],
    [1, 1],
  ]);
  t.deepEqual(count(addAll(hubCoins), Relation.CoinAnnouncementHub), [
    [1, 0],
    [0, 1],
    [0, 1],
  ]);
  t.deepEqual(count(addAll([coins[0]]), Relation.CoinAnnouncementRing), [
    [0, 0],
  ]);

  alice.finish(
    sim,
    clvm,
    addAll(ringCoins),
    [Action.send(Id.xch(), bob.puzzleHash, 6n)],
    Relation.CoinAnnouncementRing,
  );
  alice.finish(
    sim,
    clvm,
    addAll(hubCoins),
    [Action.send(Id.xch(), bob.puzzleHash, 6n)],
    Relation.CoinAnnouncementHub,
  );

  t.is(bob.balance(sim, Id.xch()), 12n);
  t.is(alice.balance(sim, Id.xch()), 18n);
});

const issueRevocableCat = (
  sim: Simulator,
  clvm: Clvm,
  issuer: Wallet,
  holder: Wallet,
  amount: bigint,
) => {
  const tail = clvm
    .everythingWithSignature()
    .curry([clvm.atom(issuer.pair.pk.toBytes())]);
  const tailSpend = new Spend(tail, clvm.nil());

  issuer.addXch(sim, amount);
  issuer.spend(sim, clvm, [
    Action.issueCat(tailSpend, issuer.puzzleHash, amount),
    Action.send(
      Id.new(0n),
      holder.puzzleHash,
      amount,
      clvm.list([clvm.atom(holder.puzzleHash)]),
    ),
  ]);

  const cats = holder.hintedCats(sim);
  if (cats.length !== 1) throw new Error("Expected a single revocable CAT");

  return { tailSpend, cat: cats[0], id: Id.existing(cats[0].info.assetId) };
};

test("revoke a cat and send it", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const issuer = new Wallet(21n);
  const bob = new Wallet(22n);
  const carol = new Wallet(23n);

  const { cat, id } = issueRevocableCat(sim, clvm, issuer, bob, 10n);
  t.true(Buffer.from(cat.info.hiddenPuzzleHash!).equals(issuer.puzzleHash));

  const spends = new Spends(clvm, issuer.puzzleHash);
  spends.addCatForRevocation(cat);

  const actions = [
    Action.send(
      id,
      carol.puzzleHash,
      10n,
      clvm.list([clvm.atom(carol.puzzleHash)]),
    ),
  ];

  const pending = spends.prepare(spends.apply(actions)).pendingSpends();
  t.is(pending.length, 1);
  t.true(pending[0].isRevocation());
  t.true(pending[0].p2PuzzleHash().equals(issuer.puzzleHash));
  t.truthy(pending[0].asCat());

  const retry = new Spends(clvm, issuer.puzzleHash);
  retry.addCatForRevocation(cat);
  issuer.finish(sim, clvm, retry, actions);

  // Carol's coin is still revocable by the issuer
  const carolCats = carol.hintedCats(sim);
  t.is(carolCats.length, 1);
  t.is(carolCats[0].coin.amount, 10n);
  t.true(
    Buffer.from(carolCats[0].info.hiddenPuzzleHash!).equals(issuer.puzzleHash),
  );
  t.is(bob.hintedCats(sim).length, 0);
});

test("revoke a cat and melt it", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const issuer = new Wallet(24n);
  const bob = new Wallet(25n);

  const { tailSpend, cat, id } = issueRevocableCat(sim, clvm, issuer, bob, 10n);

  issuer.addXch(sim, 0n);

  const spends = new Spends(clvm, issuer.puzzleHash);
  spends.addCatForRevocation(cat);
  for (const coin of issuer.fetchXch(sim)) {
    spends.addXch(coin);
  }

  issuer.finish(
    sim,
    clvm,
    spends,
    [Action.runTail(id, tailSpend, new Delta(0n, 10n))],
    Relation.AssertConcurrent,
  );

  t.is(issuer.balance(sim, Id.xch()), 10n);
  t.is(bob.hintedCats(sim).length, 0);
  t.is(issuer.hintedCats(sim).length, 0);
});

test("revoke a cat without actions returns it as revocable change", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const issuer = new Wallet(26n);
  const bob = new Wallet(27n);

  const { cat } = issueRevocableCat(sim, clvm, issuer, bob, 10n);

  const spends = new Spends(clvm, issuer.puzzleHash);
  spends.addCatForRevocation(cat);
  issuer.finish(sim, clvm, spends, []);

  const issuerCats = issuer.hintedCats(sim);
  t.is(issuerCats.length, 1);
  t.is(issuerCats[0].coin.amount, 10n);
  t.true(
    Buffer.from(issuerCats[0].info.p2PuzzleHash).equals(issuer.puzzleHash),
  );
  t.true(
    Buffer.from(issuerCats[0].info.hiddenPuzzleHash!).equals(issuer.puzzleHash),
  );
  t.is(bob.hintedCats(sim).length, 0);

  // A CAT without a revocation layer can't be revoked
  issuer.addXch(sim, 1n);
  const normal = issuer.spend(sim, clvm, [Action.singleIssueCat(null, 1n)]);
  const spends2 = new Spends(clvm, issuer.puzzleHash);
  t.throws(() => spends2.addCatForRevocation(normal.cat(normal.cats()[0])[0]));
});
