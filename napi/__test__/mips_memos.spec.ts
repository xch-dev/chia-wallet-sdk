import test from "ava";

import {
  Clvm,
  fromHex,
  InnerPuzzleMemo,
  K1SecretKey,
  MemberMemo,
  MemoKind,
  MipsMemo,
  MofNMemo,
  PublicKey,
  RestrictionMemo,
  WrapperMemo,
} from "../index.js";

test("construct mips memos", (t) => {
  const clvm = new Clvm();

  const memo = clvm.alloc(
    new MipsMemo(
      new InnerPuzzleMemo(
        0,
        [
          RestrictionMemo.timelock(clvm, 100n, true),
          RestrictionMemo.enforceDelegatedPuzzleWrappers(
            clvm,
            WrapperMemo.preventVaultSideEffects(clvm, true)
          ),
        ],
        MemoKind.mOfN(
          new MofNMemo(1, [
            new InnerPuzzleMemo(
              1,
              [],
              MemoKind.member(
                MemberMemo.bls(clvm, PublicKey.infinity(), false, true)
              )
            ),
            new InnerPuzzleMemo(
              1000,
              [],
              MemoKind.member(
                MemberMemo.k1(
                  clvm,
                  K1SecretKey.fromBytes(fromHex("11".repeat(32))).publicKey(),
                  false,
                  false
                )
              )
            ),
          ])
        )
      )
    )
  );

  t.is(
    memo.unparse(),
    '("CHIP-0043" (() ((q 0x9021fd9782d1c031ced7384dadaf8713492ab7a9b27e7ad7ee8a7559345d5ff6 100) (() 0xb73b1456ec5f1480c1dc9aa424b990a452025cbc62698d72b121c28d86a24f0d (modpow 62 66 67 ()))) 1 (q ((q () () (0x25025743040eb76267cc345e7b0f301a7a93c0c7a317b91137c492ad33ca9675 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000)) (1000 () () (0x6ba4a84b37c1bda23dcafee28d5f7b3ce23c71f3ccd64199f19a381277627b6d ()))))))'
  );
});
