use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{AuctionPayments, AuctionTimings},
};

/// The serialized bytecode of the auction's end action, compiled from Rue. See
/// [`AuctionEndActionArgs`].
pub const AUCTION_END_ACTION_PUZZLE: [u8; 650] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ff8216ffffff01ff0880ffff01ff02ffff01ff02ffff
    01ff04ffff04ff8213ffffff04ff80ffff04ffff0101ff827bff808080ffff04
    ffff03ff15ffff04ffff0181d6ffff04ffff0133ffff04ff82019fffff04ff15
    ffff04ffff04ff82019fff8080ff808080808080ffff01ff018080ffff04ffff
    03ff09ffff04ffff0181d6ffff04ffff0133ffff04ff82035fffff04ff09ffff
    04ffff04ff82035fff8080ff808080808080ffff01ff018080ffff04ffff03ff
    ff15ff02ff8080ffff04ffff0181d6ffff04ffff0133ffff04ff8201dfffff04
    ff02ffff04ffff04ff8201dfff8080ff808080808080ffff01ff018080ffff04
    ffff03ff1dffff04ffff0181d6ffff04ffff0133ffff04ff8202ffffff04ff1d
    ff8080808080ffff01ff018080ffff04ffff04ffff0142ffff04ffff0117ffff
    04ffff02ff0bffff04ff0bffff04ffff0101ffff02ff17ffff04ff827bffff82
    0fff8080808080ffff04ff8205ffff8080808080ffff04ffff01ff3cff2480ff
    ff03ffff22ffff15ff82017fff8080ffff09ff82bbffff82017f8080ff80ffff
    04ffff04ffff0151ffff04ff4fff808080ffff03ffff15ff6fff8080ffff04ff
    ff04ffff0150ffff04ff6fff808080ff8080ff8080808080808080808080ffff
    04ffff11ff825dffff0480ff018080ffff04ffff04ffff13ffff12ff822effff
    819780ffff0182271080ffff04ffff13ffff12ff822effff4780ffff01822710
    80ffff13ffff12ff822effff2f80ffff01822710808080ff01808080ff0180ff
    ff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff
    04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff
    038080ff0180ff018080
    "
);

/// The tree hash of [`AUCTION_END_ACTION_PUZZLE`].
pub const AUCTION_END_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "6f01d065d212840a9a92b05b24c575a8a46f52dd40bd114402f94127722263d5"
));

/// Ends the auction once its timings allow it, or right away if the winning bid is at the buy now
/// price, paying out the winning bid from the reserve and authorizing the NFT to be unlocked.
///
/// The reserve pays the buyer's premium and the commission, the winning bid minus the commission
/// to the payout puzzle hash, and the royalty to an ephemeral settlement coin, which the NFT's
/// transfer asserts. Payments that round to 0 are skipped. The action's solution is passed to the
/// unlocker after the winning bid, and it creates a coin announcement of `$`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct AuctionEndActionArgs<U> {
    /// The unlocker puzzle, such as [`AuctionNftUnlockerArgs`] curried into its mod, which outputs
    /// the conditions that the NFT is authorized to output.
    ///
    /// [`AuctionNftUnlockerArgs`]: crate::puzzles::AuctionNftUnlockerArgs
    pub unlocker: U,
    /// When the auction can be ended.
    pub timings: AuctionTimings,
    /// Where the winning bid is paid out.
    pub payments: AuctionPayments,
    /// The NFT's royalty in basis points of the winning bid.
    pub royalty_bps: u64,
    /// The price at which a bid wins the auction outright, or 0 if there's no buy now price. If the
    /// winning bid is at this price, the auction can be ended without waiting for the end time or
    /// grace period.
    pub buy_now_price: u64,
    /// The inner puzzle hash of the settlement coin created by the reserve, which is wrapped by the
    /// CAT layer for CAT reserves.
    pub settlement_payment_hash: Bytes32,
    /// The coin id of the locked NFT, which the unlock message is sent to.
    pub nft_coin_id: Bytes32,
}

impl<U> AuctionEndActionArgs<U> {
    /// Creates new [`AuctionEndActionArgs`] from the unlocker, timings, payments, royalty, buy now
    /// price, settlement payment hash, and NFT coin id.
    pub fn new(
        unlocker: U,
        timings: AuctionTimings,
        payments: AuctionPayments,
        royalty_bps: u64,
        buy_now_price: u64,
        settlement_payment_hash: Bytes32,
        nft_coin_id: Bytes32,
    ) -> Self {
        Self {
            unlocker,
            timings,
            payments,
            royalty_bps,
            buy_now_price,
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
