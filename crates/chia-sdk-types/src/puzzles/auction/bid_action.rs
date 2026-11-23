use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{AuctionBid, AuctionTimings},
};

/// The serialized bytecode of the auction's bid action, compiled from Rue. See
/// [`AuctionBidActionArgs`].
pub const AUCTION_BID_ACTION_PUZZLE: [u8; 449] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff03ffff22ffff20ff820b7f80ffff09ffff0d
    ff0680ffff012080ffff15ff04ff82177f80ffff21ffff20ff81bf80ffff20ff
    ff15ff04ff81bf808080ffff21ffff09ff04ff81bf80ffff02ff0bffff04ffff
    03ff82177fff820f7fff8080ffff04ff02ff808080808080ffff01ff04ffff04
    ff82027fffff04ffff10ff04ffff13ffff12ff04ff2f80ffff0182271080ffff
    13ffff12ff04ff5f80ffff018227108080ffff04ff80ff02808080ffff04ffff
    03ffff22ff8205ffffff15ff37ff808080ffff04ffff0154ffff04ff37ff8080
    80ffff04ffff0155ffff04ff27ff80808080ffff04ffff04ffff013cffff04ff
    ff02ff05ffff04ff05ff028080ff808080ffff03ff82177fffff04ffff04ffff
    0181d6ffff04ffff0133ffff04ff821f7fffff04ff82057fffff04ffff04ff82
    1f7fff8080ff808080808080ff8080ff8080808080ffff01ff088080ff0180ff
    ff04ffff04ffff10ff82027f80ff82037f80ff018080ffff04ffff01ff02ffff
    03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff
    02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ff0180
    80
    "
);

/// The tree hash of [`AUCTION_BID_ACTION_PUZZLE`].
pub const AUCTION_BID_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "b49951ee05c2dbaaff1e61d522575fe1384c1bb2b365149a081eb693f4b471e1"
));

/// Places a bid, which must be strictly higher than the current winning bid and accepted by the bid
/// verifier. The previous winning bidder is refunded in full by the reserve.
///
/// If there's a buy now price, no bid can exceed it, and a bid at exactly that price skips the bid
/// verifier, so that the increment can't make it unreachable.
///
/// The new reserve amount is the bid plus the buyer's premium and the royalty, which are each
/// rounded down separately so that the end action pays out exactly the reserve. The action creates
/// a coin announcement of the tree hash of the bid, which the bidder should assert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionBidActionArgs<V> {
    /// The bid verifier puzzle, such as [`AuctionFlatBidVerifierArgs`] or
    /// [`AuctionPercentBidVerifierArgs`] curried into its mod. It's run with the previous winning
    /// bid (or nil if there isn't one) and the new bid, and must return a truthy value.
    ///
    /// [`AuctionFlatBidVerifierArgs`]: crate::puzzles::AuctionFlatBidVerifierArgs
    /// [`AuctionPercentBidVerifierArgs`]: crate::puzzles::AuctionPercentBidVerifierArgs
    pub bid_verifier: V,
    /// When bids can be placed.
    pub timings: AuctionTimings,
    /// The buyer's premium in basis points, which the bidder locks up on top of the bid.
    pub buyers_premium_bps: u64,
    /// The NFT's royalty in basis points, which the bidder locks up on top of the bid.
    pub royalty_bps: u64,
    /// The price at which a bid wins the auction outright, or 0 if there's no buy now price. No bid
    /// can exceed it, and a bid at exactly this price skips the bid verifier.
    pub buy_now_price: u64,
}

impl<V> AuctionBidActionArgs<V> {
    /// Creates new [`AuctionBidActionArgs`] from the bid verifier, timings, fees, and buy now price.
    pub fn new(
        bid_verifier: V,
        timings: AuctionTimings,
        buyers_premium_bps: u64,
        royalty_bps: u64,
        buy_now_price: u64,
    ) -> Self {
        Self {
            bid_verifier,
            timings,
            buyers_premium_bps,
            royalty_bps,
            buy_now_price,
        }
    }
}

impl<V> Mod for AuctionBidActionArgs<V> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUCTION_BID_ACTION_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        AUCTION_BID_ACTION_PUZZLE_HASH
    }
}

/// The solution of the [`AuctionBidActionArgs`] action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionBidActionSolution {
    /// The new bid, which becomes the winning bid.
    pub bid: AuctionBid,
    /// Whether to place the bid within the grace period of the previous spend, rather than before
    /// the end time.
    pub grace: bool,
}

impl AuctionBidActionSolution {
    /// Creates a new [`AuctionBidActionSolution`] that places `bid`, within the grace period if
    /// `grace` is `true`.
    pub fn new(bid: AuctionBid, grace: bool) -> Self {
        Self { bid, grace }
    }
}
