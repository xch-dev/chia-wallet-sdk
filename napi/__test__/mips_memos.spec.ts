import test from "ava";

import {
  blsMemberHash,
  BlsPair,
  Clvm,
  customMemberHash,
  fixedMemberHash,
  force1Of2Restriction,
  forceSingletonRecreationRestriction,
  fromHex,
  InnerPuzzleMemo,
  K1Pair,
  k1MemberHash,
  K1SecretKey,
  MemberConfig,
  MemberMemo,
  MemoKind,
  MipsMemo,
  MipsMemoContext,
  MofNMemo,
  mOfNHash,
  ParsedMember,
  ParsedRestriction,
  ParsedWrapper,
  passkeyMemberHash,
  preventVaultSideEffectsRestriction,
  Program,
  PublicKey,
  R1Pair,
  r1MemberHash,
  RestrictionMemo,
  singletonMemberHash,
  timelockRestriction,
  toHex,
  treeHashPair,
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
            WrapperMemo.preventVaultSideEffects(clvm, true),
          ),
        ],
        MemoKind.mOfN(
          new MofNMemo(1, [
            new InnerPuzzleMemo(
              1,
              [],
              MemoKind.member(
                MemberMemo.bls(clvm, PublicKey.infinity(), false, false, true),
              ),
            ),
            new InnerPuzzleMemo(
              1000,
              [],
              MemoKind.member(
                MemberMemo.k1(
                  clvm,
                  K1SecretKey.fromBytes(fromHex("11".repeat(32))).publicKey(),
                  false,
                  false,
                ),
              ),
            ),
          ]),
        ),
      ),
    ),
  );

  t.is(
    memo.unparse(),
    '("CHIP-0043" (() ((q 0x9021fd9782d1c031ced7384dadaf8713492ab7a9b27e7ad7ee8a7559345d5ff6 100) (() 0xb73b1456ec5f1480c1dc9aa424b990a452025cbc62698d72b121c28d86a24f0d ((0xd11bff1c654ae6d4961aa15172ec6d8abe069daa633fad16d8661170e05ca7ff 60) (0x919553a3fea025ed5183e94596b195a5fed91936161bf4b74d0adda09519bfab 62) (0x74e623158c08720e8fc8ac52bbc38b9f33191f0dbb5387a1c441a9196f28404a 66) (0x9b2304782b50c6392f2720e1d60f6713c925bf61564c9ba4458ad81cd71ef1c2 67) (0x93b8c8abeab8f6bdba4acb49ed49362ecba94b703a48b15c8784f966547b7846 ())))) 1 (q ((q () () (0x25025743040eb76267cc345e7b0f301a7a93c0c7a317b91137c492ad33ca9675 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000)) (1000 () () (0x6ba4a84b37c1bda23dcafee28d5f7b3ce23c71f3ccd64199f19a381277627b6d ()))))))',
  );
});

function roundTrip(clvm: Clvm, memo: MipsMemo): MipsMemo {
  const bytes = clvm.mipsMemo(memo).serialize();
  const parsed = clvm.deserialize(bytes).parseMipsMemo();
  if (parsed === null) {
    throw new Error("Expected memo to parse");
  }
  if (toHex(clvm.mipsMemo(parsed).serialize()) !== toHex(bytes)) {
    throw new Error("Expected memo to re-encode identically");
  }
  return parsed;
}

function memberMemo(member: MemberMemo): MipsMemo {
  return new MipsMemo(new InnerPuzzleMemo(0, [], MemoKind.member(member)));
}

function parseMember(memo: MipsMemo, ctx = new MipsMemoContext()) {
  return memo.innerPuzzle.kind.asMember()!.parse(ctx);
}

function memberName(member: ParsedMember): string {
  if (member.asK1() !== null) return "k1";
  if (member.asR1() !== null) return "r1";
  if (member.asBls() !== null) return "bls";
  if (member.asBlsTaproot() !== null) return "blsTaproot";
  if (member.asPasskey() !== null) return "passkey";
  if (member.asSingleton() !== null) return "singleton";
  if (member.asFixedPuzzle() !== null) return "fixedPuzzle";
  if (member.asCustom() !== null) return "custom";
  throw new Error("Unknown parsed member");
}

function restrictionName(restriction: ParsedRestriction): string {
  if (restriction.asForce1Of2RestrictedVariable() !== null) return "force1Of2";
  if (restriction.asEnforceDelegatedPuzzleWrappers() !== null) return "wrappers";
  if (restriction.asTimelock() !== null) return "timelock";
  throw new Error("Unknown parsed restriction");
}

function wrapperName(wrapper: ParsedWrapper): string {
  if (wrapper.isForceCoinAnnouncement()) return "forceCoinAnnouncement";
  if (wrapper.isForceCoinMessage()) return "forceCoinMessage";
  if (wrapper.isForceSingletonRecreation()) return "forceSingletonRecreation";
  if (wrapper.asPreventConditionOpcode() !== null) return "preventConditionOpcode";
  if (wrapper.isPreventMultipleCreateCoins()) return "preventMultipleCreateCoins";
  if (wrapper.asTimelock() !== null) return "timelock";
  if (wrapper.asForce1Of2RestrictedVariable() !== null) return "force1Of2";
  throw new Error("Unknown parsed wrapper");
}

test("round trip and parse each member kind", (t) => {
  const clvm = new Clvm();
  const k1 = K1Pair.fromSeed(1n).pk;
  const r1 = R1Pair.fromSeed(2n).pk;
  const bls = BlsPair.fromSeed(3n).pk;
  const launcherId = fromHex("22".repeat(32));
  const fixedPuzzleHash = fromHex("33".repeat(32));
  const config = new MemberConfig().withTopLevel(true);

  const cases = [
    {
      member: MemberMemo.k1(clvm, k1, true, true),
      name: "k1",
      hash: k1MemberHash(config, k1, true),
    },
    {
      member: MemberMemo.r1(clvm, r1, false, true),
      name: "r1",
      hash: r1MemberHash(config, r1, false),
    },
    {
      member: MemberMemo.passkey(clvm, r1, true, true),
      name: "passkey",
      hash: passkeyMemberHash(config, r1, true),
    },
    {
      member: MemberMemo.bls(clvm, bls, false, false, true),
      name: "bls",
      hash: blsMemberHash(config, bls, false),
    },
    {
      member: MemberMemo.bls(clvm, bls, false, true, true),
      name: "blsTaproot",
      hash: null,
    },
    {
      member: MemberMemo.singleton(clvm, launcherId, false, true),
      name: "singleton",
      hash: singletonMemberHash(config, launcherId, false),
    },
    {
      member: MemberMemo.fixedPuzzle(clvm, fixedPuzzleHash, true),
      name: "fixedPuzzle",
      hash: fixedMemberHash(config, fixedPuzzleHash),
    },
  ];

  for (const { member, name, hash } of cases) {
    const memo = memberMemo(member);
    const parsed = roundTrip(clvm, memo);

    t.deepEqual(parsed.innerPuzzleHash(), memo.innerPuzzleHash());
    if (hash !== null) {
      t.deepEqual(parsed.innerPuzzleHash(), hash);
    }

    const parsedMember = parseMember(parsed);
    t.not(parsedMember, null);
    t.is(memberName(parsedMember!), name);
  }

  const k1Member = parseMember(roundTrip(clvm, memberMemo(cases[0].member)))!;
  t.true(k1Member.fastForward());
  t.deepEqual(k1Member.asK1()!.toBytes(), k1.toBytes());
  t.is(k1Member.asR1(), null);

  const singleton = parseMember(
    roundTrip(clvm, memberMemo(cases[5].member)),
  )!;
  t.deepEqual(singleton.asSingleton(), launcherId);
  t.false(singleton.fastForward());

  const fixed = parseMember(roundTrip(clvm, memberMemo(cases[6].member)))!;
  t.deepEqual(fixed.asFixedPuzzle(), fixedPuzzleHash);
});

test("hidden members need the context to parse", (t) => {
  const clvm = new Clvm();
  const k1 = K1Pair.fromSeed(1n).pk;
  const memo = roundTrip(clvm, memberMemo(MemberMemo.k1(clvm, k1, false, false)));

  t.is(parseMember(memo), null);

  const ctx = new MipsMemoContext();
  ctx.addK1(k1);
  const parsed = parseMember(memo, ctx);
  t.deepEqual(parsed?.asK1()?.toBytes(), k1.toBytes());
  t.false(parsed!.fastForward());
});

test("custom and unknown members are preserved", (t) => {
  const clvm = new Clvm();

  const puzzle = clvm.delegatedSpend([
    clvm.createCoin(fromHex("44".repeat(32)), 1n),
  ]).puzzle;
  const custom = roundTrip(
    clvm,
    memberMemo(new MemberMemo(puzzle.treeHash(), puzzle)),
  );
  const parsedCustom = parseMember(custom)!;
  t.deepEqual(parsedCustom.asCustom()!.treeHash(), puzzle.treeHash());
  t.deepEqual(
    custom.innerPuzzleHash(),
    customMemberHash(new MemberConfig().withTopLevel(true), puzzle.treeHash()),
  );

  const unknownHash = fromHex("55".repeat(32));
  const unknown = roundTrip(
    clvm,
    memberMemo(new MemberMemo(unknownHash, clvm.alloc("hello"))),
  );
  t.is(parseMember(unknown), null);
  const member = unknown.innerPuzzle.kind.asMember()!;
  t.deepEqual(member.puzzleHash, unknownHash);
  t.is(member.memo.unparse(), '"hello"');
});

test("parse restrictions and wrappers", (t) => {
  const clvm = new Clvm();
  const ctx = new MipsMemoContext();

  const timelock = RestrictionMemo.timelock(clvm, 100n, true).parse(ctx)!;
  t.is(timelock.asTimelock(), 100n);
  t.is(timelock.asEnforceDelegatedPuzzleWrappers(), null);

  const hiddenTimelock = RestrictionMemo.timelock(clvm, 200n, false);
  t.is(hiddenTimelock.parse(ctx), null);
  const timelockCtx = new MipsMemoContext();
  timelockCtx.addTimelock(200n);
  t.is(hiddenTimelock.parse(timelockCtx)?.asTimelock(), 200n);

  const leftSideSubtreeHash = fromHex("66".repeat(32));
  const memberValidatorListHash = fromHex("77".repeat(32));
  const delegatedPuzzleValidatorListHash = clvm.nil().treeHash();
  const force1Of2 = RestrictionMemo.force1Of2RestrictedVariable(
    clvm,
    leftSideSubtreeHash,
    0,
    memberValidatorListHash,
    delegatedPuzzleValidatorListHash,
  );
  const parsedForce1Of2 = force1Of2
    .parse(ctx)!
    .asForce1Of2RestrictedVariable()!;
  t.deepEqual(parsedForce1Of2.leftSideSubtreeHash, leftSideSubtreeHash);
  t.deepEqual(parsedForce1Of2.memberValidatorListHash, memberValidatorListHash);

  const wrappers = [
    new WrapperMemo(force1Of2.puzzleHash, force1Of2.memo),
    ...WrapperMemo.preventVaultSideEffects(clvm, true),
    WrapperMemo.forceSingletonRecreation(clvm),
    WrapperMemo.timelock(clvm, 50n, true),
  ];
  const enforce = RestrictionMemo.enforceDelegatedPuzzleWrappers(clvm, wrappers)
    .parse(ctx)!
    .asEnforceDelegatedPuzzleWrappers()!;
  t.is(enforce.length, wrappers.length);
  enforce.forEach((wrapper, i) => {
    t.deepEqual(wrapper.puzzleHash, wrappers[i].puzzleHash);
  });

  const parsedWrappers = enforce.map((wrapper) => wrapper.parse(ctx)!);
  t.deepEqual(parsedWrappers.map(wrapperName), [
    "force1Of2",
    "preventConditionOpcode",
    "preventConditionOpcode",
    "preventConditionOpcode",
    "preventConditionOpcode",
    "preventMultipleCreateCoins",
    "forceSingletonRecreation",
    "timelock",
  ]);
  t.deepEqual(
    parsedWrappers.slice(1, 5).map((wrapper) => wrapper.asPreventConditionOpcode()),
    [60, 62, 66, 67],
  );
  t.is(parsedWrappers[7].asTimelock(), 50n);

  t.true(WrapperMemo.forceCoinAnnouncement(clvm).parse(ctx)!.isForceCoinAnnouncement());
  t.true(WrapperMemo.forceCoinMessage(clvm).parse(ctx)!.isForceCoinMessage());

  const unknown = new RestrictionMemo(
    false,
    fromHex("88".repeat(32)),
    clvm.nil(),
  );
  t.is(unknown.parse(ctx), null);
});

test("nested m of n round trip", (t) => {
  const clvm = new Clvm();
  const keys = K1Pair.manyFromSeed(10n, 3).map((pair) => pair.pk);
  const leaves = keys.map(
    (key) =>
      new InnerPuzzleMemo(
        0,
        [],
        MemoKind.member(MemberMemo.k1(clvm, key, false, true)),
      ),
  );

  const memo = new MipsMemo(
    new InnerPuzzleMemo(
      0,
      [RestrictionMemo.timelock(clvm, 10n, true)],
      MemoKind.mOfN(
        new MofNMemo(2, [
          new InnerPuzzleMemo(0, [], MemoKind.mOfN(new MofNMemo(2, leaves))),
          leaves[0],
          leaves[1],
        ]),
      ),
    ),
  );

  const parsed = roundTrip(clvm, memo);
  t.deepEqual(parsed.innerPuzzleHash(), memo.innerPuzzleHash());
  t.is(parsed.innerPuzzle.kind.asMOfN()!.required, 2);
  t.is(parsed.innerPuzzle.kind.asMOfN()!.items.length, 3);
});

test("malformed memos are rejected", (t) => {
  const clvm = new Clvm();
  const memo = clvm.mipsMemo(
    memberMemo(MemberMemo.k1(clvm, K1Pair.fromSeed(1n).pk, false, true)),
  );
  const inner = memo.rest().first();
  const list = (...items: Program[]) => clvm.alloc(items);

  t.not(memo.parseMipsMemo(), null);

  const cases: Program[] = [
    clvm.nil(),
    clvm.alloc("CHIP-0043"),
    list(clvm.alloc("CHIP-0042"), inner),
    list(clvm.alloc("CHIP-0043")),
    list(
      clvm.alloc("CHIP-0043"),
      clvm.pair(
        inner.first(),
        clvm.pair(
          inner.rest().first(),
          list(
            clvm.alloc(0),
            list(
              clvm.atom(fromHex("11".repeat(31))),
              clvm.nil(),
            ),
          ),
        ),
      ),
    ),
    list(
      clvm.alloc("CHIP-0043"),
      clvm.pair(
        inner.first(),
        clvm.pair(
          inner.rest().first(),
          list(clvm.alloc(2), clvm.nil()),
        ),
      ),
    ),
  ];

  for (const program of cases) {
    t.is(program.parseMipsMemo(), null, program.unparse());
  }
});

type Signer =
  | { kind: "k1"; key: ReturnType<typeof K1Pair.fromSeed>["pk"] }
  | { kind: "r1" | "passkey"; key: ReturnType<typeof R1Pair.fromSeed>["pk"] }
  | { kind: "bls"; key: PublicKey }
  | { kind: "vault"; launcherId: Buffer };

interface Signers {
  signers: Signer[];
  threshold: number;
}

interface VaultKeys {
  custody: Signers;
  recovery: Signers;
  clawbackTimelock: bigint;
}

type VaultState =
  | { state: "CUSTODY" }
  | {
      state: "RECOVERY";
      postRecoveryInnerPuzzleHash: Uint8Array;
      amount: bigint;
      recoveryFinishMemos: Program | null;
    };

function memberHash(signer: Signer, config: MemberConfig): Buffer {
  switch (signer.kind) {
    case "k1":
      return k1MemberHash(config, signer.key, true);
    case "r1":
      return r1MemberHash(config, signer.key, true);
    case "passkey":
      return passkeyMemberHash(config, signer.key, true);
    case "bls":
      return blsMemberHash(config, signer.key, false);
    case "vault":
      return singletonMemberHash(config, signer.launcherId, false);
  }
}

function recoveryFinishSpend(
  clvm: Clvm,
  keys: VaultKeys,
  state: Extract<VaultState, { state: "RECOVERY" }>,
  forceSingletonRecreation: boolean,
) {
  const amount = forceSingletonRecreation ? state.amount : 1n;
  return clvm.delegatedSpend([
    clvm.createCoin(
      state.postRecoveryInnerPuzzleHash,
      amount,
      state.recoveryFinishMemos,
    ),
    clvm.assertSecondsRelative(keys.clawbackTimelock),
    ...(forceSingletonRecreation ? [clvm.assertMyAmount(amount)] : []),
  ]);
}

// Port of `getVaultInternals` in ent-wallet's `vault.ts`.
function vaultInnerPuzzleHash(
  clvm: Clvm,
  keys: VaultKeys,
  state: VaultState,
  forceSingletonRecreation: boolean,
): Buffer {
  const config = new MemberConfig();
  const sortedHashes = (signers: Signers, config: MemberConfig) =>
    signers.signers
      .map((signer) => memberHash(signer, config))
      .sort(Buffer.compare);

  const custodyHashes = sortedHashes(keys.custody, config);
  const custodyHash =
    custodyHashes.length > 1
      ? mOfNHash(config, keys.custody.threshold, custodyHashes)
      : custodyHashes[0];

  let recoveryHash: Buffer;

  if (state.state === "CUSTODY") {
    const timelock = timelockRestriction(keys.clawbackTimelock);
    const recoveryConfig = config.withRestrictions([
      force1Of2Restriction(
        custodyHash,
        0,
        treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
        clvm.nil().treeHash(),
      ),
      ...preventVaultSideEffectsRestriction(),
      ...(forceSingletonRecreation
        ? [forceSingletonRecreationRestriction()]
        : []),
    ]);

    const recoveryHashes = sortedHashes(keys.recovery, config);
    recoveryHash =
      recoveryHashes.length > 1
        ? mOfNHash(recoveryConfig, keys.recovery.threshold, recoveryHashes)
        : memberHash(keys.recovery.signers[0], recoveryConfig);
  } else {
    const spend = recoveryFinishSpend(
      clvm,
      keys,
      state,
      forceSingletonRecreation,
    );
    recoveryHash = customMemberHash(
      config.withRestrictions([timelockRestriction(keys.clawbackTimelock)]),
      spend.puzzle.treeHash(),
    );
  }

  return mOfNHash(config.withTopLevel(true), 1, [custodyHash, recoveryHash]);
}

interface MemoNode {
  hash: Buffer;
  memo: InnerPuzzleMemo;
}

function memoNode(memo: InnerPuzzleMemo): MemoNode {
  return { hash: memo.innerPuzzleHash(false), memo };
}

function signerMemberMemo(clvm: Clvm, signer: Signer): MemberMemo {
  switch (signer.kind) {
    case "k1":
      return MemberMemo.k1(clvm, signer.key, true, true);
    case "r1":
      return MemberMemo.r1(clvm, signer.key, true, true);
    case "passkey":
      return MemberMemo.passkey(clvm, signer.key, true, true);
    case "bls":
      return MemberMemo.bls(clvm, signer.key, false, false, true);
    case "vault":
      return MemberMemo.singleton(clvm, signer.launcherId, false, true);
  }
}

function signerTree(
  clvm: Clvm,
  signers: Signers,
  restrictions: RestrictionMemo[] = [],
): MemoNode {
  const nodes = signers.signers
    .map((signer) =>
      memoNode(
        new InnerPuzzleMemo(
          0,
          [],
          MemoKind.member(signerMemberMemo(clvm, signer)),
        ),
      ),
    )
    .sort((a, b) => Buffer.compare(a.hash, b.hash));

  if (nodes.length > 1) {
    return memoNode(
      new InnerPuzzleMemo(
        0,
        restrictions,
        MemoKind.mOfN(
          new MofNMemo(
            signers.threshold,
            nodes.map((node) => node.memo),
          ),
        ),
      ),
    );
  }

  return memoNode(new InnerPuzzleMemo(0, restrictions, nodes[0].memo.kind));
}

// Port of `getVaultMipsMemoListBytes` in ent-wallet's `vault.ts`.
function vaultMemoList(
  clvm: Clvm,
  keys: VaultKeys,
  state: VaultState,
  forceSingletonRecreation: boolean,
): Buffer {
  const custody = signerTree(clvm, keys.custody);
  let recovery: MemoNode;

  if (state.state === "CUSTODY") {
    const timelock = timelockRestriction(keys.clawbackTimelock);
    const force1Of2 = RestrictionMemo.force1Of2RestrictedVariable(
      clvm,
      custody.hash,
      0,
      treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
      clvm.nil().treeHash(),
    );
    recovery = signerTree(clvm, keys.recovery, [
      RestrictionMemo.enforceDelegatedPuzzleWrappers(clvm, [
        new WrapperMemo(force1Of2.puzzleHash, force1Of2.memo),
        ...WrapperMemo.preventVaultSideEffects(clvm, true),
        ...(forceSingletonRecreation
          ? [WrapperMemo.forceSingletonRecreation(clvm)]
          : []),
      ]),
    ]);
  } else {
    const puzzle = recoveryFinishSpend(
      clvm,
      keys,
      state,
      forceSingletonRecreation,
    ).puzzle;
    recovery = memoNode(
      new InnerPuzzleMemo(
        0,
        [RestrictionMemo.timelock(clvm, keys.clawbackTimelock, true)],
        MemoKind.member(new MemberMemo(puzzle.treeHash(), puzzle)),
      ),
    );
  }

  const memo = new MipsMemo(
    new InnerPuzzleMemo(
      0,
      [],
      MemoKind.mOfN(new MofNMemo(1, [custody.memo, recovery.memo])),
    ),
  );

  return clvm.alloc([clvm.mipsMemo(memo)]).serialize();
}

interface Found {
  members: string[];
  restrictions: string[];
  wrappers: string[];
}

function collect(
  t: { not: (a: unknown, b: unknown, message?: string) => void },
  memo: InnerPuzzleMemo,
  found: Found,
) {
  const ctx = new MipsMemoContext();

  for (const restriction of memo.restrictions) {
    const parsed = restriction.parse(ctx);
    t.not(parsed, null, "restriction should parse");
    found.restrictions.push(restrictionName(parsed!));

    for (const wrapper of parsed!.asEnforceDelegatedPuzzleWrappers() ?? []) {
      const parsedWrapper = wrapper.parse(ctx);
      t.not(parsedWrapper, null, "wrapper should parse");
      found.wrappers.push(wrapperName(parsedWrapper!));
    }
  }

  const member = memo.kind.asMember();
  if (member) {
    const parsed = member.parse(ctx);
    t.not(parsed, null, "member should parse");
    found.members.push(memberName(parsed!));
    return;
  }

  for (const item of memo.kind.asMOfN()!.items) {
    collect(t, item, found);
  }
}

const k1Signers = K1Pair.manyFromSeed(100n, 3).map(
  (pair): Signer => ({ kind: "k1", key: pair.pk }),
);
const r1Signers = R1Pair.manyFromSeed(200n, 2).map((pair) => pair.pk);
const blsSigner: Signer = { kind: "bls", key: BlsPair.fromSeed(300n).pk };
const vaultSigner: Signer = {
  kind: "vault",
  launcherId: fromHex("99".repeat(32)),
};

const custodyCases: Record<string, Signers> = {
  singleK1: { signers: [k1Signers[0]], threshold: 1 },
  singleBls: { signers: [blsSigner], threshold: 1 },
  twoOfFourMixed: {
    signers: [
      k1Signers[1],
      { kind: "r1", key: r1Signers[0] },
      { kind: "passkey", key: r1Signers[1] },
      blsSigner,
    ],
    threshold: 2,
  },
  nestedVault: { signers: [k1Signers[0], vaultSigner], threshold: 1 },
};

const recoveryCases: Record<string, Signers> = {
  singleK1: { signers: [k1Signers[2]], threshold: 1 },
  singleVault: { signers: [vaultSigner], threshold: 1 },
  twoOfTwo: {
    signers: [k1Signers[2], { kind: "passkey", key: r1Signers[0] }],
    threshold: 2,
  },
};

for (const [custodyName, custody] of Object.entries(custodyCases)) {
  for (const [recoveryName, recovery] of Object.entries(recoveryCases)) {
    for (const forceSingletonRecreation of [false, true]) {
      const keys: VaultKeys = { custody, recovery, clawbackTimelock: 3600n };
      const name = `${custodyName} / ${recoveryName} / fsr=${forceSingletonRecreation}`;

      test(`vault custody memo round trip: ${name}`, (t) => {
        const clvm = new Clvm();
        const state: VaultState = { state: "CUSTODY" };
        const expected = vaultInnerPuzzleHash(
          clvm,
          keys,
          state,
          forceSingletonRecreation,
        );
        const bytes = vaultMemoList(clvm, keys, state, forceSingletonRecreation);

        const memo = clvm.deserialize(bytes).first().parseMipsMemo();
        t.not(memo, null);
        t.deepEqual(memo!.innerPuzzleHash(), expected);
        t.deepEqual(clvm.alloc([clvm.mipsMemo(memo!)]).serialize(), bytes);

        const found: Found = { members: [], restrictions: [], wrappers: [] };
        collect(t, memo!.innerPuzzle, found);
        t.is(found.members.length, custody.signers.length + recovery.signers.length);
        t.deepEqual(found.restrictions, ["wrappers"]);
        t.is(found.wrappers.length, forceSingletonRecreation ? 7 : 6);
        t.is(found.wrappers[0], "force1Of2");
      });

      test(`vault recovery memo round trip: ${name}`, (t) => {
        const clvm = new Clvm();
        const postRecoveryKeys: VaultKeys = {
          custody: recovery,
          recovery: custody,
          clawbackTimelock: 7200n,
        };
        const postRecoveryState: VaultState = { state: "CUSTODY" };
        const postRecoveryInnerPuzzleHash = vaultInnerPuzzleHash(
          clvm,
          postRecoveryKeys,
          postRecoveryState,
          forceSingletonRecreation,
        );
        const state: VaultState = {
          state: "RECOVERY",
          postRecoveryInnerPuzzleHash,
          amount: 1n,
          recoveryFinishMemos: clvm.deserialize(
            vaultMemoList(
              clvm,
              postRecoveryKeys,
              postRecoveryState,
              forceSingletonRecreation,
            ),
          ),
        };

        const expected = vaultInnerPuzzleHash(
          clvm,
          keys,
          state,
          forceSingletonRecreation,
        );
        const bytes = vaultMemoList(clvm, keys, state, forceSingletonRecreation);

        const memo = clvm.deserialize(bytes).first().parseMipsMemo();
        t.not(memo, null);
        t.deepEqual(memo!.innerPuzzleHash(), expected);
        t.deepEqual(clvm.alloc([clvm.mipsMemo(memo!)]).serialize(), bytes);

        const found: Found = { members: [], restrictions: [], wrappers: [] };
        collect(t, memo!.innerPuzzle, found);
        t.is(found.members.at(-1), "custom");
        t.deepEqual(found.restrictions, ["timelock"]);

        const recoveryFinish = memo!.innerPuzzle.kind.asMOfN()!.items[1];
        const puzzle = recoveryFinish.kind
          .asMember()!
          .parse(new MipsMemoContext())!
          .asCustom()!;
        const createCoin = puzzle.rest().first().toList()!;
        t.deepEqual(createCoin[1].toAtom(), postRecoveryInnerPuzzleHash);

        const nested = createCoin[3].first().parseMipsMemo();
        t.not(nested, null);
        t.deepEqual(nested!.innerPuzzleHash(), postRecoveryInnerPuzzleHash);
      });
    }
  }
}
