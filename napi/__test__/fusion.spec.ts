import test from "ava";

import {
  AssetInfo,
  Clvm,
  Constants,
  Fusion,
  fusionHint,
  FusionDirection,
  FusionUnlock,
  Nft,
  NftAssetInfo,
  NftMint,
  NotarizedPayment,
  Offer,
  Payment,
  RequestedPayments,
  Signature,
  Simulator,
  SpendBundle,
} from "../index.js";

function mintNft(
  sim: Simulator,
  clvm: Clvm,
  owner: ReturnType<Simulator["bls"]>,
): Nft {
  const coin = sim.newCoin(owner.puzzleHash, 1n);

  const {
    nfts: [nft],
    parentConditions,
  } = clvm.mintNfts(coin.coinId(), [
    new NftMint(
      clvm.nil(),
      Constants.nftMetadataUpdaterDefaultHash(),
      owner.puzzleHash,
      owner.puzzleHash,
      300,
      null,
    ),
  ]);

  clvm.spendStandardCoin(
    coin,
    owner.pk,
    clvm.delegatedSpend(parentConditions),
  );
  sim.spendCoins(clvm.coinSpends(), [owner.sk]);

  return nft;
}

/// Launches a fusion and locks side A, which belongs to the house.
function setup(sim: Simulator, clvm: Clvm) {
  const house = sim.bls(0n);
  const user = sim.bls(0n);

  const nftA = mintNft(sim, clvm, house);
  const nftB = mintNft(sim, clvm, user);

  const coin = sim.newCoin(house.puzzleHash, 1n);
  const { fusion, parentConditions } = clvm.launchFusion(
    coin.coinId(),
    [nftA.info.launcherId],
    [nftB.info.launcherId],
  );

  clvm.spendStandardCoin(
    coin,
    house.pk,
    clvm.delegatedSpend(parentConditions),
  );

  const locked = clvm.spendNft(
    nftA,
    clvm.standardSpend(
      house.pk,
      clvm.delegatedSpend(fusion.lockNftConditions(clvm, nftA)),
    ),
  );

  sim.spendCoins(clvm.coinSpends(), [house.sk]);

  return { house, user, fusion, nftA, nftB, locked };
}

/// The assertions that make a spend depend on the NFT being paid to the recipient.
function paymentAssertions(
  clvm: Clvm,
  nft: Nft,
  nonce: Uint8Array,
  recipient: Uint8Array,
) {
  const launcherId = nft.info.launcherId;

  const requestedPayments = new RequestedPayments(clvm);
  requestedPayments.addNft(
    launcherId,
    new NotarizedPayment(nonce, [
      new Payment(recipient, 1n, clvm.list([clvm.atom(recipient)])),
    ]),
  );

  const assetInfo = new AssetInfo(clvm);
  assetInfo.insertNft(
    launcherId,
    new NftAssetInfo(
      nft.info.metadata,
      nft.info.metadataUpdaterPuzzleHash,
      nft.info.royaltyPuzzleHash,
      nft.info.royaltyBasisPoints,
    ),
  );

  return requestedPayments.assertions(assetInfo);
}

/// Sends the NFT to the settlement puzzle, with the conditions that assert the requested payments.
function lockSettlement(
  clvm: Clvm,
  owner: ReturnType<Simulator["bls"]>,
  nft: Nft,
  extraConditions: ReturnType<Clvm["nil"]>[] = [],
): Nft {
  return clvm.spendNft(
    nft,
    clvm.standardSpend(
      owner.pk,
      clvm.delegatedSpend([
        clvm.createCoin(Constants.settlementPaymentHash(), 1n),
        clvm.transferNft(null, [], null),
        ...extraConditions,
      ]),
    ),
  );
}

test("fuses and defuses without an offer file", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const { user, fusion, nftB, locked } = setup(sim, clvm);

  t.deepEqual(locked.info.p2PuzzleHash, fusion.p2PuzzleHash());
  t.deepEqual(locked.info.currentOwner, null);

  // The launcher is hinted, and can be parsed to recover the fusion
  const launcherSpend = sim.coinSpend(fusion.launcherId)!;
  const parsedLauncher = Fusion.parseLauncher(
    launcherSpend.coin,
    clvm.deserialize(launcherSpend.solution),
  );
  t.deepEqual(parsedLauncher?.sideA, fusion.sideA);
  t.deepEqual(parsedLauncher?.sideB, fusion.sideB);
  t.deepEqual(parsedLauncher?.coin.coinId(), fusion.coin.coinId());
  t.deepEqual(
    sim.hintedCoins(fusionHint()).map((id) => Buffer.from(id)),
    [Buffer.from(fusion.launcherId)],
  );

  // Fuse side B into side A
  const nonce = new Uint8Array(32).fill(1);
  const settledB = lockSettlement(
    clvm,
    user,
    nftB,
    paymentAssertions(clvm, locked, nonce, user.puzzleHash),
  );
  const fused = fusion.swap(
    clvm,
    FusionDirection.Fuse,
    [settledB],
    [new FusionUnlock(locked, user.puzzleHash)],
    nonce,
  );
  sim.spendCoins(clvm.coinSpends(), [user.sk]);

  t.truthy(sim.coinState(fused.fusion.coin.coinId()));
  t.deepEqual(fused.unlocked[0].info.p2PuzzleHash, user.puzzleHash);
  t.deepEqual(fused.locked[0].info.p2PuzzleHash, fusion.p2PuzzleHash());
  t.truthy(sim.coinState(fused.unlocked[0].coin.coinId()));
  t.truthy(sim.coinState(fused.locked[0].coin.coinId()));

  const spend = sim.coinSpend(fusion.coin.coinId())!;
  const child = Fusion.parseChild(
    spend.coin,
    clvm.deserialize(spend.puzzleReveal),
    clvm.deserialize(spend.solution),
  );
  t.is(child?.direction, FusionDirection.Fuse);
  t.deepEqual(child?.fusion.coin.coinId(), fused.fusion.coin.coinId());

  // Defuse side A to get side B back
  const settledA = lockSettlement(
    clvm,
    user,
    fused.unlocked[0],
    paymentAssertions(clvm, fused.locked[0], nonce, user.puzzleHash),
  );
  const defused = fused.fusion.swap(
    clvm,
    FusionDirection.Defuse,
    [settledA],
    [new FusionUnlock(fused.locked[0], user.puzzleHash)],
    nonce,
  );
  sim.spendCoins(clvm.coinSpends(), [user.sk]);

  t.deepEqual(defused.unlocked[0].info.launcherId, nftB.info.launcherId);
  t.truthy(sim.coinState(defused.unlocked[0].coin.coinId()));
});

test("takes an offer of side B for side A", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const { user, fusion, nftA, nftB, locked } = setup(sim, clvm);

  const launcherId = locked.info.launcherId;
  const requestedPayments = new RequestedPayments(clvm);
  requestedPayments.addNft(
    launcherId,
    new NotarizedPayment(Offer.nonce([nftB.coin.coinId()]), [
      new Payment(
        user.puzzleHash,
        1n,
        clvm.list([clvm.atom(user.puzzleHash)]),
      ),
    ]),
  );

  const assetInfo = new AssetInfo(clvm);
  assetInfo.insertNft(
    launcherId,
    new NftAssetInfo(
      nftA.info.metadata,
      nftA.info.metadataUpdaterPuzzleHash,
      nftA.info.royaltyPuzzleHash,
      nftA.info.royaltyBasisPoints,
    ),
  );

  lockSettlement(clvm, user, nftB, requestedPayments.assertions(assetInfo));

  const offer = Offer.fromInputSpendBundle(
    clvm,
    new SpendBundle(clvm.coinSpends(), Signature.infinity()),
    requestedPayments,
    assetInfo,
  );

  const { fusion: next, unlocked } = fusion.takeOffer(offer, [locked]);

  sim.spendCoins(
    offer.take(new SpendBundle(clvm.coinSpends(), Signature.infinity()))
      .coinSpends,
    [user.sk],
  );

  t.truthy(sim.coinState(next.coin.coinId()));
  t.deepEqual(unlocked[0].info.p2PuzzleHash, user.puzzleHash);
  t.truthy(sim.coinState(unlocked[0].coin.coinId()));
});

test("rejects invalid fusions and swaps", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const { user, fusion, nftB, locked } = setup(sim, clvm);
  const launcherId = locked.info.launcherId;

  t.throws(() => clvm.launchFusion(fusion.launcherId, [], [launcherId]));
  t.throws(() => clvm.launchFusion(fusion.launcherId, [launcherId], []));
  t.throws(() =>
    clvm.launchFusion(fusion.launcherId, [launcherId], [launcherId]),
  );

  // Side B is requested, but it isn't locked
  t.throws(() =>
    fusion.swap(
      clvm,
      FusionDirection.Defuse,
      [locked],
      [new FusionUnlock(nftB, user.puzzleHash)],
      new Uint8Array(32),
    ),
  );

  // An unrelated NFT would be stuck if it was locked
  const other = mintNft(sim, clvm, user);
  t.throws(() => fusion.lockNftConditions(clvm, other));

  t.is(Constants.fusionSingletonHash().length, 32);
  t.is(Constants.p2FusionHash().length, 32);
});
