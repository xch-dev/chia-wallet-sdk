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
  fromHex,
  k1MemberHash,
  K1Pair,
  MemberConfig,
  mOfNHash,
  parseVaultTransaction,
  preventVaultSideEffectsRestriction,
  Reveals,
  sha256,
  Simulator,
  singletonMemberHash,
  SpendBundle,
  timelockRestriction,
  treeHashPair,
  Vault,
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

test("signs the start recovery message", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const custodyKey = K1Pair.fromSeed(1n);
  const recoveryKey = sim.bls(0n);

  const config = new MemberConfig();
  const memberHash = k1MemberHash(config, custodyKey.pk, false);

  const recoveryTimelock = 1n;
  const timelock = timelockRestriction(recoveryTimelock);
  const recoveryRestrictions = [
    force1Of2Restriction(
      memberHash,
      0,
      treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
      clvm.nil().treeHash(),
    ),
    ...preventVaultSideEffectsRestriction(),
  ];
  const recoveryHash = blsMemberHash(
    config.withRestrictions(recoveryRestrictions),
    recoveryKey.pk,
    false,
  );

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
  ]);

  const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
  mips.mOfN(config.withTopLevel(true), 1, [memberHash, recoveryHash]);
  mips.blsMember(
    config.withRestrictions(recoveryRestrictions),
    recoveryKey.pk,
    false,
  );
  mips.preventVaultSideEffects();
  mips.force1Of2RestrictedVariable(
    memberHash,
    0,
    treeHashPair(timelock.puzzleHash, clvm.nil().treeHash()),
    clvm.nil().treeHash(),
    recoveryFinishMemberSpend.puzzle.treeHash(),
  );
  mips.spendVault(vault);

  const message = calculateVaultStartRecoveryMessage(
    delegatedSpend.puzzle.treeHash(),
    memberHash,
    recoveryTimelock,
    vault.coin.coinId(),
    GENESIS_CHALLENGE,
  );

  t.notThrows(() =>
    sim.newTransaction(
      new SpendBundle(clvm.coinSpends(), recoveryKey.sk.sign(message)),
    ),
  );
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
