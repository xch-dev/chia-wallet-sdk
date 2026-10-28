import test from "ava";

import {
  AssetInfo,
  CatAssetInfo,
  Clvm,
  NftAssetInfo,
  OptionAssetInfo,
} from "../index.js";

test("round trips asset info", (t) => {
  const clvm = new Clvm();
  const assetInfo = new AssetInfo(clvm);

  const assetId = Buffer.alloc(32, 1);
  const launcherId = Buffer.alloc(32, 2);
  const optionId = Buffer.alloc(32, 3);

  assetInfo.insertCat(assetId, new CatAssetInfo(Buffer.alloc(32, 4)));
  assetInfo.insertNft(
    launcherId,
    new NftAssetInfo(
      clvm.list([clvm.int(1n)]),
      Buffer.alloc(32, 5),
      Buffer.alloc(32, 6),
      300,
    ),
  );
  assetInfo.insertOption(
    optionId,
    new OptionAssetInfo(Buffer.alloc(32, 7), Buffer.alloc(32, 8)),
  );

  t.deepEqual(assetInfo.catAssetIds(), [assetId]);
  t.deepEqual(assetInfo.nftLauncherIds(), [launcherId]);
  t.deepEqual(assetInfo.optionLauncherIds(), [optionId]);

  t.deepEqual(assetInfo.cat(assetId)?.hiddenPuzzleHash, Buffer.alloc(32, 4));

  const nft = assetInfo.nft(launcherId);
  t.deepEqual(nft?.metadata.treeHash(), clvm.list([clvm.int(1n)]).treeHash());
  t.deepEqual(nft?.metadataUpdaterPuzzleHash, Buffer.alloc(32, 5));
  t.deepEqual(nft?.royaltyPuzzleHash, Buffer.alloc(32, 6));
  t.is(nft?.royaltyBasisPoints, 300);

  const option = assetInfo.option(optionId);
  t.deepEqual(option?.underlyingCoinId, Buffer.alloc(32, 7));
  t.deepEqual(option?.underlyingDelegatedPuzzleHash, Buffer.alloc(32, 8));

  t.is(assetInfo.cat(launcherId), null);
  t.is(assetInfo.nft(assetId), null);
  t.is(assetInfo.option(assetId), null);
});
