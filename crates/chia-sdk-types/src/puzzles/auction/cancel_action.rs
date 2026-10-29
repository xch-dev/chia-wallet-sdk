use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const AUCTION_CANCEL_ACTION_PUZZLE: [u8; 259] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff20ff82016f80ffff20ff8202ef8080ffff
    01ff04ffff04ff4fffff04ff80ffff04ffff0101ff8201ef808080ffff04ffff
    04ffff0143ffff04ffff0117ffff04ffff0163ffff04ff0bff8080808080ffff
    04ffff04ffff0142ffff04ffff0117ffff04ffff02ff02ffff04ff02ffff04ff
    ff0101ffff02ff05ffff04ff8201efff3f8080808080ffff04ff17ff80808080
    80ffff01ffff3cff248080808080ffff01ff088080ff0180ffff04ffff01ff02
    ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080
    ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ff
    018080
    "
);

pub const AUCTION_CANCEL_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "3711cd50a0332a4b8f46bb2f1eb66d9c1d1ed658d104b94a4cf6e424906c782e"
));

/// The message that the auction's payout puzzle must send to the auction coin to authorize a
/// cancel.
pub const AUCTION_CANCEL_MESSAGE: &[u8] = b"c";

/// The mode of the cancel message, which commits to the sender's puzzle hash and the receiving
/// auction coin.
pub const AUCTION_CANCEL_MESSAGE_MODE: u8 = 0b010_111;

/// Cancels the auction before the first bid, which must be authorized by the payout puzzle. This
/// leaves the auction in the same state as ending it without any bids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionCancelActionArgs<U> {
    pub unlocker: U,
    pub payout_puzzle_hash: Bytes32,
    pub nft_coin_id: Bytes32,
}

impl<U> AuctionCancelActionArgs<U> {
    pub fn new(unlocker: U, payout_puzzle_hash: Bytes32, nft_coin_id: Bytes32) -> Self {
        Self {
            unlocker,
            payout_puzzle_hash,
            nft_coin_id,
        }
    }
}

impl<U> Mod for AuctionCancelActionArgs<U> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUCTION_CANCEL_ACTION_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        AUCTION_CANCEL_ACTION_PUZZLE_HASH
    }
}
