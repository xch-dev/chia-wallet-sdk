import test from "ava";

import {
  childVault,
  ClvmAllocator,
  force1Of2RestrictedVariable,
  k1MemberHash,
  K1SecretKey,
  K1Signature,
  MemberConfig,
  mOfNHash,
  sha256,
  Simulator,
  Spend,
  toCoinId,
  Vault,
  VaultSpend,
} from "../index.js";

test("single signer vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const k1 = sim.k1Pair(1);

  const config: MemberConfig = {
    topLevel: true,
    nonce: 0,
    restrictions: [],
  };

  const vault = mintVault(sim, clvm, k1MemberHash(config, k1.publicKey, false));

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(clvm, k1.secretKey, vault, delegatedSpend, false);

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendK1(clvm, config, k1.publicKey, signature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("single signer fast forward vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const k1 = sim.k1Pair(1);

  const config: MemberConfig = {
    topLevel: true,
    nonce: 0,
    restrictions: [],
  };

  const vault = mintVault(sim, clvm, k1MemberHash(config, k1.publicKey, true));

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(clvm, k1.secretKey, vault, delegatedSpend, true);

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendK1(clvm, config, k1.publicKey, signature, true);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("1 of 2 vault (path 1)", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const alice = sim.k1Pair(1);
  const bob = sim.k1Pair(2);

  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const aliceHash = k1MemberHash(config, alice.publicKey, false);
  const bobHash = k1MemberHash(config, bob.publicKey, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash({ ...config, topLevel: true }, 1, [aliceHash, bobHash])
  );

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(clvm, alice.secretKey, vault, delegatedSpend, false);

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [aliceHash, bobHash]);
  vaultSpend.spendK1(clvm, config, alice.publicKey, signature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("1 of 2 vault (path 2)", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const alice = sim.k1Pair(1);
  const bob = sim.k1Pair(2);

  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const aliceHash = k1MemberHash(config, alice.publicKey, false);
  const bobHash = k1MemberHash(config, bob.publicKey, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash({ ...config, topLevel: true }, 1, [aliceHash, bobHash])
  );

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(clvm, bob.secretKey, vault, delegatedSpend, false);

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [aliceHash, bobHash]);
  vaultSpend.spendK1(clvm, config, bob.publicKey, signature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("2 of 2 vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const alice = sim.k1Pair(1);
  const bob = sim.k1Pair(2);

  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const aliceHash = k1MemberHash(config, alice.publicKey, false);
  const bobHash = k1MemberHash(config, bob.publicKey, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash({ ...config, topLevel: true }, 2, [aliceHash, bobHash])
  );

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const aliceSignature = signK1(
    clvm,
    alice.secretKey,
    vault,
    delegatedSpend,
    false
  );
  const bobSignature = signK1(
    clvm,
    bob.secretKey,
    vault,
    delegatedSpend,
    false
  );

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 2, [aliceHash, bobHash]);
  vaultSpend.spendK1(clvm, config, alice.publicKey, aliceSignature, false);
  vaultSpend.spendK1(clvm, config, bob.publicKey, bobSignature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("2 of 3 vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const alice = sim.k1Pair(1);
  const bob = sim.k1Pair(2);
  const charlie = sim.k1Pair(3);

  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const aliceHash = k1MemberHash(config, alice.publicKey, false);
  const bobHash = k1MemberHash(config, bob.publicKey, false);
  const charlieHash = k1MemberHash(config, charlie.publicKey, false);

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash({ ...config, topLevel: true }, 2, [
      aliceHash,
      bobHash,
      charlieHash,
    ])
  );

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const aliceSignature = signK1(
    clvm,
    alice.secretKey,
    vault,
    delegatedSpend,
    false
  );
  const bobSignature = signK1(
    clvm,
    bob.secretKey,
    vault,
    delegatedSpend,
    false
  );

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 2, [
    aliceHash,
    bobHash,
    charlieHash,
  ]);
  vaultSpend.spendK1(clvm, config, alice.publicKey, aliceSignature, false);
  vaultSpend.spendK1(clvm, config, bob.publicKey, bobSignature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("fast forward paths vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const alice = sim.k1Pair(1);
  const bob = sim.k1Pair(2);

  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const aliceRegularHash = k1MemberHash(config, alice.publicKey, false);
  const aliceFastForwardHash = k1MemberHash(config, alice.publicKey, true);
  const bobRegularHash = k1MemberHash(config, bob.publicKey, false);
  const bobFastForwardHash = k1MemberHash(config, bob.publicKey, true);

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
    mOfNHash({ ...config, topLevel: true }, 1, [
      regularPathHash,
      fastForwardPathHash,
    ])
  );

  for (const fastForward of [false, true, false, true]) {
    const delegatedSpend = clvm.delegatedSpendForConditions([
      clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
    ]);

    const aliceSignature = signK1(
      clvm,
      alice.secretKey,
      vault,
      delegatedSpend,
      fastForward
    );

    const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
    vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [
      regularPathHash,
      fastForwardPathHash,
    ]);
    vaultSpend.spendMOfN(
      config,
      1,
      fastForward
        ? [aliceFastForwardHash, bobFastForwardHash]
        : [aliceRegularHash, bobRegularHash]
    );
    vaultSpend.spendK1(
      clvm,
      config,
      alice.publicKey,
      aliceSignature,
      fastForward
    );
    clvm.spendVault(vault, vaultSpend);

    sim.spend(clvm.coinSpends(), []);

    vault = childVault(vault, vault.custodyHash);
  }

  t.true(true);
});

test("single signer recovery vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const k1 = sim.k1Pair(1);
  const recovery = sim.k1Pair(2);

  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const recoveryPathHash = k1MemberHash(config, recovery.publicKey, false);
  const memberHash = k1MemberHash(config, k1.publicKey, false);

  const topLevelConfig: MemberConfig = {
    topLevel: true,
    nonce: 0,
    restrictions: [
      force1Of2RestrictedVariable(
        recoveryPathHash,
        0,
        clvm.nil().treeHash(),
        clvm.nil().treeHash()
      ),
    ],
  };

  const vault = mintVault(
    sim,
    clvm,
    mOfNHash(topLevelConfig, 1, [recoveryPathHash, memberHash])
  );

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const signature = signK1(clvm, k1.secretKey, vault, delegatedSpend, false);

  const vaultSpend = new VaultSpend(delegatedSpend, vault.coin);
  vaultSpend.spendForce1Of2RestrictedVariable(
    clvm,
    recoveryPathHash,
    0,
    clvm.nil().treeHash(),
    clvm.nil().treeHash(),
    memberHash
  );
  vaultSpend.spendMOfN(topLevelConfig, 1, [recoveryPathHash, memberHash]);
  vaultSpend.spendK1(clvm, config, k1.publicKey, signature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

function mintVault(
  sim: Simulator,
  clvm: ClvmAllocator,
  custodyHash: Uint8Array
): Vault {
  const p2 = sim.newP2(1n);

  const { vault, parentConditions } = clvm.mintVault(
    toCoinId(p2.coin),
    custodyHash,
    clvm.nil()
  );

  const spend = clvm.spendP2Standard(
    p2.publicKey,
    clvm.delegatedSpendForConditions(parentConditions)
  );

  sim.spend(
    [
      {
        coin: p2.coin,
        puzzleReveal: spend.puzzle.serialize(),
        solution: spend.solution.serialize(),
      },
      ...clvm.coinSpends(),
    ],
    [p2.secretKey]
  );

  return vault;
}

function signK1(
  clvm: ClvmAllocator,
  sk: K1SecretKey,
  vault: Vault,
  delegatedSpend: Spend,
  fastForward: boolean
): K1Signature {
  return sk.signPrehashed(
    sha256(
      Uint8Array.from([
        ...clvm.treeHash(delegatedSpend.puzzle),
        ...(fastForward ? vault.coin.puzzleHash : toCoinId(vault.coin)),
      ])
    )
  );
}
