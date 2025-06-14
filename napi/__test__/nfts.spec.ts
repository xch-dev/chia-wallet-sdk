import test from "ava";

import {
  Clvm,
  Coin,
  Constants,
  CreatedDid,
  NftMint,
  PublicKey,
  Simulator,
  standardPuzzleHash,
} from "../index.js";

test("mints and transfers an nft", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(2n);

  // Create a DID
  const { did, parentConditions: didParentConditions } = createDid(
    clvm,
    alice.coin.coinId(),
    alice.pk
  );

  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend(
      didParentConditions.concat([clvm.createCoin(alice.puzzleHash, 0n)])
    )
  );

  // Mint an NFT
  const mintCoin = new Coin(alice.coin.coinId(), alice.puzzleHash, 0n);

  const {
    nfts: [nft],
    parentConditions: mintParentConditions,
  } = clvm.mintNfts(mintCoin.coinId(), [
    new NftMint(
      clvm.nil(),
      Constants.nftMetadataUpdaterDefaultHash(),
      alice.puzzleHash,
      alice.puzzleHash,
      300,
      null
    ),
  ]);

  clvm.spendStandardCoin(
    mintCoin,
    alice.pk,
    clvm.delegatedSpend(mintParentConditions)
  );

  // Assign the NFT to the DID by spending both
  clvm.spendNft(
    nft,
    clvm.standardSpend(
      alice.pk,
      clvm.delegatedSpend([
        clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
        clvm.transferNft(did.info.launcherId, [], did.info.innerPuzzleHash()),
      ])
    )
  );

  clvm.spendDid(
    did,
    clvm.standardSpend(
      alice.pk,
      clvm.delegatedSpend([
        clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
        clvm.createPuzzleAnnouncement(nft.info.launcherId),
      ])
    )
  );

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(true);
});

function createDid(
  clvm: Clvm,
  parentCoinId: Buffer,
  pk: PublicKey
): CreatedDid {
  const p2PuzzleHash = standardPuzzleHash(pk);
  const eveDid = clvm.createEveDid(parentCoinId, p2PuzzleHash);

  clvm.spendDid(
    eveDid.did,
    clvm.standardSpend(
      pk,
      clvm.delegatedSpend([
        clvm.createCoin(
          eveDid.did.info.innerPuzzleHash(),
          1n,
          clvm.alloc([p2PuzzleHash])
        ),
      ])
    )
  );

  return new CreatedDid(
    eveDid.did.child(p2PuzzleHash, eveDid.did.info.metadata),
    eveDid.parentConditions
  );
}
