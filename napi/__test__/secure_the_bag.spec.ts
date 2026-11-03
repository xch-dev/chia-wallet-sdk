import test from "ava";
import { readFileSync } from "fs";

import {
  BagAsset,
  BagCoin,
  BagFormat,
  BagPayment,
  Cat,
  CatInfo,
  CatSpend,
  ClawbackV2,
  Clvm,
  Coin,
  Constants,
  SecureTheBag,
  Simulator,
  sha256,
} from "../index.js";

interface Fixture {
  name: string;
  asset:
    | { type: "xch" }
    | { type: "cat"; asset_id: string }
    | { type: "revocable_cat"; asset_id: string; hidden_puzzle_hash: string };
  leaf_width: number | null;
  sorted: boolean;
  expected: { root_hash: string; total_amount: string; edges_digest: string };
  payments: {
    puzzle_hash: string;
    amount: string;
    clawback?: {
      sender_puzzle_hash: string;
      seconds: string;
      hinted: boolean;
    };
  }[];
}

const fromHex = (value: string) => Buffer.from(value, "hex");
const toHex = (value: Uint8Array) => Buffer.from(value).toString("hex");

const fixtures: Fixture[] = JSON.parse(
  readFileSync(
    new URL(
      "../../crates/chia-sdk-driver/tests/secure_the_bag/data/legacy.json",
      import.meta.url,
    ),
    "utf-8",
  ),
);

function bagAsset(asset: Fixture["asset"]) {
  switch (asset.type) {
    case "xch":
      return BagAsset.xch();
    case "cat":
      return BagAsset.cat(fromHex(asset.asset_id));
    case "revocable_cat":
      return BagAsset.revocableCat(
        fromHex(asset.asset_id),
        fromHex(asset.hidden_puzzle_hash),
      );
  }
}

function bagPayment(payment: Fixture["payments"][number]) {
  const puzzleHash = fromHex(payment.puzzle_hash);
  const amount = BigInt(payment.amount);

  if (!payment.clawback) {
    return new BagPayment(puzzleHash, amount);
  }

  return BagPayment.fromClawback(
    new ClawbackV2(
      fromHex(payment.clawback.sender_puzzle_hash),
      puzzleHash,
      BigInt(payment.clawback.seconds),
      amount,
      payment.clawback.hinted,
    ),
  );
}

// Converts a bag into the child lookup that Cloud Wallet stores: each parent's full puzzle hash maps to its
// children, along with the parent's serialized puzzle reveal and inner solution.
function childLookup(clvm: Clvm, bag: SecureTheBag) {
  const asset = bag.asset();
  const assetId = asset.assetId();
  const hiddenPuzzleHash = asset.hiddenPuzzleHash();

  const lookup: Record<
    string,
    {
      puzzleHash: string;
      amount: bigint;
      parentPuzzleReveal: Buffer;
      parentInnerSolution: Buffer;
    }[]
  > = {};

  for (const puzzleHash of bag.nodePuzzleHashes()) {
    const node = bag.node(puzzleHash)!;
    const spend = bag.nodeSpend(clvm, puzzleHash);

    let puzzle = spend.puzzle;
    let innerSolution = spend.solution;

    if (hiddenPuzzleHash) {
      puzzle = clvm
        .revocationLayer()
        .curry([
          clvm.atom(Constants.revocationLayerHash()),
          clvm.atom(hiddenPuzzleHash),
          clvm.atom(puzzleHash),
        ]);
      innerSolution = clvm.alloc([0, spend.puzzle, spend.solution]);
    }

    if (assetId) {
      puzzle = clvm
        .catPuzzle()
        .curry([
          clvm.atom(Constants.catPuzzleHash()),
          clvm.atom(assetId),
          puzzle,
        ]);
    }

    lookup[toHex(asset.puzzleHash(puzzleHash))] = node.children.map(
      (child) => ({
        puzzleHash: toHex(asset.puzzleHash(child.puzzleHash())),
        amount: child.amount(),
        parentPuzzleReveal: puzzle.serialize(),
        parentInnerSolution: innerSolution.serialize(),
      }),
    );
  }

  return lookup;
}

// The digest of the sorted edges, in the same format as the golden vectors.
function edgesDigest(lookup: ReturnType<typeof childLookup>) {
  const edges = Object.entries(lookup).flatMap(([parent, children]) =>
    children.map(
      (child) =>
        `${parent} ${child.puzzleHash} ${child.amount} ${toHex(
          sha256(child.parentPuzzleReveal),
        )} ${toHex(sha256(child.parentInnerSolution))}\n`,
    ),
  );
  edges.sort();
  return toHex(sha256(Buffer.from(edges.join(""), "utf-8")));
}

test("matches the golden vectors through the bindings", (t) => {
  for (const fixture of fixtures) {
    const clvm = new Clvm();
    const bag = new SecureTheBag(
      BagFormat.legacy(
        fixture.leaf_width === null ? null : BigInt(fixture.leaf_width),
        fixture.sorted,
      ),
      bagAsset(fixture.asset),
      fixture.payments.map(bagPayment),
    );

    t.is(toHex(bag.rootHash()), fixture.expected.root_hash, fixture.name);
    t.is(
      bag.totalAmount(),
      BigInt(fixture.expected.total_amount),
      fixture.name,
    );
    t.is(
      edgesDigest(childLookup(clvm, bag)),
      fixture.expected.edges_digest,
      fixture.name,
    );
  }
});

test("unrolls an xch bag with a clawback", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const payments = [1n, 2n, 3n, 4n, 5n].map(
    (amount) => new BagPayment(sha256(Buffer.from([Number(amount)])), amount),
  );
  const clawback = new ClawbackV2(
    sha256(Buffer.from("sender")),
    sha256(Buffer.from("receiver")),
    100n,
    10n,
    true,
  );
  payments.push(BagPayment.fromClawback(clawback));

  const bag = new SecureTheBag(
    BagFormat.legacy(2n, true),
    BagAsset.xch(),
    payments,
  );

  t.is(bag.totalAmount(), 25n);
  t.is(bag.depth(), 3n);
  t.is(bag.format().toLegacy()?.leafWidth, 2n);
  t.deepEqual(
    bag.nodePuzzleHashes().at(-1),
    bag.rootHash(),
    "the root is built last",
  );

  const alice = sim.bls(25n);
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(bag.rootHash(), 25n, clvm.alloc([bag.rootHash()])),
    ]),
  );

  const root = BagCoin.xch(new Coin(alice.coin.coinId(), bag.rootHash(), 25n));

  const coinPuzzleHash = payments[5].coinPuzzleHash();
  t.deepEqual(coinPuzzleHash, clawback.puzzleHash());
  t.is(bag.path(coinPuzzleHash)?.length, 3);
  t.is(bag.coinPath(root, coinPuzzleHash).length, 4);
  t.is(bag.amount(coinPuzzleHash), 10n);

  // Coins that aren't part of the bag are rejected
  t.throws(() =>
    bag.coinPath(
      BagCoin.xch(new Coin(alice.coin.coinId(), bag.rootHash(), 24n)),
      coinPuzzleHash,
    ),
  );
  t.throws(() => bag.unroll(clvm, BagCoin.xch(alice.coin)));

  const coins = bag.unroll(clvm, root);
  t.is(coins.length, 6);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  for (const payment of bag.payments()) {
    const coin = coins.find(
      (coin) => toHex(coin.p2PuzzleHash()) === toHex(payment.coinPuzzleHash()),
    )!;
    t.is(coin.coin().amount, payment.amount);
    t.truthy(sim.coinState(coin.coin().coinId()));
    t.true(
      sim
        .hintedCoins(payment.puzzleHash)
        .some((coinId) => toHex(coinId) === toHex(coin.coin().coinId())),
    );
  }
});

test("spends the path to one payment of a revocable cat bag", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1000n);
  const tail = clvm.nil();
  const assetId = tail.treeHash();

  // NFT bulk mints keep the order of the payments
  const payments = Array.from(
    { length: 10 },
    (_, i) => new BagPayment(sha256(Buffer.from([i])), 100n),
  );
  const asset = BagAsset.revocableCat(assetId, alice.puzzleHash);
  const bag = new SecureTheBag(BagFormat.legacy(3n, false), asset, payments);

  t.deepEqual(
    bag.payments().map((payment) => toHex(payment.puzzleHash)),
    payments.map((payment) => toHex(payment.puzzleHash)),
  );

  // Issue the CAT straight to the root of the bag
  const eveInfo = new CatInfo(assetId, alice.puzzleHash, alice.puzzleHash);
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([clvm.createCoin(eveInfo.puzzleHash(), 1000n)]),
  );

  const eve = new Cat(
    new Coin(alice.coin.coinId(), eveInfo.puzzleHash(), 1000n),
    null,
    eveInfo,
  );

  clvm.spendCats([
    new CatSpend(
      eve,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([
          clvm.createCoin(bag.rootHash(), 1000n, clvm.alloc([bag.rootHash()])),
          clvm.runCatTail(tail, clvm.nil()),
        ]),
      ),
    ),
  ]);

  const root = BagCoin.cat(eve.child(bag.rootHash(), 1000n));
  t.deepEqual(root.coin().puzzleHash, bag.rootPuzzleHash());

  const target = payments[7].puzzleHash;
  const coin = bag.spendPath(clvm, root, target);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.deepEqual(coin.p2PuzzleHash(), target);
  t.deepEqual(coin.toCat()?.info.hiddenPuzzleHash, alice.puzzleHash);
  t.truthy(sim.coinState(coin.coin().coinId()));
  t.is(sim.hintedCoins(target).length, 1);

  // Payments in other leaves haven't been created yet
  t.is(sim.hintedCoins(payments[0].puzzleHash).length, 0);
});

test("rejects invalid bags", (t) => {
  const payment = new BagPayment(sha256(Buffer.from("a")), 1n);

  t.throws(
    () => new SecureTheBag(BagFormat.legacy(null, true), BagAsset.xch(), []),
  );
  t.throws(
    () =>
      new SecureTheBag(BagFormat.legacy(null, true), BagAsset.xch(), [
        payment,
        payment,
      ]),
  );
  t.throws(
    () =>
      new SecureTheBag(BagFormat.legacy(null, true), BagAsset.xch(), [
        new BagPayment(sha256(Buffer.from("a")), 0xffffffffffffffffn),
        new BagPayment(sha256(Buffer.from("b")), 1n),
      ]),
  );
});
