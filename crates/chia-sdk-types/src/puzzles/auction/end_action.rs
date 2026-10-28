use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{AuctionPayments, AuctionTimings},
};

pub const AUCTION_END_ACTION_PUZZLE: [u8; 631] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ff820b7fffff01ff0880ffff01ff02ffff01ff02ffff
    01ff02ffff01ff04ffff04ff8213ffffff04ff80ffff04ffff0101ff827bff80
    8080ffff04ffff03ff15ffff04ffff0181d6ffff04ffff0133ffff04ff82033f
    ffff04ff15ffff04ffff04ff82033fff8080ff808080808080ffff01ff018080
    ffff04ffff03ff09ffff04ffff0181d6ffff04ffff0133ffff04ff8206bfffff
    04ff09ffff04ffff04ff8206bfff8080ff808080808080ffff01ff018080ffff
    04ffff03ffff15ff02ff8080ffff04ffff0181d6ffff04ffff0133ffff04ff82
    03bfffff04ff02ffff04ffff04ff8203bfff8080ff808080808080ffff01ff01
    8080ffff04ffff03ff1dffff04ffff0181d6ffff04ffff0133ffff04ff8202ff
    ffff04ff1dff8080808080ffff01ff018080ffff04ffff04ffff0142ffff04ff
    ff0117ffff04ffff02ff17ffff04ff17ffff04ffff0101ffff02ff2fffff04ff
    827bffff820fff8080808080ffff04ff8205ffff8080808080ffff04ffff04ff
    ff0151ffff04ff819fff808080ffff04ffff01ff3cff2480ffff03ffff15ff81
    dfff8080ffff04ffff04ffff0150ffff04ff81dfff808080ff8080ff80808080
    808080808080ffff04ffff11ff05ff0480ff018080ffff04ffff04ffff13ffff
    12ff02ff82012f80ffff0182271080ffff04ffff13ffff12ff02ff818f80ffff
    0182271080ffff13ffff12ff02ff5f80ffff01822710808080ff018080ffff04
    ff82177fff01808080ff0180ffff04ffff01ff02ffff03ffff07ff0380ffff01
    ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07
    808080ffff01ff0bffff0101ff038080ff0180ff018080
    "
);

pub const AUCTION_END_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "4cdb31b702e6f9e363035aab9e431352a901e6e2c31a76b0552d6434a6741344"
));

/// Ends the auction once its timings allow it, paying out the winning bid from the reserve and
/// authorizing the NFT to be unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionEndActionArgs<U> {
    pub unlocker: U,
    pub timings: AuctionTimings,
    pub payments: AuctionPayments,
    pub royalty_bps: u64,
    /// The inner puzzle hash of the settlement coin created by the reserve, which is wrapped by the
    /// CAT layer for CAT reserves.
    pub settlement_payment_hash: Bytes32,
    pub nft_coin_id: Bytes32,
}

impl<U> AuctionEndActionArgs<U> {
    pub fn new(
        unlocker: U,
        timings: AuctionTimings,
        payments: AuctionPayments,
        royalty_bps: u64,
        settlement_payment_hash: Bytes32,
        nft_coin_id: Bytes32,
    ) -> Self {
        Self {
            unlocker,
            timings,
            payments,
            royalty_bps,
            settlement_payment_hash,
            nft_coin_id,
        }
    }
}

impl<U> Mod for AuctionEndActionArgs<U> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&AUCTION_END_ACTION_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        AUCTION_END_ACTION_PUZZLE_HASH
    }
}
