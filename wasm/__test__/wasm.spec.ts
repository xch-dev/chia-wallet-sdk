import test from "ava";

import {
  Clvm,
  CreateCoin,
  fromHex,
  PublicKey,
  RunCatTail,
  setPanicHook,
  Signature,
  standardPuzzleHash,
  toHex,
} from "../pkg";

setPanicHook();

test("Buffer and Uint8Array can be used as arguments", (t) => {
  t.is(toHex(Buffer.from("00", "hex")), toHex(new Uint8Array([0])));
});

test("functions return Uint8Array rather than Buffer", (t) => {
  const array = fromHex("00");

  t.assert(!(array instanceof Buffer));
  t.assert(array instanceof Uint8Array);
});

test("none is represented with undefined", (t) => {
  const condition = new CreateCoin(
    Buffer.from("00".repeat(32), "hex"),
    100n,
    undefined
  );

  t.is(condition.memos, undefined);
});

test("values are taken by reference", (t) => {
  const publicKey = PublicKey.infinity();

  const puzzleHash1 = toHex(standardPuzzleHash(publicKey));
  const puzzleHash2 = toHex(standardPuzzleHash(publicKey));

  t.is(puzzleHash1, puzzleHash2);
});

test("options are taken by reference", (t) => {
  const clvm = new Clvm();

  const puzzleHash = Buffer.from("00".repeat(32), "hex");
  const program = clvm.list([clvm.string("a"), clvm.string("b")]);

  const createCoin1 = clvm.createCoin(puzzleHash, 1n, program);
  const createCoin2 = clvm.createCoin(puzzleHash, 1n, program);
  clvm.createCoin(puzzleHash, 1n); // Options are optional

  t.is(toHex(createCoin1.serialize()), toHex(createCoin2.serialize()));

  t.is(
    toHex(
      createCoin1.parseCreateCoin()?.memos?.serialize() ?? new Uint8Array()
    ),
    "ff61ff6280"
  );
});

test("arrays are taken by reference", (t) => {
  const sig = Signature.infinity();

  const aggregateOne = Signature.aggregate([sig]);
  const aggregateTwo = Signature.aggregate([sig, sig]);

  t.is(toHex(aggregateOne.toBytes()), toHex(aggregateTwo.toBytes()));
});

test("alloc", (t) => {
  const clvm = new Clvm();

  const program = clvm.alloc([
    clvm.nil(),
    PublicKey.infinity(),
    "Hello, world!",
    42n,
    100,
    true,
    new Uint8Array([1, 2, 3]),
    Buffer.from("00".repeat(32), "hex"),
    null,
    undefined,
    new RunCatTail(clvm.nil(), clvm.nil()),
  ]);

  t.is(
    toHex(program.serialize()),
    "ff80ffb0c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff818fff80ff808080"
  );
});
