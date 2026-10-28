use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{AuctionBid, AuctionTimings},
};

pub const AUCTION_BID_ACTION_PUZZLE: [u8; 420] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff03ffff22ffff22ffff22ffff20ff8205bf80
    ffff09ffff0dff0680ffff01208080ffff15ff04ff820bbf8080ffff02ff0bff
    ff04ffff03ff820bbfff8207bfff8080ffff04ff02ff8080808080ffff01ff04
    ffff04ff82013fffff04ffff10ff04ffff13ffff12ff04ff2f80ffff01822710
    80ffff13ffff12ff04ff5f80ffff018227108080ffff04ff80ff02808080ffff
    04ffff03ffff22ff8202ffffff15ff37ff808080ffff04ffff0154ffff04ff37
    ff808080ffff04ffff0155ffff04ff27ff80808080ffff04ffff04ffff013cff
    ff04ffff02ff05ffff04ff05ff028080ff808080ffff03ff820bbfffff04ffff
    04ffff0181d6ffff04ffff0133ffff04ff820fbfffff04ff8202bfffff04ffff
    04ff820fbfff8080ff808080808080ff8080ff8080808080ffff01ff088080ff
    0180ffff04ffff04ffff10ff82013f80ff8201bf80ff018080ffff04ffff01ff
    02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff0580
    80ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180
    ff018080
    "
);

pub const AUCTION_BID_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "3357138553b6efb673f36202c78967ade33bbb80cc8e3683f9fe0de97e36a357"
));

/// Places a bid, which must be strictly higher than the current winning bid and accepted by the bid
/// verifier. The previous winning bidder is refunded in full by the reserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionBidActionArgs<V> {
    pub bid_verifier: V,
    pub timings: AuctionTimings,
    pub buyers_premium_bps: u64,
    pub royalty_bps: u64,
}

impl<V> AuctionBidActionArgs<V> {
    pub fn new(
        bid_verifier: V,
        timings: AuctionTimings,
        buyers_premium_bps: u64,
        royalty_bps: u64,
    ) -> Self {
        Self {
            bid_verifier,
            timings,
            buyers_premium_bps,
            royalty_bps,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionBidActionSolution {
    pub bid: AuctionBid,
    /// Whether to place the bid within the grace period of the previous spend, rather than before
    /// the end time.
    pub grace: bool,
}

impl AuctionBidActionSolution {
    pub fn new(bid: AuctionBid, grace: bool) -> Self {
        Self { bid, grace }
    }
}
