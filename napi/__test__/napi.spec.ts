import test from "ava";
import { fromHex, toHex, treeHashAtom } from "..";

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
