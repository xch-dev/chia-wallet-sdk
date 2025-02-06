import test from "ava";

import {
  blsMemberHash,
  childVault,
  ClvmAllocator,
  Coin,
  customMemberHash,
  force1Of2Restriction,
  k1MemberHash,
  K1SecretKey,
  K1Signature,
  MemberConfig,
  MipsSpend,
  mOfNHash,
  passkeyMemberHash,
  preventConditionOpcodeRestriction,
  preventMultipleCreateCoinsRestriction,
  sha256,
  Simulator,
  singletonMemberHash,
  Spend,
  timelockRestriction,
  toCoinId,
  treeHashPair,
  Vault,
  wrappedDelegatedPuzzleHash,
} from "../index.js";

test("bls key vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const alice = sim.newP2(0n);

  const config: MemberConfig = {
    topLevel: true,
    nonce: 0,
    restrictions: [],
  };

  const [vault, coin] = mintVaultWithCoin(
    sim,
    clvm,
    blsMemberHash(config, alice.publicKey),
    1n
  );

  const coinDelegatedSpend = clvm.delegatedSpendForConditions([
    clvm.reserveFee(1n),
  ]);

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
    clvm.sendMessage(23, coinDelegatedSpend.puzzle.treeHash(), [
      clvm.alloc(toCoinId(coin)),
    ]),
  ]);

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
  vaultSpend.spendBls(clvm, config, alice.publicKey);
  clvm.spendVault(vault, vaultSpend);

  const p2Spend = new MipsSpend(coinDelegatedSpend, coin);
  p2Spend.spendSingleton(
    clvm,
    { topLevel: true, nonce: 0, restrictions: [] },
    vault.launcherId,
    vault.custodyHash,
    vault.coin.amount
  );

  const spend = p2Spend.spend(clvm, coin.puzzleHash);

  sim.spend(
    [
      ...clvm.coinSpends(),
      {
        coin,
        puzzleReveal: spend.puzzle.serialize(),
        solution: spend.solution.serialize(),
      },
    ],
    [alice.secretKey]
  );

  t.true(true);
});

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

  const signature = signK1(
    k1.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
  vaultSpend.spendK1(clvm, config, k1.publicKey, signature, false);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  t.true(true);
});

test("passkey member vault", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const r1 = sim.r1Pair(1);

  const config: MemberConfig = {
    topLevel: true,
    nonce: 0,
    restrictions: [],
  };

  const fastForward = false;

  const vault = mintVault(
    sim,
    clvm,
    passkeyMemberHash(config, r1.publicKey, fastForward)
  );

  const delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  const challengeIndex = 23;
  const originalMessage = Buffer.from(
    sha256(
      Buffer.concat([
        Buffer.from(delegatedSpend.puzzle.treeHash()),
        fastForward ? vault.coin.puzzleHash : toCoinId(vault.coin),
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

  const signature = r1.secretKey.signPrehashed(message);

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
  vaultSpend.spendPasskey(
    clvm,
    config,
    r1.publicKey,
    signature,
    authenticatorData,
    clientDataJSON,
    challengeIndex,
    fastForward
  );
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

  const signature = signK1(
    k1.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    true
  );

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
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

  const signature = signK1(
    alice.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
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

  const signature = signK1(
    bob.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
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
    alice.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );
  const bobSignature = signK1(
    bob.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
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
    alice.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );
  const bobSignature = signK1(
    bob.secretKey,
    vault,
    delegatedSpend.puzzle.treeHash(),
    false
  );

  const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
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
      alice.secretKey,
      vault,
      delegatedSpend.puzzle.treeHash(),
      fastForward
    );

    const vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
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

  const custodyKey = sim.k1Pair(1);
  const recoveryKey = sim.k1Pair(2);

  // Initial vault
  const config: MemberConfig = {
    topLevel: false,
    nonce: 0,
    restrictions: [],
  };

  const memberHash = k1MemberHash(config, custodyKey.publicKey, false);

  const timelock = timelockRestriction(1n);
  const preventedConditions = [60, 62, 66, 67];
  const recoveryRestrictions = [
    force1Of2Restriction(
      memberHash,
      0,
      treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
      clvm.nil().treeHash()
    ),
    ...preventedConditions.map(preventConditionOpcodeRestriction),
    preventMultipleCreateCoinsRestriction(),
  ];
  const initialRecoveryHash = k1MemberHash(
    {
      ...config,
      restrictions: recoveryRestrictions,
    },
    recoveryKey.publicKey,
    false
  );

  let vault = mintVault(
    sim,
    clvm,
    mOfNHash({ ...config, topLevel: true }, 1, [
      memberHash,
      initialRecoveryHash,
    ])
  );

  let delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);

  let vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [
    memberHash,
    initialRecoveryHash,
  ]);
  vaultSpend.spendK1(
    clvm,
    config,
    custodyKey.publicKey,
    signK1(
      custodyKey.secretKey,
      vault,
      delegatedSpend.puzzle.treeHash(),
      false
    ),
    false
  );
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  // Initiate recovery
  const oldCustodyHash = vault.custodyHash;
  const recoveryDelegatedSpend: Spend = {
    puzzle: clvm.nil(),
    solution: clvm.nil(),
  };

  const recoveryFinishMemberSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(oldCustodyHash, vault.coin.amount, null),
    clvm.assertSecondsRelative(1n),
  ]);
  const recoveryFinishMemberHash = customMemberHash(
    { ...config, restrictions: [timelock] },
    recoveryFinishMemberSpend.puzzle.treeHash()
  );

  const custodyHash = mOfNHash({ ...config, topLevel: true }, 1, [
    memberHash,
    recoveryFinishMemberHash,
  ]);

  delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(custodyHash, vault.coin.amount, null),
  ]);

  vault = childVault(vault, vault.custodyHash);
  vaultSpend = new MipsSpend(delegatedSpend, vault.coin);

  vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [
    memberHash,
    initialRecoveryHash,
  ]);

  vaultSpend.spendK1(
    clvm,
    { ...config, restrictions: recoveryRestrictions },
    recoveryKey.publicKey,
    signK1(
      recoveryKey.secretKey,
      vault,
      wrappedDelegatedPuzzleHash(
        recoveryRestrictions,
        delegatedSpend.puzzle.treeHash()
      ),
      false
    ),
    false
  );

  vaultSpend.spendPreventMultipleCreateCoins(clvm);

  for (const condition of preventedConditions.reverse()) {
    vaultSpend.spendPreventConditionOpcode(clvm, condition);
  }

  vaultSpend.spendForce1Of2Restriction(
    clvm,
    memberHash,
    0,
    treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
    clvm.nil().treeHash(),
    recoveryFinishMemberSpend.puzzle.treeHash()
  );

  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  // Finish recovery
  vault = childVault(vault, custodyHash);
  vaultSpend = new MipsSpend(recoveryDelegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [
    memberHash,
    recoveryFinishMemberHash,
  ]);
  vaultSpend.spendCustomMember(
    clvm,
    { ...config, restrictions: [timelock] },
    recoveryFinishMemberSpend
  );
  vaultSpend.spendTimelockRestriction(clvm, 1n);
  clvm.spendVault(vault, vaultSpend);

  sim.spend(clvm.coinSpends(), []);

  // Make sure the vault is spendable after recovery
  vault = childVault(vault, oldCustodyHash);
  delegatedSpend = clvm.delegatedSpendForConditions([
    clvm.createCoin(vault.custodyHash, vault.coin.amount, null),
  ]);
  vaultSpend = new MipsSpend(delegatedSpend, vault.coin);
  vaultSpend.spendMOfN({ ...config, topLevel: true }, 1, [
    memberHash,
    initialRecoveryHash,
  ]);
  vaultSpend.spendK1(
    clvm,
    config,
    custodyKey.publicKey,
    signK1(
      custodyKey.secretKey,
      vault,
      delegatedSpend.puzzle.treeHash(),
      false
    ),
    false
  );
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

test("non-vault MIPS spend", (t) => {
  const sim = new Simulator();
  const clvm = new ClvmAllocator();

  const p2 = sim.newP2(1n);

  const config: MemberConfig = {
    topLevel: true,
    nonce: 0,
    restrictions: [],
  };
  const puzzleHash = blsMemberHash(config, p2.publicKey);

  const spend1 = clvm.spendP2Standard(
    p2.publicKey,
    clvm.delegatedSpendForConditions([clvm.createCoin(puzzleHash, 1n, null)])
  );

  const coin: Coin = {
    parentCoinInfo: toCoinId(p2.coin),
    puzzleHash,
    amount: 1n,
  };

  const mipsSpend = new MipsSpend(
    clvm.delegatedSpendForConditions([
      clvm.createCoin(p2.puzzleHash, 1n, null),
    ]),
    coin
  );

  mipsSpend.spendBls(clvm, config, p2.publicKey);
  const spend2 = mipsSpend.spend(clvm, puzzleHash);

  sim.spend(
    [
      ...clvm.coinSpends(),
      {
        coin: p2.coin,
        puzzleReveal: spend1.puzzle.serialize(),
        solution: spend1.solution.serialize(),
      },
      {
        coin,
        puzzleReveal: spend2.puzzle.serialize(),
        solution: spend2.solution.serialize(),
      },
    ],
    [p2.secretKey]
  );

  t.true(true);
});

function mintVaultWithCoin(
  sim: Simulator,
  clvm: ClvmAllocator,
  custodyHash: Uint8Array,
  amount: bigint
): [Vault, Coin] {
  const p2 = sim.newP2(amount + 1n);

  const { vault, parentConditions } = clvm.mintVault(
    toCoinId(p2.coin),
    custodyHash,
    clvm.nil()
  );

  const p2PuzzleHash = singletonMemberHash(
    { topLevel: true, nonce: 0, restrictions: [] },
    vault.launcherId
  );

  const spend = clvm.spendP2Standard(
    p2.publicKey,
    clvm.delegatedSpendForConditions([
      ...parentConditions,
      clvm.createCoin(p2PuzzleHash, amount, clvm.alloc([vault.launcherId])),
    ])
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

  return [
    vault,
    {
      parentCoinInfo: toCoinId(p2.coin),
      puzzleHash: p2PuzzleHash,
      amount,
    },
  ];
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
        ...(fastForward ? vault.coin.puzzleHash : toCoinId(vault.coin)),
      ])
    )
  );
}
