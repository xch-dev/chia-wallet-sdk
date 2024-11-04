import test from "ava";

import {
  ClvmAllocator,
  compareBytes,
  curryTreeHash,
  fromHex,
  Simulator,
  toCoinId,
  toHex,
} from "../index.js";

test("calculate coin id", (t) => {
  const coinId = toCoinId({
    parentCoinInfo: fromHex(
      "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"
    ),
    puzzleHash: fromHex(
      "dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986"
    ),
    amount: 100n,
  });

  t.true(
    compareBytes(
      coinId,
      fromHex(
        "fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a"
      )
    )
  );
});

test("byte equality", (t) => {
  const a = Uint8Array.from([1, 2, 3]);
  const b = Uint8Array.from([1, 2, 3]);

  t.true(compareBytes(a, b));
  t.true(Buffer.from(a).equals(b));
});

test("byte inequality", (t) => {
  const a = Uint8Array.from([1, 2, 3]);
  const b = Uint8Array.from([1, 2, 4]);

  t.true(!compareBytes(a, b));
  t.true(!Buffer.from(a).equals(b));
});

test("atom roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const expected = Uint8Array.from([1, 2, 3]);
  const atom = clvm.alloc(expected);

  t.true(compareBytes(atom.toAtom()!, expected));
});

test("string roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const expected = "hello world";
  const atom = clvm.alloc(expected);
  t.is(atom.toString(), expected);
});

test("number roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  for (const expected of [
    Number.MIN_SAFE_INTEGER,
    -1000,
    0,
    34,
    1000,
    Number.MAX_SAFE_INTEGER,
  ]) {
    const num = clvm.alloc(expected);
    t.is(num.toBigInt(), BigInt(expected));
  }
});

test("invalid number", (t) => {
  const clvm = new ClvmAllocator();

  for (const expected of [
    Number.MIN_SAFE_INTEGER - 1,
    Number.MAX_SAFE_INTEGER + 1,
    Infinity,
    -Infinity,
    NaN,
  ]) {
    t.throws(() => clvm.alloc(expected));
  }
});

test("bigint roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  for (const expected of [
    0n,
    1n,
    420n,
    67108863n,
    -1n,
    -100n,
    -421489719874198729487129847n,
    4384723984791283749823764732649187498237483927482n,
  ]) {
    const num = clvm.alloc(expected);
    t.is(num.toBigInt(), expected);
  }
});

test("pair roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const ptr = clvm.pair(1, 100n);
  const [first, rest] = ptr.toPair()!;

  t.is(first.toSmallNumber(), 1);
  t.is(rest.toBigInt(), 100n);
});

test("list roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.alloc(items);
  const list = ptr.toList().map((ptr) => ptr.toSmallNumber());

  t.deepEqual(list, items);
});

test("clvm value allocation", (t) => {
  const clvm = new ClvmAllocator();

  const shared = clvm.alloc(42);

  const manual = clvm.alloc([
    clvm.alloc(42),
    clvm.alloc("Hello, world!"),
    clvm.alloc(true),
    clvm.alloc(Uint8Array.from([1, 2, 3])),
    clvm.alloc([clvm.alloc(34)]),
    clvm.alloc(100n),
    shared,
  ]);

  const auto = clvm.alloc([
    42,
    "Hello, world!",
    true,
    Uint8Array.from([1, 2, 3]),
    [34],
    100n,
    shared,
  ]);

  t.true(compareBytes(clvm.treeHash(manual), clvm.treeHash(auto)));
});

test("curry add function", (t) => {
  const clvm = new ClvmAllocator();

  const addMod = clvm.deserialize(fromHex("ff10ff02ff0580"));
  const addToTen = clvm.curry(addMod, [clvm.alloc(10)]);
  const result = clvm.run(addToTen, clvm.alloc([5]), 10000000n, true);

  t.is(result.value.toSmallNumber(), 15);
  t.is(result.cost, 1082n);
});

test("curry roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.curry(
    clvm.nil(),
    items.map((i) => clvm.alloc(i))
  );
  const uncurry = ptr.uncurry()!;
  const args = uncurry.args.map((ptr) => ptr.toSmallNumber());

  t.true(
    compareBytes(clvm.treeHash(clvm.nil()), clvm.treeHash(uncurry.program))
  );
  t.deepEqual(args, items);
});

test("clvm serialization", (t) => {
  const clvm = new ClvmAllocator();

  for (const [ptr, hex] of [
    [clvm.alloc(Uint8Array.from([1, 2, 3])), "83010203"],
    [clvm.alloc(420), "8201a4"],
    [clvm.alloc(100n), "64"],
    [clvm.pair(Uint8Array.from([1, 2, 3]), 100n), "ff8301020364"],
  ] as const) {
    const serialized = ptr.serialize();
    const deserialized = clvm.deserialize(serialized);

    t.true(compareBytes(clvm.treeHash(ptr), clvm.treeHash(deserialized)));
    t.is(hex as string, toHex(serialized));
  }
});

test("curry tree hash", (t) => {
  const clvm = new ClvmAllocator();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.curry(
    clvm.nil(),
    items.map((i) => clvm.alloc(i))
  );

  const treeHash = curryTreeHash(
    clvm.treeHash(clvm.nil()),
    items.map((i) => clvm.treeHash(clvm.alloc(i)))
  );
  const expected = clvm.treeHash(ptr);

  t.true(compareBytes(treeHash, expected));
});

test("mint and spend nft", (t) => {
  const clvm = new ClvmAllocator();
  const simulator = new Simulator();
  const p2 = simulator.newP2(1n);

  const result = clvm.mintNfts(toCoinId(p2.coin), [
    {
      metadata: {
        dataUris: ["https://example.com"],
        metadataUris: ["https://example.com"],
        licenseUris: ["https://example.com"],
        editionNumber: 1n,
        editionTotal: 1n,
      },
      p2PuzzleHash: p2.puzzleHash,
      royaltyPuzzleHash: p2.puzzleHash,
      royaltyTenThousandths: 300,
    },
  ]);

  const spend = clvm.spendP2Standard(
    p2.publicKey,
    clvm.delegatedSpendForConditions(result.parentConditions)
  );

  simulator.spend(
    result.coinSpends.concat([
      {
        coin: p2.coin,
        puzzleReveal: spend.puzzle.serialize(),
        solution: spend.solution.serialize(),
      },
    ]),
    [p2.secretKey]
  );

  const innerSpend = clvm.spendP2Standard(
    p2.publicKey,
    clvm.delegatedSpendForConditions([
      clvm.createCoin(p2.puzzleHash, 1n, [p2.puzzleHash]),
    ])
  );

  const coinSpends = clvm.spendNft(result.nfts[0], innerSpend);

  simulator.spend(coinSpends, [p2.secretKey]);

  t.true(
    compareBytes(
      clvm
        .nftMetadata(
          clvm.parseNftMetadata(clvm.deserialize(result.nfts[0].info.metadata))
        )
        .serialize(),
      result.nfts[0].info.metadata
    )
  );
});

test("create and parse condition", (t) => {
  const clvm = new ClvmAllocator();

  const puzzleHash = fromHex("ff".repeat(32));

  const condition = clvm.createCoin(puzzleHash, 1n, [puzzleHash]);
  const parsed = clvm.parseCreateCoin(condition);

  t.deepEqual(parsed, {
    puzzleHash,
    amount: 1n,
    memos: [puzzleHash],
  });
});
