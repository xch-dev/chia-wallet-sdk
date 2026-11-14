import test from "ava";

import {
  MempoolItemsAdded,
  MempoolItemsRemoved,
  MempoolRemoveReason,
  PeerOptions,
  RemovedMempoolItem,
} from "../index.js";

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
