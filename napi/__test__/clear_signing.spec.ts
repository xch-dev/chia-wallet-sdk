import test from "ava";

import {
  blsMemberHash,
  calculateVaultCoinMessage,
  calculateVaultPuzzleMessage,
  calculateVaultStartRecoveryMessage,
  Clvm,
  Coin,
  customMemberHash,
  force1Of2Restriction,
  forceSingletonRecreationRestriction,
  fromHex,
  k1MemberHash,
  K1Pair,
  MemberConfig,
  MipsSpend,
  mOfNHash,
  parseVaultTransaction,
  passkeyMemberHash,
  preventVaultSideEffectsRestriction,
  r1MemberHash,
  R1Pair,
  Reveals,
  sha256,
  Signature,
  Simulator,
  singletonMemberHash,
  SpendBundle,
  timelockRestriction,
  treeHashPair,
  Vault,
  vaultMessageEnvelope,
  VaultMessageProof,
  vaultRecoveryDelegatedPuzzleHash,
  VaultVersion,
  verifyVaultSignedMessage,
} from "../index.js";

// The simulator validates signatures against testnet11's AGG_SIG_ME additional data.
const GENESIS_CHALLENGE = fromHex(
  "37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615",
);

test("parses a vault transaction and signs the coin message", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(0n);
  const bob = sim.bls(0n);

  const config = new MemberConfig().withTopLevel(true);

  const [vault, coin] = mintVaultWithCoin(
    sim,
    clvm,
    blsMemberHash(config, alice.pk, false),
    1000n,
  );

  const coinDelegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(bob.puzzleHash, 900n, null),
    clvm.reserveFee(100n),
  ]);

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
    clvm.sendMessage(23, coinDelegatedSpend.puzzle.treeHash(), [
      clvm.alloc(coin.coinId()),
    ]),
  ]);

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.blsMember(config, alice.pk, false);
  mips.spendVault(vault);

  const p2Spend = clvm.mipsSpend(coin, coinDelegatedSpend);
  p2Spend.singletonMember(
    config,
    vault.info.launcherId,
    false,
    vault.info.custodyHash,
    vault.coin.amount,
  );
  clvm.spendCoin(coin, p2Spend.spend(coin.puzzleHash));

  const coinSpends = clvm.coinSpends();

  const tx = parseVaultTransaction(
    clvm,
    Reveals.fromCoinSpends(clvm, coinSpends),
    vault.info.launcherId,
    delegatedSpend,
  );

  t.deepEqual(tx.vaultChild?.custodyHash, vault.info.custodyHash);
  t.is(tx.vaultChild?.amount, vault.coin.amount);
  t.is(tx.dropCoins.length, 0);
  t.is(tx.reservedFee, 100n);
  t.deepEqual(tx.p2PuzzleHashes, [coin.puzzleHash]);
  t.deepEqual(tx.delegatedPuzzleHash, delegatedSpend.puzzle.treeHash());

  t.is(tx.spends.length, 1);
  const [spend] = tx.spends;
  t.deepEqual(spend.asset.asXch()?.coinId(), coin.coinId());
  t.is(spend.clawback, null);
  t.false(spend.revoked);

  const custody = spend.custody.asP2Singleton();
  t.truthy(custody);
  t.deepEqual(custody?.launcherId, vault.info.launcherId);
  t.is(custody?.nonce, 0n);
  t.deepEqual(custody?.p2PuzzleHash, coin.puzzleHash);
  t.is(custody?.conditions.length, 2);

  t.is(spend.children.length, 1);
  const [child] = spend.children;
  t.is(child.asset.coin().amount, 900n);
  t.deepEqual(child.memos.p2PuzzleHash, bob.puzzleHash);
  t.true(child.transferType.isSent());

  t.is(tx.assetFlows.length, 1);
  const [flow] = tx.assetFlows;
  t.true(flow.asset.isXch());
  t.is(flow.inputAmount, 1000n);
  t.is(flow.outputAmount, 900n);

  // The coin message is what a BLS member signs, since AGG_SIG_ME appends the coin id and genesis challenge.
  const message = calculateVaultCoinMessage(
    tx.delegatedPuzzleHash,
    vault.coin.coinId(),
    GENESIS_CHALLENGE,
  );

  t.notThrows(() =>
    sim.newTransaction(new SpendBundle(coinSpends, alice.sk.sign(message))),
  );
});

test("signs the vault puzzle message for fast forward members", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const k1 = K1Pair.fromSeed(1n);

  const config = new MemberConfig().withTopLevel(true);

  const vault = mintVault(sim, clvm, k1MemberHash(config, k1.pk, true));

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const message = calculateVaultPuzzleMessage(
    delegatedSpend.puzzle.treeHash(),
    vault.coin.puzzleHash,
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.k1Member(config, k1.pk, k1.sk.signPrehashed(sha256(message)), true);
  mips.spendVault(vault);

  t.notThrows(() => sim.spendCoins(clvm.coinSpends(), []));
});

for (const [name, version] of [
  ["legacy", VaultVersion.Legacy],
  ["force singleton recreation", VaultVersion.ForceSingletonRecreation],
] as const) {
  test(`signs the start recovery coin message (${name})`, (t) => {
    const sim = new Simulator();
    const clvm = new Clvm();
    const recoveryKey = sim.bls(0n);

    const recovery = startRecovery(
      sim,
      clvm,
      version,
      (config) => blsMemberHash(config, recoveryKey.pk, false),
      (mips, config) => mips.blsMember(config, recoveryKey.pk, false),
    );

    const message = calculateVaultStartRecoveryMessage(
      recovery.delegatedPuzzleHash,
      recovery.leftSideSubtreeHash,
      recovery.recoveryTimelock,
      recovery.vault.coin.coinId(),
      GENESIS_CHALLENGE,
      version,
    );

    t.notThrows(() =>
      sim.newTransaction(
        new SpendBundle(clvm.coinSpends(), recoveryKey.sk.sign(message)),
      ),
    );
  });

  test(`signs the start recovery puzzle message (${name})`, (t) => {
    const sim = new Simulator();
    const clvm = new Clvm();
    const recoveryKey = K1Pair.fromSeed(2n);

    startRecovery(
      sim,
      clvm,
      version,
      (config) => k1MemberHash(config, recoveryKey.pk, true),
      (mips, config, recovery) => {
        const message = calculateVaultPuzzleMessage(
          vaultRecoveryDelegatedPuzzleHash(
            recovery.delegatedPuzzleHash,
            recovery.leftSideSubtreeHash,
            recovery.recoveryTimelock,
            version,
          ),
          recovery.vault.coin.puzzleHash,
        );

        mips.k1Member(
          config,
          recoveryKey.pk,
          recoveryKey.sk.signPrehashed(sha256(message)),
          true,
        );
      },
    );

    t.notThrows(() => sim.spendCoins(clvm.coinSpends(), []));
  });
}

test("legacy start recovery message is rejected for force singleton recreation vaults", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();
  const recoveryKey = sim.bls(0n);

  const recovery = startRecovery(
    sim,
    clvm,
    VaultVersion.ForceSingletonRecreation,
    (config) => blsMemberHash(config, recoveryKey.pk, false),
    (mips, config) => mips.blsMember(config, recoveryKey.pk, false),
  );

  const message = calculateVaultStartRecoveryMessage(
    recovery.delegatedPuzzleHash,
    recovery.leftSideSubtreeHash,
    recovery.recoveryTimelock,
    recovery.vault.coin.coinId(),
    GENESIS_CHALLENGE,
    VaultVersion.Legacy,
  );

  t.throws(() =>
    sim.newTransaction(
      new SpendBundle(clvm.coinSpends(), recoveryKey.sk.sign(message)),
    ),
  );
});

test("start recovery messages differ by vault version", (t) => {
  const args = [
    new Uint8Array(32).fill(1),
    new Uint8Array(32).fill(2),
    1n,
    new Uint8Array(32).fill(3),
    GENESIS_CHALLENGE,
  ] as const;

  t.notDeepEqual(
    calculateVaultStartRecoveryMessage(...args, VaultVersion.Legacy),
    calculateVaultStartRecoveryMessage(
      ...args,
      VaultVersion.ForceSingletonRecreation,
    ),
  );
});

interface StartedRecovery {
  vault: Vault;
  delegatedPuzzleHash: Uint8Array;
  leftSideSubtreeHash: Uint8Array;
  recoveryTimelock: bigint;
}

// Spends a vault through its recovery path without submitting the transaction, so the
// caller can sign the start recovery message for the given vault version.
function startRecovery(
  sim: Simulator,
  clvm: Clvm,
  version: VaultVersion,
  recoveryMemberHash: (config: MemberConfig) => Uint8Array,
  addRecoveryMember: (
    mips: MipsSpend,
    config: MemberConfig,
    recovery: StartedRecovery,
  ) => void,
): StartedRecovery {
  const custodyKey = K1Pair.fromSeed(1n);

  const config = new MemberConfig();
  const memberHash = k1MemberHash(config, custodyKey.pk, false);

  const recoveryTimelock = 1n;
  const timelock = timelockRestriction(recoveryTimelock);
  const recoveryConfig = config.withRestrictions([
    force1Of2Restriction(
      memberHash,
      0,
      treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
      clvm.nil().treeHash(),
    ),
    ...preventVaultSideEffectsRestriction(),
    ...(version === VaultVersion.ForceSingletonRecreation
      ? [forceSingletonRecreationRestriction()]
      : []),
  ]);
  const recoveryHash = recoveryMemberHash(recoveryConfig);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 1, [memberHash, recoveryHash]),
  );

  const recoveryFinishMemberSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
    clvm.assertSecondsRelative(recoveryTimelock),
  ]);
  const recoveryFinishMemberHash = customMemberHash(
    config.withRestrictions([timelock]),
    recoveryFinishMemberSpend.puzzle.treeHash(),
  );
  const custodyHash = mOfNHash(config.withTopLevel(true), 1, [
    memberHash,
    recoveryFinishMemberHash,
  ]);

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(custodyHash, vault.coin.amount, null),
    clvm.assertMyAmount(vault.coin.amount),
  ]);

  const recovery = {
    vault,
    delegatedPuzzleHash: delegatedSpend.puzzle.treeHash(),
    leftSideSubtreeHash: memberHash,
    recoveryTimelock,
  };

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [memberHash, recoveryHash]);
  addRecoveryMember(mips, recoveryConfig, recovery);
  mips.preventVaultSideEffects();
  if (version === VaultVersion.ForceSingletonRecreation) {
    mips.forceSingletonRecreation();
  }
  mips.force1Of2RestrictedVariable(
    memberHash,
    0,
    treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
    clvm.nil().treeHash(),
    recoveryFinishMemberSpend.puzzle.treeHash(),
  );
  mips.spendVault(vault);

  return recovery;
}

test("signs and verifies vault messages", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const r1 = R1Pair.fromSeed(1n);
  const passkey = R1Pair.fromSeed(2n);

  const config = new MemberConfig();
  const r1Hash = r1MemberHash(config, r1.pk, true);
  const passkeyHash = passkeyMemberHash(config, passkey.pk, true);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 2, [r1Hash, passkeyHash]),
  );

  const messages = [
    Buffer.from("Sign in to example.com\nNonce: 4f9a2c", "utf-8"),
    Buffer.from([0xff, 0x00, 0x80]),
  ];
  const envelope = vaultMessageEnvelope(clvm, vault, messages);

  // Clear signing shows the messages, and that the vault is recreated as it was.
  const tx = parseVaultTransaction(
    clvm,
    new Reveals(clvm),
    vault.info.launcherId,
    envelope,
  );
  t.deepEqual(
    tx.signedMessages.map((message) => Buffer.from(message)),
    messages,
  );
  t.deepEqual(tx.vaultChild?.custodyHash, vault.info.custodyHash);
  t.is(tx.vaultChild?.amount, vault.coin.amount);
  t.is(tx.spends.length, 0);

  // Each member signs the envelope for the vault's puzzle hash, as with any fast forward signature.
  const message = sha256(
    calculateVaultPuzzleMessage(
      envelope.puzzle.treeHash(),
      vault.coin.puzzleHash,
    ),
  );

  const authenticatorData = Buffer.from(
    "49960de5880e8c687434170f6476605b8fe4aeb9a28632c7995cf3ba831d97630500000009",
    "hex",
  );
  const clientDataJSON = Buffer.from(
    `{"type":"webauthn.get","challenge":"${Buffer.from(message).toString(
      "base64url",
    )}","origin":"http://localhost:3000","crossOrigin":false}`,
    "utf-8",
  );
  const challengeIndex = clientDataJSON.indexOf('"challenge":');

  const mips = clvm.mipsSpend(vault.coin, envelope);
  mips.mOfN(config.withTopLevel(true), 2, [r1Hash, passkeyHash]);
  mips.r1Member(config, r1.pk, r1.sk.signPrehashed(message), true);
  mips.passkeyMember(
    config,
    passkey.pk,
    passkey.sk.signPrehashed(
      sha256(Buffer.concat([authenticatorData, sha256(clientDataJSON)])),
    ),
    authenticatorData,
    clientDataJSON,
    challengeIndex,
    true,
  );

  // The vault is spent like any other transaction, and the signed spend is the proof.
  mips.spendVault(vault);
  const [coinSpend] = clvm.coinSpends();
  const proof = new VaultMessageProof(coinSpend, Signature.infinity());

  const encoded = proof.encodeString();
  t.true(encoded.startsWith("vaultsig1"));

  for (const decoded of [
    VaultMessageProof.decodeString(encoded),
    VaultMessageProof.decode(proof.encode()),
  ]) {
    t.deepEqual(
      verifyVaultSignedMessage(
        decoded,
        vault.info.launcherId,
        vault.info.custodyHash,
        GENESIS_CHALLENGE,
      ).map((message) => Buffer.from(message)),
      messages,
    );
  }

  t.throws(() =>
    verifyVaultSignedMessage(
      proof,
      vault.info.launcherId,
      mOfNHash(config.withTopLevel(true), 1, [r1Hash, passkeyHash]),
      GENESIS_CHALLENGE,
    ),
  );
  t.throws(() =>
    verifyVaultSignedMessage(
      proof,
      new Uint8Array(32),
      vault.info.custodyHash,
      GENESIS_CHALLENGE,
    ),
  );

  // The proof is a valid vault spend, but it can never be included on chain.
  t.throws(() => sim.spendCoins([coinSpend], []));
});

function mintVault(sim: Simulator, clvm: Clvm, custodyHash: Uint8Array): Vault {
  const p2 = sim.bls(1n);

  const { vault, parentConditions } = clvm.mintVault(
    p2.coin.coinId(),
    custodyHash,
    clvm.nil(),
  );

  clvm.spendCoin(
    p2.coin,
    clvm.standardSpend(p2.pk, clvm.delegatedSpend(parentConditions)),
  );

  sim.spendCoins(clvm.coinSpends(), [p2.sk]);

  return vault;
}

function mintVaultWithCoin(
  sim: Simulator,
  clvm: Clvm,
  custodyHash: Uint8Array,
  amount: bigint,
): [Vault, Coin] {
  const p2 = sim.bls(amount + 1n);

  const { vault, parentConditions } = clvm.mintVault(
    p2.coin.coinId(),
    custodyHash,
    clvm.nil(),
  );

  const p2PuzzleHash = singletonMemberHash(
    new MemberConfig().withTopLevel(true),
    vault.info.launcherId,
    false,
  );

  clvm.spendCoin(
    p2.coin,
    clvm.standardSpend(
      p2.pk,
      clvm.delegatedSpend([
        ...parentConditions,
        clvm.createCoin(
          p2PuzzleHash,
          amount,
          clvm.alloc([vault.info.launcherId]),
        ),
      ]),
    ),
  );

  sim.spendCoins(clvm.coinSpends(), [p2.sk]);

  return [vault, new Coin(p2.coin.coinId(), p2PuzzleHash, amount)];
}
