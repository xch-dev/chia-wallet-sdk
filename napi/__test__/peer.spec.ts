import test from "ava";

import {
  BlsPairWithCoin,
  bytesEqual,
  Clvm,
  Peer,
  PeerSimulator,
  SpendBundle,
} from "../index.js";

// Spends the coin to a new coin of `amount` with the same puzzle hash, and signs the
// AGG_SIG_ME condition of the standard puzzle. The peer simulator uses testnet11 constants,
// whose AGG_SIG_ME additional data is the genesis challenge.
function spendCoin(
  sim: PeerSimulator,
  alice: BlsPairWithCoin,
  amount: bigint,
): SpendBundle {
  const clvm = new Clvm();
  const delegated = clvm.delegatedSpend([
    clvm.createCoin(alice.puzzleHash, amount),
  ]);
  clvm.spendStandardCoin(alice.coin, alice.pk, delegated);

  const message = Buffer.concat([
    delegated.puzzle.treeHash(),
    alice.coin.coinId(),
    sim.genesisChallenge(),
  ]);

  return new SpendBundle(clvm.coinSpends(), alice.sk.sign(message));
}

async function nextCoinStateUpdate(peer: Peer) {
  while (true) {
    const event = await peer.next();
    if (!event) throw new Error("connection closed before a coin state update");
    if (event.coinStateUpdate) return event.coinStateUpdate;
  }
}

test("sends a transaction and reads it back through the peer", async (t) => {
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

  const spendBundle = spendCoin(sim, alice, 900n);
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

  const transaction = await peer.requestTransaction(spendBundle.hash());
  t.true(bytesEqual(transaction.transaction.hash(), spendBundle.hash()));

  const peers = await peer.requestPeers();
  t.deepEqual(peers.peerList, []);
});

test("returns a rejected transaction ack instead of throwing", async (t) => {
  const sim = await PeerSimulator.start();
  const peer = await sim.connect();

  const alice = await sim.bls(1000n);
  const spendBundle = spendCoin(sim, alice, 1000n);

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
