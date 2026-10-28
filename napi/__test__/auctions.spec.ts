import test from "ava";

import {
  Auction,
  AuctionBid,
  AuctionBidVerifier,
  AuctionBpsPayment,
  AuctionLaunch,
  AuctionPayments,
  AuctionReserve,
  AuctionSettings,
  AuctionTimings,
  auctionLockP2PuzzleHash,
  BlsPairWithCoin,
  calculateAuctionPayment,
  Clvm,
  Coin,
  Constants,
  Nft,
  NftMint,
  RoyaltyInfo,
  Simulator,
} from "../index.js";

const END_TIME = 1000n;
const PREMIUM_PUZZLE_HASH = new Uint8Array(32).fill(1);
const COMMISSION_PUZZLE_HASH = new Uint8Array(32).fill(2);

interface LaunchedAuction {
  sim: Simulator;
  clvm: Clvm;
  seller: BlsPairWithCoin;
  auction: Auction;
  nft: Nft;
}

// Mints an NFT, locks it up, and launches an auction for it in a single spend bundle
function launchAuction(): LaunchedAuction {
  const sim = new Simulator();
  const clvm = new Clvm();

  const seller = sim.bls(2n);

  const {
    nfts: [nft],
    parentConditions: mintParentConditions,
  } = clvm.mintNfts(seller.coin.coinId(), [
    new NftMint(
      clvm.nil(),
      Constants.nftMetadataUpdaterDefaultHash(),
      seller.puzzleHash,
      seller.puzzleHash,
      500,
      null,
    ),
  ]);

  // The NFT must be locked by the auction before it's launched
  const launcherId = new Coin(
    seller.coin.coinId(),
    Constants.singletonLauncherHash(),
    1n,
  ).coinId();
  const lockPuzzleHash = auctionLockP2PuzzleHash(launcherId);

  const lockedNft = clvm.spendNft(
    nft,
    clvm.standardSpend(
      seller.pk,
      clvm.delegatedSpend([clvm.createCoin(lockPuzzleHash, 1n)]),
    ),
  );

  const settings = new AuctionSettings(
    AuctionBidVerifier.flat(100n, 100n),
    new AuctionTimings(END_TIME, 0n),
    new AuctionPayments(
      new AuctionBpsPayment(300n, PREMIUM_PUZZLE_HASH),
      new AuctionBpsPayment(100n, COMMISSION_PUZZLE_HASH),
      seller.puzzleHash,
    ),
  );

  // The reserve is an empty coin locked by the auction
  const reserve = AuctionReserve.xch(
    new Coin(seller.coin.coinId(), lockPuzzleHash, 0n),
  );

  const { auction, parentConditions: launchParentConditions } =
    clvm.launchAuction(seller.coin.coinId(), settings, reserve, lockedNft);

  clvm.spendStandardCoin(
    seller.coin,
    seller.pk,
    clvm.delegatedSpend([
      ...mintParentConditions,
      ...launchParentConditions,
      clvm.createCoin(lockPuzzleHash, 0n),
    ]),
  );

  sim.spendCoins(clvm.coinSpends(), [seller.sk]);

  return { sim, clvm, seller, auction, nft: lockedNft };
}

// Parses the child of an auction from the blockchain, like a wallet syncing it would
function parseChild(sim: Simulator, clvm: Clvm, auction: Auction): Auction {
  const coinSpend = sim.coinSpend(auction.coin.coinId());
  if (!coinSpend) throw new Error("Auction spend not found");
  return auction.parseChild(clvm, clvm.deserialize(coinSpend.solution));
}

test("auction can be parsed from its launcher", (t) => {
  const { sim, clvm, auction, nft } = launchAuction();

  const launcherSpend = sim.coinSpend(auction.info.launcherId);
  if (!launcherSpend) throw new Error("Launcher spend not found");

  const launch = AuctionLaunch.parse(
    launcherSpend.coin,
    clvm.deserialize(launcherSpend.solution),
  );
  if (!launch) throw new Error("Launcher isn't for an auction");

  t.deepEqual(launch.nftCoinId(), nft.coin.coinId());
  t.is(launch.settings().bidVerifier.minimumBid(), 100n);
  t.is(launch.settings().bidVerifier.bidIncrement(), 100n);

  // The NFT's royalty info isn't in the memo, so it's fetched from the NFT. The reserve is XCH, so
  // there's no lineage proof to fetch from its parent.
  t.is(launch.reserveAssetId(), null);
  t.deepEqual(
    launch.reserveParentCoinInfo(),
    auction.info.reserve.coin().parentCoinInfo,
  );

  const parsed = launch.intoAuction(
    new RoyaltyInfo(
      nft.info.launcherId,
      nft.info.royaltyPuzzleHash,
      nft.info.royaltyBasisPoints,
    ),
    null,
  );
  t.deepEqual(parsed.coin.coinId(), auction.coin.coinId());
  t.deepEqual(parsed.info.innerPuzzleHash(), auction.info.innerPuzzleHash());

  // Other launchers aren't auctions
  const nftLauncherSpend = sim.coinSpend(nft.info.launcherId);
  if (!nftLauncherSpend) throw new Error("NFT launcher spend not found");
  t.is(
    AuctionLaunch.parse(
      nftLauncherSpend.coin,
      clvm.deserialize(nftLauncherSpend.solution),
    ),
    null,
  );
});

test("auction pays out the winning bid", (t) => {
  const { sim, clvm, seller, auction: launched, nft } = launchAuction();
  let auction = launched;

  t.throws(() => AuctionBidVerifier.flat(0n, 100n));
  t.is(calculateAuctionPayment(1000n, 300n), 30n);

  const bid = (amount: bigint) => {
    const reserveAmount = auction.info.reserveAmountForBid(amount);
    if (reserveAmount === null) throw new Error("Reserve amount overflow");

    const bidder = sim.bls(reserveAmount);
    const bid = new AuctionBid(amount, bidder.puzzleHash);

    // The bidder funds the reserve, and asserts the bid so the funds can't be used for another
    clvm.spendStandardCoin(
      bidder.coin,
      bidder.pk,
      clvm.delegatedSpend([
        clvm.assertCoinAnnouncement(auction.bidAnnouncementId(bid)),
      ]),
    );

    const child = auction.spend(
      clvm,
      [auction.bidActionSpend(clvm, bid, false)],
      [],
    );
    sim.spendCoins(clvm.coinSpends(), [bidder.sk]);

    t.deepEqual(
      parseChild(sim, clvm, auction).coin.coinId(),
      child.coin.coinId(),
    );
    auction = child;

    return { bidder, reserveAmount };
  };

  const first = bid(1000n);
  t.is(first.reserveAmount, 1000n + 30n + 50n);

  // The first bidder is refunded in full when they're outbid
  const outbidReserve = auction.info.reserve.coin();
  const second = bid(2000n);
  t.is(auction.info.state.winningBid.amount, 2000n);
  t.is(auction.info.reserve.coin().amount, second.reserveAmount);
  t.truthy(
    sim.coinState(
      new Coin(
        outbidReserve.coinId(),
        first.bidder.puzzleHash,
        first.reserveAmount,
      ).coinId(),
    ),
  );

  // Bids must be at least the increment more than the previous one
  const lowBid = new AuctionBid(2050n, first.bidder.puzzleHash);
  t.throws(() =>
    auction.childState(clvm, [auction.bidActionSpend(clvm, lowBid, false)]),
  );

  // End the auction, which sends the NFT to the winning bidder
  sim.setNextTimestamp(END_TIME);

  const finalReserve = auction.info.reserve.coin();
  const endAction = auction.endActionSpend(clvm, nft);
  const wonNft = auction.unlockNft(clvm, nft);
  const ended = auction.spend(clvm, [endAction], []);
  sim.spendCoins(clvm.coinSpends(), []);

  t.true(ended.info.state.ended);
  t.deepEqual(wonNft.info.p2PuzzleHash, second.bidder.puzzleHash);
  t.is(sim.coinState(wonNft.coin.coinId())?.spentHeight, null);

  // The seller receives the winning bid minus the commission
  t.truthy(
    sim.coinState(
      new Coin(finalReserve.coinId(), seller.puzzleHash, 2000n - 20n).coinId(),
    ),
  );
  t.truthy(
    sim.coinState(
      new Coin(finalReserve.coinId(), PREMIUM_PUZZLE_HASH, 60n).coinId(),
    ),
  );
  t.truthy(
    sim.coinState(
      new Coin(finalReserve.coinId(), COMMISSION_PUZZLE_HASH, 20n).coinId(),
    ),
  );
});

test("auction can be cancelled before the first bid", (t) => {
  const { sim, clvm, seller, auction, nft } = launchAuction();

  // A coin with the payout puzzle hash authorizes the cancel
  const authorizer = sim.newCoin(seller.puzzleHash, 0n);
  clvm.spendStandardCoin(
    authorizer,
    seller.pk,
    clvm.delegatedSpend(auction.cancelAuthorization(clvm)),
  );

  const cancelAction = auction.cancelActionSpend(clvm, nft);
  const returnedNft = auction.unlockNft(clvm, nft);
  const cancelled = auction.spend(clvm, [cancelAction], []);
  sim.spendCoins(clvm.coinSpends(), [seller.sk]);

  t.true(cancelled.info.state.ended);
  t.deepEqual(returnedNft.info.p2PuzzleHash, seller.puzzleHash);
  t.is(sim.coinState(returnedNft.coin.coinId())?.spentHeight, null);

  const parsed = parseChild(sim, clvm, auction);
  t.true(parsed.info.state.ended);
  t.deepEqual(parsed.coin.coinId(), cancelled.coin.coinId());
});
