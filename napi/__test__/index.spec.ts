import test from "ava";

import {
  ClvmAllocator,
  ClvmPtr,
  compareBytes,
  fromHex,
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
  const atom = clvm.newAtom(expected);

  t.true(compareBytes(clvm.atom(atom), expected));
});

test("string roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const expected = "hello world";
  const atom = clvm.newString(expected);
  t.is(clvm.string(atom), expected);
});

test("small number roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  for (const expected of [0, 1, 420, 67108863]) {
    const num = clvm.newSmallNumber(expected);
    t.is(clvm.smallNumber(num), expected);
  }
});

test("small number overflow", (t) => {
  const clvm = new ClvmAllocator();

  for (const expected of [67108864, 2 ** 32 - 1, Number.MAX_SAFE_INTEGER]) {
    t.throws(() => clvm.newSmallNumber(expected));
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
    const num = clvm.newBigInt(expected);
    t.is(clvm.bigInt(num), expected);
  }
});

test("pair roundtrip", (t) => {
  const clvm = new ClvmAllocator();
  const a = clvm.newSmallNumber(1);
  const b = clvm.newBigInt(100n);

  const ptr = clvm.newPair(a, b);
  const { first, rest } = clvm.pair(ptr);

  t.is(clvm.smallNumber(first), 1);
  t.is(clvm.bigInt(rest), 100n);
});

test("list roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.newList(items.map((i) => clvm.newSmallNumber(i)));
  const list = clvm.list(ptr).map((ptr) => clvm.smallNumber(ptr));

  t.deepEqual(list, items);
});

test("curry add function", (t) => {
  const clvm = new ClvmAllocator();

  const addMod = clvm.deserialize(fromHex("ff10ff02ff0580"));
  const addToTen = clvm.curry(addMod, [clvm.newSmallNumber(10)]);
  const result = clvm.run(
    addToTen,
    clvm.newList([clvm.newSmallNumber(5)]),
    10000000n,
    true
  );

  t.is(clvm.smallNumber(result.value), 15);
  t.is(result.cost, 1082n);
});

test("curry roundtrip", (t) => {
  const clvm = new ClvmAllocator();

  const items = Array.from({ length: 10 }, (_, i) => i);
  const ptr = clvm.curry(
    ClvmPtr.nil(),
    items.map((i) => clvm.newSmallNumber(i))
  );
  const uncurry = clvm.uncurry(ptr);
  const args = uncurry.args.map((ptr) => clvm.smallNumber(ptr));

  t.true(
    compareBytes(clvm.treeHash(ClvmPtr.nil()), clvm.treeHash(uncurry.program))
  );
  t.deepEqual(args, items);
});

test("clvm serialization", (t) => {
  const clvm = new ClvmAllocator();

  for (const [ptr, hex] of [
    [clvm.newAtom(Uint8Array.from([1, 2, 3])), "83010203"],
    [clvm.newSmallNumber(420), "8201a4"],
    [clvm.newBigInt(100n), "64"],
    [
      clvm.newPair(
        clvm.newAtom(Uint8Array.from([1, 2, 3])),
        clvm.newBigInt(100n)
      ),
      "ff8301020364",
    ],
  ] as const) {
    const serialized = clvm.serialize(ptr);
    const deserialized = clvm.deserialize(serialized);

    t.true(compareBytes(clvm.treeHash(ptr), clvm.treeHash(deserialized)));
    t.is(hex as string, toHex(serialized));
  }
});
