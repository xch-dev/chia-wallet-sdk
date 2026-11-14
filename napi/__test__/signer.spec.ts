import test from "ava";

import {
  AggSigConstants,
  Clvm,
  k1MemberHash,
  K1Pair,
  MemberConfig,
  sha256,
  Signature,
  Simulator,
  SpendBundle,
  toHex,
} from "../index.js";

test("sign a standard spend with the required signatures", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);
  const bob = sim.bls(2n);

  clvm.spendCoin(
    alice.coin,
    clvm.standardSpend(
      alice.pk,
      clvm.delegatedSpend([clvm.createCoin(alice.puzzleHash, 1n, null)]),
    ),
  );
  clvm.spendCoin(
    bob.coin,
    clvm.standardSpend(
      bob.pk,
      clvm.delegatedSpend([clvm.createCoin(bob.puzzleHash, 2n, null)]),
    ),
  );

  const coinSpends = clvm.coinSpends();
  const constants = AggSigConstants.testnet11();
  const required = clvm.requiredSignatures(coinSpends, constants);

  t.is(required.bls.length, 2);
  t.is(required.secp.length, 0);

  const [first, second] = required.bls;
  t.is(toHex(first.coinId), toHex(alice.coin.coinId()));
  t.is(toHex(second.coinId), toHex(bob.coin.coinId()));
  t.is(toHex(first.domainString!), toHex(constants.me()));

  // AGG_SIG_ME signs the raw message, the coin id, and the domain string.
  t.is(
    toHex(first.message()),
    toHex(
      Uint8Array.from([
        ...first.rawMessage,
        ...alice.coin.coinId(),
        ...constants.me(),
      ]),
    ),
  );

  const keys = [alice, bob];
  const signatures = required.bls.map((item) => {
    const pair = keys.find(
      (key) => toHex(key.pk.toBytes()) === toHex(item.publicKey.toBytes()),
    );
    if (!pair) throw new Error("unknown public key");
    return pair.sk.sign(item.message());
  });

  sim.newTransaction(
    new SpendBundle(coinSpends, Signature.aggregate(signatures)),
  );

  t.is(sim.unspentCoins(alice.puzzleHash, false).length, 1);
  t.is(sim.unspentCoins(bob.puzzleHash, false).length, 1);
});

test("sign a k1 vault spend with the required signatures", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const k1 = K1Pair.fromSeed(1n);
  const config = new MemberConfig().withTopLevel(true);

  // Mint the vault, signing the parent coin with the required signatures.
  const p2 = sim.bls(1n);
  const { vault, parentConditions } = clvm.mintVault(
    p2.coin.coinId(),
    k1MemberHash(config, k1.pk, false),
    clvm.nil(),
  );
  clvm.spendCoin(
    p2.coin,
    clvm.standardSpend(p2.pk, clvm.delegatedSpend(parentConditions)),
  );
  const mintSpends = clvm.coinSpends();
  const mintRequired = clvm.requiredSignatures(
    mintSpends,
    AggSigConstants.testnet11(),
  );
  sim.newTransaction(
    new SpendBundle(
      mintSpends,
      Signature.aggregate(
        mintRequired.bls.map((item) => p2.sk.sign(item.message())),
      ),
    ),
  );

  const spendVault = (signature: ReturnType<typeof k1.sk.signPrehashed>) => {
    const delegatedSpend = clvm.delegatedSpend([
      clvm.createCoin(vault.info.custodyHash, vault.coin.amount, null),
    ]);
    const mips = clvm.mipsSpend(vault.coin, delegatedSpend);
    mips.k1Member(config, k1.pk, signature, false);
    mips.spendVault(vault);
    return { coinSpends: clvm.coinSpends(), delegatedSpend };
  };

  // Calculate the required signatures with a placeholder signature in the solution.
  const placeholder = K1Pair.fromSeed(2n).sk.signPrehashed(new Uint8Array(32));
  const { coinSpends: unsigned, delegatedSpend } = spendVault(placeholder);

  const required = clvm.requiredSignatures(
    unsigned,
    AggSigConstants.testnet11(),
  );

  t.is(required.bls.length, 0);
  t.is(required.secp.length, 1);

  const [secp] = required.secp;
  t.is(toHex(secp.coinId), toHex(vault.coin.coinId()));
  t.is(secp.r1PublicKey, null);
  t.is(toHex(secp.k1PublicKey!.toBytes()), toHex(k1.pk.toBytes()));
  t.is(toHex(secp.placeholder.toAtom()!), toHex(placeholder.toBytes()));
  t.is(
    toHex(secp.messageHash),
    toHex(
      sha256(
        Uint8Array.from([
          ...delegatedSpend.puzzle.treeHash(),
          ...vault.coin.coinId(),
        ]),
      ),
    ),
  );

  // The placeholder signature fails, but the real signature passes.
  t.throws(() =>
    sim.newTransaction(new SpendBundle(unsigned, Signature.infinity())),
  );

  const { coinSpends: signed } = spendVault(
    k1.sk.signPrehashed(secp.messageHash),
  );
  sim.newTransaction(new SpendBundle(signed, Signature.infinity()));

  t.truthy(sim.coinState(vault.coin.coinId())?.spentHeight);
});

test("a puzzle reveal that doesn't match the coin is rejected", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(1n);
  const bob = sim.bls(1n);

  clvm.spendCoin(
    alice.coin,
    clvm.standardSpend(
      bob.pk,
      clvm.delegatedSpend([clvm.createCoin(alice.puzzleHash, 1n, null)]),
    ),
  );

  t.throws(
    () =>
      clvm.requiredSignatures(
        clvm.coinSpends(),
        AggSigConstants.testnet11(),
      ),
    { message: /does not match coin puzzle hash/ },
  );
});

test("agg sig constants are derived from the agg sig me data", (t) => {
  const mainnet = AggSigConstants.mainnet();
  const derived = new AggSigConstants(mainnet.me());

  t.is(toHex(derived.parent()), toHex(mainnet.parent()));
  t.is(toHex(derived.parentPuzzle()), toHex(mainnet.parentPuzzle()));
  t.not(toHex(mainnet.me()), toHex(AggSigConstants.testnet11().me()));
});
