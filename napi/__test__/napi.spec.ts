import test from "ava";
import { Clvm, fromHex, PublicKey, RunCatTail, toHex, treeHashAtom } from "..";

test("ensure Buffer and Uint8Array are used properly", (t) => {
  const roundtrip = fromHex("ff").toString("hex");

  const hash = treeHashAtom(fromHex(roundtrip));
  t.is(
    hash.toString("hex"),
    "4b3a43f592f577fcfcb5b0e1f42bec5182c9edc414e1f667528f56e7cf0be11d"
  );

  const fromUint8Array = Uint8Array.from(fromHex(roundtrip));
  const hash2 = treeHashAtom(fromUint8Array);
  t.is(toHex(hash2), toHex(hash));
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
