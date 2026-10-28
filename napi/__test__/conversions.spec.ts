import test from "ava";

import { Clvm, Coin, SecretKey, toHex } from "../index.js";

test("bigint atoms have the correct bytes", (t) => {
  const clvm = new Clvm();

  for (const [value, hex] of [
    [0n, ""],
    [1n, "01"],
    [-1n, "ff"],
    [0xffn, "00ff"],
    [2n ** 63n, "008000000000000000"],
    [2n ** 64n - 1n, "00ffffffffffffffff"],
    [2n ** 64n, "010000000000000000"],
    [2n ** 64n + 2n, "010000000000000002"],
    [-(2n ** 64n), "ff0000000000000000"],
    [2n ** 128n + 1n, "0100000000000000000000000000000001"],
  ] as const) {
    t.is(toHex(clvm.int(value).toAtom()!), hex, `value ${value}`);
    t.is(toHex(clvm.alloc(value).toAtom()!), hex, `value ${value}`);
    t.is(clvm.int(value).toInt(), value);
  }
});

test("u64 amounts above 2^32 roundtrip", (t) => {
  const zero = new Uint8Array(32);

  for (const amount of [
    2n ** 32n,
    2n ** 63n + 5n,
    2n ** 64n - 1n,
    0x0123456789abcdefn,
  ]) {
    t.is(new Coin(zero, zero, amount).amount, amount);
  }
});

test("out of range u64 amounts are rejected", (t) => {
  const zero = new Uint8Array(32);

  t.throws(() => new Coin(zero, zero, 2n ** 64n));
  t.throws(() => new Coin(zero, zero, -1n));
});

test("out of range u32 numbers are rejected", (t) => {
  const sk = SecretKey.fromSeed(new Uint8Array(32));

  t.notDeepEqual(
    sk.deriveUnhardened(1).toBytes(),
    sk.deriveUnhardened(0).toBytes(),
  );
  t.deepEqual(
    sk.deriveUnhardened(2 ** 32 - 1).toBytes(),
    sk.deriveUnhardened(2 ** 32 - 1).toBytes(),
  );

  for (const index of [2 ** 32, -1, 1.5, NaN, Infinity]) {
    t.throws(() => sk.deriveUnhardened(index), undefined, `index ${index}`);
  }

  t.throws(() => sk.deriveUnhardenedPath([0, 2 ** 32]));
});

test("deeply nested arrays are rejected", (t) => {
  const clvm = new Clvm();

  let shallow: unknown = [];
  for (let i = 0; i < 100; i++) shallow = [shallow];
  t.notThrows(() => clvm.alloc(shallow));

  let deep: unknown = [];
  for (let i = 0; i < 100_000; i++) deep = [deep];
  t.throws(() => clvm.alloc(deep));

  const cyclic: unknown[] = [];
  cyclic.push(cyclic);
  t.throws(() => clvm.alloc(cyclic));
});
