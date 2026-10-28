use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const AUCTION_FLAT_BID_VERIFIER_PUZZLE: [u8; 53] = hex!(
    // Rue
    "
    ff02ffff03ff0bffff01ff15ff27ffff11ffff10ff13ff0580ffff01018080ff
    ff01ff15ff27ffff11ff02ffff0101808080ff0180
    "
);

pub const AUCTION_FLAT_BID_VERIFIER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "6f89d267366539c51f4d06455de4f2a461637a3d000483d78c8b317e6cc2078e"
));

pub const AUCTION_PERCENT_BID_VERIFIER_PUZZLE: [u8; 69] = hex!(
    // Rue
    "
    ff02ffff03ff0bffff01ff15ff27ffff11ffff10ff13ffff13ffff12ff13ff05
    80ffff018227108080ffff01018080ffff01ff15ff27ffff11ff02ffff010180
    8080ff0180
    "
);

pub const AUCTION_PERCENT_BID_VERIFIER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "3dd15361ad8955a37f758ec39a8fb081f1025eda62c06996f4bc7a2a5f07ddc0"
));

/// Requires the first bid to be at least `minimum`, and each bid after it to be at least
/// `increment` more than the previous one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionFlatBidVerifierArgs {
    pub minimum: u64,
    pub increment: u64,
}

impl AuctionFlatBidVerifierArgs {
    pub fn new(minimum: u64, increment: u64) -> Self {
        Self { minimum, increment }
    }
}

impl Mod for AuctionFlatBidVerifierArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUCTION_FLAT_BID_VERIFIER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        AUCTION_FLAT_BID_VERIFIER_PUZZLE_HASH
    }
}

/// Requires the first bid to be at least `minimum`, and each bid after it to be at least
/// `increment_bps` basis points (rounded down) more than the previous one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionPercentBidVerifierArgs {
    pub minimum: u64,
    pub increment_bps: u64,
}

impl AuctionPercentBidVerifierArgs {
    pub fn new(minimum: u64, increment_bps: u64) -> Self {
        Self {
            minimum,
            increment_bps,
        }
    }
}

impl Mod for AuctionPercentBidVerifierArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUCTION_PERCENT_BID_VERIFIER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        AUCTION_PERCENT_BID_VERIFIER_PUZZLE_HASH
    }
}
