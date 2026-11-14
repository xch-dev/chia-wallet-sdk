import test from "ava";

import {
  AggSigConstants,
  BlsPairWithCoin,
  bytesEqual,
  Clvm,
  MempoolItemsAdded,
  MempoolItemsRemoved,
  MempoolRemoveReason,
  Peer,
  PeerOptions,
  PeerSimulator,
  RemovedMempoolItem,
  Signature,
  SpendBundle,
} from "../index.js";

// Spends the coin to a new coin of `amount` with the same puzzle hash, and signs it with the
// required signatures. The peer simulator uses testnet11 constants.
function spendCoin(alice: BlsPairWithCoin, amount: bigint): SpendBundle {
  const clvm = new Clvm();
  const delegated = clvm.delegatedSpend([
    clvm.createCoin(alice.puzzleHash, amount),
  ]);
  clvm.spendStandardCoin(alice.coin, alice.pk, delegated);

  const coinSpends = clvm.coinSpends();
  const required = clvm.requiredSignatures(
    coinSpends,
    AggSigConstants.testnet11(),
  );
  const signatures = required.bls.map((item) => alice.sk.sign(item.message()));

  return new SpendBundle(coinSpends, Signature.aggregate(signatures));
}

async function nextCoinStateUpdate(peer: Peer) {
  while (true) {
    const event = await peer.next();
    if (!event) throw new Error("connection closed before a coin state update");
    if (event.coinStateUpdate) return event.coinStateUpdate;
  }
}

test("sends a transaction and reads its result through the peer", async (t) => {
  const sim = await PeerSimulator.start();
  const peer = await sim.connect();

  t.is(peer.socketAddr(), sim.socketAddr());

  const alice = await sim.bls(1000n);
  const coinId = alice.coin.coinId();

  const coinUpdates = await peer.registerForCoinUpdates([coinId], 0);
  t.is(coinUpdates.coinStates.length, 1);
  t.true(bytesEqual(coinUpdates.coinStates[0].coin.coinId(), coinId));
  t.is(coinUpdates.minHeight, 0);

  const phUpdates = await peer.registerForPhUpdates([alice.puzzleHash], 0);
  t.is(phUpdates.coinStates.length, 1);
  t.true(bytesEqual(phUpdates.puzzleHashes[0], alice.puzzleHash));

  const spendBundle = spendCoin(alice, 900n);
  const ack = await peer.sendTransaction(spendBundle);
  t.is(ack.status, 1);
  t.falsy(ack.error);
  t.true(bytesEqual(ack.txid, spendBundle.hash()));
  t.is(await sim.height(), 1);

  const update = await nextCoinStateUpdate(peer);
  t.is(update.height, 1);
  t.true(bytesEqual(update.peakHash, await sim.headerHash()));
  const spent = update.items.find((item) =>
    bytesEqual(item.coin.coinId(), coinId),
  );
  t.not(spent?.spentHeight, undefined);
  t.not(spent?.spentHeight, null);
  t.is(spent?.spentHeight, (await sim.coinState(coinId))?.spentHeight);

  const children = await peer.requestChildren(coinId);
  t.is(children.coinStates.length, 1);
  t.is(children.coinStates[0].coin.amount, 900n);
  t.true(bytesEqual(children.coinStates[0].coin.parentCoinInfo, coinId));

  const peers = await peer.requestPeers();
  t.deepEqual(peers.peerList, []);
});

async function nextMatching<T>(
  peer: Peer,
  pick: (event: NonNullable<Awaited<ReturnType<Peer["next"]>>>) => T | undefined,
): Promise<T> {
  while (true) {
    const event = await peer.next();
    if (!event) throw new Error("connection closed before the expected event");
    const value = pick(event);
    if (value) return value;
  }
}

test("waits in the mempool until a block is farmed", async (t) => {
  const sim = await PeerSimulator.startWithMempool();
  const peer = await sim.connectWithMempoolUpdates();

  const alice = await sim.bls(1000n);
  const coinId = alice.coin.coinId();
  await peer.registerForPhUpdates([alice.puzzleHash], 0);

  const height = await sim.height();
  const spendBundle = spendCoin(alice, 900n);
  const transactionId = spendBundle.hash();
  t.is((await peer.sendTransaction(spendBundle)).status, 1);

  const added = await nextMatching(peer, (event) => event.mempoolItemsAdded);
  t.true(bytesEqual(added.transactionIds[0], transactionId));

  // The transaction is pending, so the coin isn't spent and no block was made.
  t.is(await sim.height(), height);
  const pending = await sim.coinState(coinId);
  t.truthy(pending);
  t.falsy(pending?.spentHeight);

  const transaction = await peer.requestTransaction(transactionId);
  t.true(bytesEqual(transaction.transaction.hash(), transactionId));

  const records = await sim.farmBlock(1);
  t.is(records.length, 1);
  t.is(await sim.height(), height + 1);

  const removed = await nextMatching(peer, (event) => event.mempoolItemsRemoved);
  t.true(bytesEqual(removed.removedItems[0].transactionId, transactionId));
  t.is(removed.removedItems[0].reason, MempoolRemoveReason.BlockInclusion);

  t.is((await sim.coinState(coinId))?.spentHeight, height + 1);
});

test("only farms blocks in mempool mode", async (t) => {
  const sim = await PeerSimulator.start();
  await t.throwsAsync(() => sim.farmBlock(1), { message: /mempool mode/ });
});

test("returns a rejected transaction ack instead of throwing", async (t) => {
  const sim = await PeerSimulator.start();
  const peer = await sim.connect();

  const alice = await sim.bls(1000n);
  const spendBundle = spendCoin(alice, 1000n);

  t.is((await peer.sendTransaction(spendBundle)).status, 1);

  // The coin has already been spent.
  const ack = await peer.sendTransaction(spendBundle);
  t.is(ack.status, 3);
  t.truthy(ack.error);
});

test("fails requests after the peer is closed", async (t) => {
  const sim = await PeerSimulator.start();
  const peer = await sim.connect();

  await peer.close();

  await t.throwsAsync(peer.requestPeers());
  await t.throwsAsync(peer.requestChildren(new Uint8Array(32)));
  t.is(await peer.next(), null);
});

test("mempool updates are off by default and can be enabled", (t) => {
  const options = new PeerOptions();
  t.false(options.mempoolUpdates);
  t.is(options.rateLimitFactor, 0.6);

  options.mempoolUpdates = true;
  t.true(options.mempoolUpdates);
  t.true(options.clone().mempoolUpdates);
});

test("mempool update messages roundtrip", (t) => {
  const first = new Uint8Array(32).fill(1);
  const second = new Uint8Array(32).fill(2);

  const added = new MempoolItemsAdded([first, second]);
  t.deepEqual(
    added.transactionIds.map((id) => Uint8Array.from(id)),
    [first, second],
  );

  const reasons = [
    MempoolRemoveReason.Conflict,
    MempoolRemoveReason.BlockInclusion,
    MempoolRemoveReason.PoolFull,
    MempoolRemoveReason.Expired,
  ];

  const removed = new MempoolItemsRemoved(
    reasons.map((reason) => new RemovedMempoolItem(first, reason)),
  );

  t.deepEqual(
    removed.removedItems.map((item) => item.reason),
    reasons,
  );
  t.deepEqual(Uint8Array.from(removed.removedItems[0].transactionId), first);
});
