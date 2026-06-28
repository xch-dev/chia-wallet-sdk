import test from "ava";
import {
  Action,
  BlsPair,
  catPuzzleHash,
  Clvm,
  Coin,
  Constants,
  Delta,
  Deltas,
  Id,
  Nft,
  NftMetadata,
  Outputs,
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
        0n
      );
    } else {
      return 0n;
    }
  }

  selectCoins(
    sim: Simulator,
    spends: Spends,
    actions: Action[],
    reservedNfts: Map<string, Nft>
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
    extras?: { nfts?: Nft[] }
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
    null
  );

  const mint = Action.mintNft(
    clvm,
    clvm.nftMetadata(metadata),
    Constants.nftMetadataUpdaterDefaultHash(),
    alice.puzzleHash,
    0,
    1n,
    null
  );

  const metadataUpdate = new Spend(
    clvm.nftMetadataUpdaterDefault(),
    clvm.list([clvm.string("u"), clvm.string("https://example.com/2")])
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
    null
  );

  const mint = Action.mintNft(
    clvm,
    clvm.nftMetadata(metadata),
    Constants.nftMetadataUpdaterDefaultHash(),
    alice.puzzleHash,
    0,
    1n,
    null
  );

  const mintOutputs = alice.spend(sim, clvm, [mint]);
  const mintedId = mintOutputs.nfts()[0];
  const mintedNft = mintOutputs.nft(mintedId);

  // Update the metadata using the existing NFT
  const metadataUpdate = new Spend(
    clvm.nftMetadataUpdaterDefault(),
    clvm.list([clvm.string("u"), clvm.string("https://example.com/2")])
  );

  const update = Action.updateNft(Id.existing(mintedNft.info.launcherId), [metadataUpdate]);

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
