import test from "ava";

import { toCoinId } from "../index.js";

test("calculate coin id", (t) => {
  const coinId = toCoinId({
    parentCoinInfo: Buffer.from(
      "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
      "hex"
    ),
    puzzleHash: Buffer.from(
      "dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986",
      "hex"
    ),
    amount: 100n,
  });

  t.true(
    Buffer.from(coinId).equals(
      Buffer.from(
        "fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a",
        "hex"
      )
    )
  );
});
