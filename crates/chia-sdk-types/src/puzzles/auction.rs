mod auction_types;
mod bid_action;
mod bid_verifiers;
mod cancel_action;
mod end_action;
mod nft_unlocker;

pub use auction_types::*;
pub use bid_action::*;
pub use bid_verifiers::*;
pub use cancel_action::*;
pub use end_action::*;
pub use nft_unlocker::*;

#[cfg(test)]
mod tests {
    use crate::assert_puzzle_hash;

    use super::*;

    #[test]
    fn test_puzzle_hashes() -> anyhow::Result<()> {
        assert_puzzle_hash!(AUCTION_BID_ACTION_PUZZLE => AUCTION_BID_ACTION_PUZZLE_HASH);
        assert_puzzle_hash!(AUCTION_END_ACTION_PUZZLE => AUCTION_END_ACTION_PUZZLE_HASH);
        assert_puzzle_hash!(AUCTION_CANCEL_ACTION_PUZZLE => AUCTION_CANCEL_ACTION_PUZZLE_HASH);
        assert_puzzle_hash!(AUCTION_NFT_UNLOCKER_PUZZLE => AUCTION_NFT_UNLOCKER_PUZZLE_HASH);
        assert_puzzle_hash!(AUCTION_FLAT_BID_VERIFIER_PUZZLE => AUCTION_FLAT_BID_VERIFIER_PUZZLE_HASH);
        assert_puzzle_hash!(AUCTION_PERCENT_BID_VERIFIER_PUZZLE => AUCTION_PERCENT_BID_VERIFIER_PUZZLE_HASH);
        Ok(())
    }
}
