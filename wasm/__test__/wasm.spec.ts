import test from "ava";

import {
  calculateVaultPuzzleMessage,
  Clvm,
  CreateCoin,
  fromHex,
  InnerPuzzleMemo,
  K1Pair,
  MemberConfig,
  MemberMemo,
  MemoKind,
  MipsMemo,
  MipsMemoContext,
  MofNMemo,
  PublicKey,
  r1MemberHash,
  R1Pair,
  RestrictionMemo,
  RunCatTail,
  setPanicHook,
  sha256,
  Signature,
  standardPuzzleHash,
  toHex,
  vaultMessageEnvelope,
  VaultMessageProof,
  verifyVaultSignedMessage,
} from "../pkg/chia_wallet_sdk_wasm.js";

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

test("mips memo round trip and parse", (t) => {
  const clvm = new Clvm();
  const keys = K1Pair.manyFromSeed(1n, 2).map((pair) => pair.pk);

  const memo = new MipsMemo(
    new InnerPuzzleMemo(
      0,
      [],
      MemoKind.mOfN(
        new MofNMemo(
          1,
          keys.map(
            (key, i) =>
              new InnerPuzzleMemo(
                0,
                i === 1 ? [RestrictionMemo.timelock(clvm, 3600n, true)] : [],
                MemoKind.member(MemberMemo.k1(clvm, key, true, true)),
              ),
          ),
        ),
      ),
    ),
  );

  const bytes = clvm.alloc([clvm.mipsMemo(memo)]).serialize();
  const parsed = clvm.deserialize(bytes).first().parseMipsMemo();

  t.not(parsed, undefined);
  t.is(toHex(parsed!.innerPuzzleHash()), toHex(memo.innerPuzzleHash()));
  t.is(toHex(clvm.alloc([clvm.mipsMemo(parsed!)]).serialize()), toHex(bytes));

  const ctx = new MipsMemoContext();
  const [first, second] = parsed!.innerPuzzle.kind.asMOfN()!.items;

  const member = first.kind.asMember()!.parse(ctx);
  t.is(toHex(member!.asK1()!.toBytes()), toHex(keys[0].toBytes()));
  t.is(member!.asR1(), undefined);
  t.true(member!.fastForward());

  const restriction = second.restrictions[0].parse(ctx);
  t.is(restriction?.asTimelock(), 3600n);

  t.is(clvm.alloc(["CHIP-0042", clvm.nil()]).parseMipsMemo(), undefined);});

test("vault message proofs can be created and verified without chain access", (t) => {
  const clvm = new Clvm();

  const r1 = R1Pair.fromSeed(1n);
  const config = new MemberConfig().withTopLevel(true);
  const custodyHash = r1MemberHash(config, r1.pk, true);

  const { vault } = clvm.mintVault(
    new Uint8Array(32).fill(1),
    custodyHash,
    clvm.nil(),
  );

  const messages = [new TextEncoder().encode("hello"), new Uint8Array([0xff])];
  const envelope = vaultMessageEnvelope(clvm, messages);

  const message = sha256(
    calculateVaultPuzzleMessage(
      envelope.puzzle.treeHash(),
      vault.coin.puzzleHash,
    ),
  );

  const mips = clvm.mipsSpend(vault.coin, envelope);
  mips.r1Member(config, r1.pk, r1.sk.signPrehashed(message), true);

  const proof = VaultMessageProof.decodeString(
    VaultMessageProof.fromCustodySpend(
      vault.info.launcherId,
      mips.spend(custodyHash),
    ).encodeString(),
  );

  t.deepEqual(
    verifyVaultSignedMessage(proof, vault.info.launcherId, custodyHash),
    messages,
  );
  t.throws(() =>
    verifyVaultSignedMessage(proof, new Uint8Array(32), custodyHash),
  );
});
