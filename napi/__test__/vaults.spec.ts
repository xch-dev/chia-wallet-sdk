import test from "ava";

import {
  blsMemberHash,
  Clvm,
  Coin,
  customMemberHash,
  force1Of2Restriction,
  k1MemberHash,
  K1Pair,
  K1SecretKey,
  K1Signature,
  MemberConfig,
  mOfNHash,
  passkeyMemberHash,
  preventVaultSideEffectsRestriction,
  R1Pair,
  sha256,
  Simulator,
  singletonMemberHash,
  Spend,
  timelockRestriction,
  treeHashPair,
  Vault,
  wrappedDelegatedPuzzleHash,
} from "../index.js";

test("bls key vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(0n);

  const config = new MemberConfig().withTopLevel(true);

  const [vault, coin] = mintVaultWithCoin(
    sim,
    clvm,
    blsMemberHash(config, alice.pk, false),
    1n
  );

  const coinDelegatedSpend = clvm.delegatedSpend([clvm.reserveFee(1n)]);

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
    vault.coin.amount
  );

  clvm.spendCoin(coin, p2Spend.spend(coin.puzzleHash));

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(true);
});

test("single signer vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const k1 = K1Pair.fromSeed(1n);

  const config = new MemberConfig().withTopLevel(true);

  const vault = mintVault(sim, clvm, k1MemberHash(config, k1.pk, false));

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(
    k1.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.k1Member(config, k1.pk, signature, false);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("passkey member vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const r1 = R1Pair.fromSeed(1n);

  const config = new MemberConfig().withTopLevel(true);

  const fastForward = false;

  const vault = mintVault(
    sim,
    clvm,
    passkeyMemberHash(config, r1.pk, fastForward)
  );

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const challengeIndex = 23;
  const originalMessage = Buffer.from(
    sha256(
      Buffer.concat([
        Buffer.from(delegatedSpend.puzzle.treeHash()),
        fastForward ? vault.coin.puzzleHash : vault.coin.coinId(),
      ])
    )
  );

  const authenticatorData = Buffer.from(
    "49960de5880e8c687434170f6476605b8fe4aeb9a28632c7995cf3ba831d97630500000009",
    "hex"
  );
  const clientDataJSON = Buffer.from(
    `{"type":"webauthn.get","challenge":"${originalMessage.toString(
      "base64url"
    )}","origin":"http://localhost:3000","crossOrigin":false}`,
    "utf-8"
  );
  // Reproduce web browser passkey behavior
  const message = sha256(
    Buffer.concat([authenticatorData, sha256(clientDataJSON)])
  );

  const signature = r1.sk.signPrehashed(message);

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.passkeyMember(
    config,
    r1.pk,
    signature,
    authenticatorData,
    clientDataJSON,
    challengeIndex,
    fastForward
  );
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("single signer fast forward vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const k1 = K1Pair.fromSeed(1n);

  const config = new MemberConfig().withTopLevel(true);

  const vault = mintVault(sim, clvm, k1MemberHash(config, k1.pk, true));

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(
    k1.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    true
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.k1Member(config, k1.pk, signature, true);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("1 of 2 vault (path 1)", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = K1Pair.fromSeed(1n);
  const bob = K1Pair.fromSeed(2n);

  const config = new MemberConfig();

  const aliceHash = k1MemberHash(config, alice.pk, false);
  const bobHash = k1MemberHash(config, bob.pk, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 1, [aliceHash, bobHash])
  );

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(
    alice.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [aliceHash, bobHash]);
  mips.k1Member(config, alice.pk, signature, false);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("1 of 2 vault (path 2)", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = K1Pair.fromSeed(1n);
  const bob = K1Pair.fromSeed(2n);

  const config = new MemberConfig();

  const aliceHash = k1MemberHash(config, alice.pk, false);
  const bobHash = k1MemberHash(config, bob.pk, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 1, [aliceHash, bobHash])
  );

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(
    bob.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [aliceHash, bobHash]);
  mips.k1Member(config, bob.pk, signature, false);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("2 of 2 vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = K1Pair.fromSeed(1n);
  const bob = K1Pair.fromSeed(2n);

  const config = new MemberConfig();

  const aliceHash = k1MemberHash(config, alice.pk, false);
  const bobHash = k1MemberHash(config, bob.pk, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 2, [aliceHash, bobHash])
  );

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const aliceSignature = signK1(
    alice.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );
  const bobSignature = signK1(
    bob.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 2, [aliceHash, bobHash]);
  mips.k1Member(config, alice.pk, aliceSignature, false);
  mips.k1Member(config, bob.pk, bobSignature, false);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("2 of 3 vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = K1Pair.fromSeed(1n);
  const bob = K1Pair.fromSeed(2n);
  const charlie = K1Pair.fromSeed(3n);

  const config = new MemberConfig();

  const aliceHash = k1MemberHash(config, alice.pk, false);
  const bobHash = k1MemberHash(config, bob.pk, false);
  const charlieHash = k1MemberHash(config, charlie.pk, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 2, [aliceHash, bobHash, charlieHash])
  );

  const delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  const aliceSignature = signK1(
    alice.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );
  const bobSignature = signK1(
    bob.sk,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 2, [aliceHash, bobHash, charlieHash]);
  mips.k1Member(config, alice.pk, aliceSignature, false);
  mips.k1Member(config, bob.pk, bobSignature, false);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

test("fast forward paths vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = K1Pair.fromSeed(1n);
  const bob = K1Pair.fromSeed(2n);

  const config = new MemberConfig();

  const aliceRegularHash = k1MemberHash(config, alice.pk, false);
  const aliceFastForwardHash = k1MemberHash(config, alice.pk, true);
  const bobRegularHash = k1MemberHash(config, bob.pk, false);
  const bobFastForwardHash = k1MemberHash(config, bob.pk, true);

  const regularPathHash = mOfNHash(config, 1, [
    aliceRegularHash,
    bobRegularHash,
  ]);
  const fastForwardPathHash = mOfNHash(config, 1, [
    aliceFastForwardHash,
    bobFastForwardHash,
  ]);

  let vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 1, [
      regularPathHash,
      fastForwardPathHash,
    ])
  );

  for (const fastForward of [false, true, false, true]) {
    const delegatedSpend = clvm.delegatedSpend([
      clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
    ]);

    const aliceSignature = signK1(
      alice.sk,
      vault,
      delegatedSpend.puzzle.treeHash(),
      fastForward
    );

    const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
    mips.mOfN(config.withTopLevel(true), 1, [
      regularPathHash,
      fastForwardPathHash,
    ]);
    mips.mOfN(
      config,
      1,
      fastForward
        ? [aliceFastForwardHash, bobFastForwardHash]
        : [aliceRegularHash, bobRegularHash]
    );
    mips.k1Member(config, alice.pk, aliceSignature, fastForward);
    mips.spendVault(vault);

    sim.spendCoins(clvm.coinSpends(), []);

    vault = vault.child(vault.info.custodyHash, vault.coin.amount);
  }

  t.true(true);
});

test("single signer recovery vault", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const custodyKey = K1Pair.fromSeed(1n);
  const recoveryKey = K1Pair.fromSeed(2n);

  // Initial vault
  const config = new MemberConfig();

  const memberHash = k1MemberHash(config, custodyKey.pk, false);

  const timelock = timelockRestriction(1n);
  const recoveryRestrictions = [
    force1Of2Restriction(
      memberHash,
      0,
      treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
      clvm.nil().treeHash()
    ),
    ...preventVaultSideEffectsRestriction(),
  ];
  const initialRecoveryHash = k1MemberHash(
    config.withRestrictions(recoveryRestrictions),
    recoveryKey.pk,
    false
  );

  let vault = mintVault(
    sim,
    clvm,
    mOfNHash(config.withTopLevel(true), 1, [memberHash, initialRecoveryHash])
  );

  let delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);

  let mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [memberHash, initialRecoveryHash]);
  mips.k1Member(
    config,
    custodyKey.pk,
    signK1(custodyKey.sk, vault, delegatedSpend.puzzle.treeHash(), false),
    false
  );
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  // Initiate recovery
  const oldCustodyHash = vault.info.custodyHash;
  const recoveryDelegatedSpend = new Spend(clvm.nil(), clvm.nil());

  const recoveryFinishMemberSpend = clvm.delegatedSpend([
    clvm.createCoin(oldCustodyHash, vault.coin.amount, null),
    clvm.assertSecondsRelative(1n),
  ]);
  const recoveryFinishMemberHash = customMemberHash(
    config.withRestrictions([timelock]),
    recoveryFinishMemberSpend.puzzle.treeHash()
  );

  const custodyHash = mOfNHash(config.withTopLevel(true), 1, [
    memberHash,
    recoveryFinishMemberHash,
  ]);

  delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(custodyHash, vault.coin.amount, null),
  ]);

  vault = vault.child(vault.info.custodyHash, vault.coin.amount);
  mips = clvm.mipsSpend(vault.coin, delegatedSpend);

  mips.mOfN(config.withTopLevel(true), 1, [memberHash, initialRecoveryHash]);

  mips.k1Member(
    config.withRestrictions(recoveryRestrictions),
    recoveryKey.pk,
    signK1(
      recoveryKey.sk,
      vault,
      wrappedDelegatedPuzzleHash(
        recoveryRestrictions,
        delegatedSpend.puzzle.treeHash()
      ),
      false
    ),
    false
  );

  mips.preventVaultSideEffects();

  mips.force1Of2RestrictedVariable(
    memberHash,
    0,
    treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
    clvm.nil().treeHash(),
    recoveryFinishMemberSpend.puzzle.treeHash()
  );

  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  // Finish recovery
  vault = vault.child(custodyHash, vault.coin.amount);
  mips = clvm.mipsSpend(vault.coin, recoveryDelegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [
    memberHash,
    recoveryFinishMemberHash,
  ]);
  mips.customMember(
    config.withRestrictions([timelock]),
    recoveryFinishMemberSpend
  );
  mips.timelock(1n);
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  // Make sure the vault is spendable after recovery
  vault = vault.child(oldCustodyHash, vault.coin.amount);
  delegatedSpend = clvm.delegatedSpend([
    clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
  ]);
  mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [memberHash, initialRecoveryHash]);
  mips.k1Member(
    config,
    custodyKey.pk,
    signK1(custodyKey.sk, vault, delegatedSpend.puzzle.treeHash(), false),
    false
  );
  mips.spendVault(vault);

  sim.spendCoins(clvm.coinSpends(), []);

  t.true(true);
});

function mintVault(sim: Simulator, clvm: Clvm, custodyHash: Uint8Array): Vault {
  const p2 = sim.bls(1n);

  const { vault, parentConditions } = clvm.mintVault(
    p2.coin.coinId(),
    custodyHash,
    clvm.nil()
  );

  const spend = clvm.standardSpend(
    p2.pk,
    clvm.delegatedSpend(parentConditions)
  );

  clvm.spendCoin(p2.coin, spend);

  sim.spendCoins(clvm.coinSpends(), [p2.sk]);

  return vault;
}

test("non-vault MIPS spend", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const p2 = sim.bls(1n);

  const config = new MemberConfig().withTopLevel(true);
  const puzzleHash = blsMemberHash(config, p2.pk, false);

  const spend1 = clvm.standardSpend(
    p2.pk,
    clvm.delegatedSpend([clvm.createCoin(puzzleHash, 1n, null)])
  );

  const coin: Coin = new Coin(p2.coin.coinId(), puzzleHash, 1n);

  const mipsSpend = clvm.mipsSpend(
    coin,
    clvm.delegatedSpend([clvm.createCoin(puzzleHash, 1n, null)])
  );

  mipsSpend.blsMember(config, p2.pk, false);
  const spend2 = mipsSpend.spend(puzzleHash);

  clvm.spendCoin(p2.coin, spend1);
  clvm.spendCoin(coin, spend2);

  sim.spendCoins(clvm.coinSpends(), [p2.sk]);

  t.true(true);
});

function mintVaultWithCoin(
  sim: Simulator,
  clvm: Clvm,
  custodyHash: Uint8Array,
  amount: bigint
): [Vault, Coin] {
  const p2 = sim.bls(amount + 1n);

  const { vault, parentConditions } = clvm.mintVault(
    p2.coin.coinId(),
    custodyHash,
    clvm.nil()
  );

  const p2PuzzleHash = singletonMemberHash(
    new MemberConfig().withTopLevel(true),
    vault.info.launcherId,
    false
  );

  const spend = clvm.standardSpend(
    p2.pk,
    clvm.delegatedSpend([
      ...parentConditions,
      clvm.createCoin(
        p2PuzzleHash,
        amount,
        clvm.alloc([vault.info.launcherId])
      ),
    ])
  );

  clvm.spendCoin(p2.coin, spend);

  sim.spendCoins(clvm.coinSpends(), [p2.sk]);

  return [vault, new Coin(p2.coin.coinId(), p2PuzzleHash, amount)];
}

function signK1(
  sk: K1SecretKey,
  vault: Vault,
  delegatedPuzzleHash: Uint8Array,
  fastForward: boolean
): K1Signature {
  return sk.signPrehashed(
    sha256(
      Uint8Array.from([
        ...delegatedPuzzleHash,
        ...(fastForward ? vault.coin.puzzleHash : vault.coin.coinId()),
      ])
    )
  );
}
