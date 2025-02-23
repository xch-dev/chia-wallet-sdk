import test from "ava";

import {
  bytesEqual,
  Clvm,
  curryTreeHash,
  fromHex,
  PublicKey,
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
    bytesEqual(
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

  t.true(bytesEqual(a, b));
  t.true(Buffer.from(a).equals(b));
});

test("byte inequality", (t) => {
  const a = Uint8Array.from([1, 2, 3]);
  const b = Uint8Array.from([1, 2, 4]);

  t.true(!bytesEqual(a, b));
  t.true(!Buffer.from(a).equals(b));
});

test("atom roundtrip", (t) => {
  const clvm = new Clvm();

  const expected = Uint8Array.from([1, 2, 3]);
  const atom = clvm.alloc(expected);

  t.true(bytesEqual(atom.toBytes()!, expected));
});

test("string roundtrip", (t) => {
  const clvm = new Clvm();

  const expected = "hello world";
  const atom = clvm.alloc(expected);
  t.is(atom.toString(), expected);
});

test("number roundtrip", (t) => {
  const clvm = new Clvm();

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
  const clvm = new Clvm();

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
  const clvm = new Clvm();

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
  const clvm = new Clvm();

  const ptr = clvm.pair(1, 100n);
  const { first, rest } = ptr.toPair()!;

  t.is(first.toNumber(), 1);
  t.is(rest.toBigInt(), 100n);
});

test("list roundtrip", (t) => {
  const clvm = new Clvm();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.alloc(items);
  const list = ptr.toList()?.map((ptr) => ptr.toNumber());

  t.deepEqual(list, items);
});

test("clvm value allocation", (t) => {
  const clvm = new Clvm();

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

  t.true(bytesEqual(manual.treeHash(), auto.treeHash()));
});

test("public key roundtrip", (t) => {
  const clvm = new Clvm();

  const ptr = clvm.alloc(PublicKey.infinity());
  const pk = PublicKey.fromBytes(ptr.toBytes()!);

  t.true(bytesEqual(PublicKey.infinity().toBytes(), pk.toBytes()));
});

test("curry add function", (t) => {
  const clvm = new Clvm();

  const addMod = clvm.deserialize(fromHex("ff10ff02ff0580"));
  const addToTen = addMod.curry([clvm.alloc(10)]);
  const result = addToTen.run(clvm.alloc([5]), 10000000n, true);

  t.is(result.value.toNumber(), 15);
  t.is(result.cost, 1082n);
});

test("curry roundtrip", (t) => {
  const clvm = new Clvm();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.nil().curry(items.map((i) => clvm.alloc(i)));
  const uncurry = ptr.uncurry()!;
  const args = uncurry.args?.map((ptr) => ptr.toNumber());

  t.true(bytesEqual(clvm.nil().treeHash(), uncurry.program.treeHash()));
  t.deepEqual(args, items);
});

test("clvm serialization", (t) => {
  const clvm = new Clvm();

  for (const [ptr, hex] of [
    [clvm.alloc(Uint8Array.from([1, 2, 3])), "83010203"],
    [clvm.alloc(420), "8201a4"],
    [clvm.alloc(100n), "64"],
    [clvm.pair(Uint8Array.from([1, 2, 3]), 100n), "ff8301020364"],
  ] as const) {
    const serialized = ptr.serialize();
    const deserialized = clvm.deserialize(serialized);

    t.true(bytesEqual(ptr.treeHash(), deserialized.treeHash()));
    t.is(hex as string, toHex(serialized));
  }
});

test("curry tree hash", (t) => {
  const clvm = new Clvm();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.nil().curry(items.map((i) => clvm.alloc(i)));

  const treeHash = curryTreeHash(
    clvm.nil().treeHash(),
    items.map((i) => clvm.alloc(i).treeHash())
  );
  const expected = ptr.treeHash();

  t.true(bytesEqual(treeHash, expected));
});

test("mint and spend nft", (t) => {
  const clvm = new Clvm();
  const simulator = new Simulator();
  const alice = simulator.bls(1n);

  const result = clvm.mintNfts(toCoinId(alice.coin), [
    {
      metadata: {
        dataUris: ["https://example.com"],
        metadataUris: ["https://example.com"],
        licenseUris: ["https://example.com"],
        editionNumber: 1n,
        editionTotal: 1n,
      },
      p2PuzzleHash: alice.puzzleHash,
      royaltyPuzzleHash: alice.puzzleHash,
      royaltyTenThousandths: 300,
    },
  ]);

  const spend = clvm.standardSpend(
    alice.pk,
    clvm.delegatedSpend(result.parentConditions)
  );

  simulator.spendCoins(
    clvm.coinSpends().concat([
      {
        coin: alice.coin,
        puzzleReveal: spend.puzzle.serialize(),
        solution: spend.solution.serialize(),
      },
    ]),
    [alice.sk]
  );

  const innerSpend = clvm.standardSpend(
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
    ])
  );

  clvm.spendNft(result.nfts[0], innerSpend);

  simulator.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(
    bytesEqual(
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
  const clvm = new Clvm();

  const puzzleHash = fromHex("ff".repeat(32));

  const condition = clvm.createCoin(puzzleHash, 1n, clvm.alloc([puzzleHash]));
  const parsed = condition.parseCreateCoin();

  t.true(parsed !== null && bytesEqual(parsed.puzzleHash, puzzleHash));
  t.true(parsed !== null && parsed.amount === 1n);

  t.deepEqual(
    parsed?.memos
      ?.toList()
      ?.map((memo) => memo.toBytes())
      .filter((memo) => memo !== null),
    [puzzleHash]
  );
});
