use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::AuctionBid};

pub const AUCTION_NFT_UNLOCKER_PUZZLE: [u8; 132] = hex!(
    // Rue
    "
    ff04ffff04ffff0149ffff04ff17ff808080ffff04ffff04ffff0133ffff04ff
    1bffff04ff17ffff04ffff04ff1bff8080ff8080808080ffff04ffff04ffff01
    81f6ffff04ff80ffff04ffff03ffff13ffff12ff13ff0580ffff0182271080ff
    ff04ffff04ff13ffff04ff02ff808080ff8080ff8080ffff01ff8080808080ff
    80808080
    "
);

pub const AUCTION_NFT_UNLOCKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "bd7340be78fe28c227bf948fd8e8d322f798ecd0b3c43fa150582ee4b119a830"
));

/// Outputs the conditions that the locked NFT must output when the auction ends, which send it to
/// the winning bidder (or back to the payout puzzle hash if there are no bids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionNftUnlockerArgs {
    /// The full puzzle hash of the settlement payments puzzle for the reserve's asset, which is used
    /// as the trade price puzzle hash of the NFT transfer.
    pub settlement_puzzle_hash: Bytes32,
    pub royalty_bps: u64,
}

impl AuctionNftUnlockerArgs {
    pub fn new(settlement_puzzle_hash: Bytes32, royalty_bps: u64) -> Self {
        Self {
            settlement_puzzle_hash,
            royalty_bps,
        }
    }
}

impl Mod for AuctionNftUnlockerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUCTION_NFT_UNLOCKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        AUCTION_NFT_UNLOCKER_PUZZLE_HASH
    }
}

/// The end and cancel actions pass the winning bid to the unlocker, followed by the rest of their
/// own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionNftUnlockerSolution {
    pub winning_bid: AuctionBid,
    pub nft_amount: u64,
}

impl AuctionNftUnlockerSolution {
    pub fn new(winning_bid: AuctionBid, nft_amount: u64) -> Self {
        Self {
            winning_bid,
            nft_amount,
        }
    }
}
