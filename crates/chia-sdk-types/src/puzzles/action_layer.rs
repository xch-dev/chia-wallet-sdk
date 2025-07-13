mod action_layer_puzzle;
mod actions;
mod any_metadata_updater;
mod cat_makers;
mod cat_nft_metadata;
mod default_finalizer;
mod name_nft_metadata;
mod p2_delegated_by_singleton;
mod p2_m_of_n_delegate_direct;
mod precommit_layer;
mod reserve_finalizer;
mod slot;
mod slot_values;
mod state_scheduler;
mod uniqueness_prelauncher;
mod verification_asserter;
mod verification_layer;

pub use action_layer_puzzle::*;
pub use actions::*;
pub use any_metadata_updater::*;
pub use cat_makers::*;
pub use cat_nft_metadata::*;
pub use default_finalizer::*;
pub use name_nft_metadata::*;
pub use p2_delegated_by_singleton::*;
pub use p2_m_of_n_delegate_direct::*;
pub use precommit_layer::*;
pub use reserve_finalizer::*;
pub use slot::*;
pub use slot_values::*;
pub use state_scheduler::*;
pub use uniqueness_prelauncher::*;
pub use verification_asserter::*;
pub use verification_layer::*;

#[cfg(test)]
mod tests {
    use crate::assert_puzzle_hash;

    use super::*;

    #[test]
    fn test_puzzle_hashes() -> anyhow::Result<()> {
        assert_puzzle_hash!(DEFAULT_FINALIZER_PUZZLE => DEFAULT_FINALIZER_PUZZLE_HASH);
        assert_puzzle_hash!(ACTION_LAYER_PUZZLE => ACTION_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(DELEGATED_STATE_ACTION_PUZZLE => DELEGATED_STATE_ACTION_PUZZLE_HASH);
        assert_puzzle_hash!(CATALOG_REGISTER_PUZZLE => CATALOG_REGISTER_PUZZLE_HASH);
        assert_puzzle_hash!(CATALOG_REFUND_PUZZLE => CATALOG_REFUND_PUZZLE_HASH);
        assert_puzzle_hash!(UNIQUENESS_PRELAUNCHER_PUZZLE => UNIQUENESS_PRELAUNCHER_PUZZLE_HASH);
        assert_puzzle_hash!(PRECOMMIT_LAYER_PUZZLE => PRECOMMIT_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(SLOT_PUZZLE => SLOT_PUZZLE_HASH);
        assert_puzzle_hash!(ANY_METADATA_UPDATER => ANY_METADATA_UPDATER_HASH);
        assert_puzzle_hash!(VERIFICATION_LAYER_PUZZLE => VERIFICATION_LAYER_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_REGISTER_PUZZLE => XCHANDLES_REGISTER_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_UPDATE_PUZZLE => XCHANDLES_UPDATE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXTEND_PUZZLE => XCHANDLES_EXTEND_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXPIRE_PUZZLE => XCHANDLES_EXPIRE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_ORACLE_PUZZLE => XCHANDLES_ORACLE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_REFUND_PUZZLE => XCHANDLES_REFUND_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_FACTOR_PRICING_PUZZLE => XCHANDLES_FACTOR_PRICING_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE => XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH);
        assert_puzzle_hash!(DEFAULT_CAT_MAKER_PUZZLE => DEFAULT_CAT_MAKER_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE => REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE => REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE => REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE => REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE => REWARD_DISTRIBUTOR_NEW_EPOCH_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE => REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_SYNC_PUZZLE => REWARD_DISTRIBUTOR_SYNC_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE => REWARD_DISTRIBUTOR_WITHDRAW_INCENTIVES_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_STAKE_PUZZLE => REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE => REWARD_DISTRIBUTOR_UNSTAKE_PUZZLE_HASH);
        assert_puzzle_hash!(NONCE_WRAPPER_PUZZLE => NONCE_WRAPPER_PUZZLE_HASH);
        assert_puzzle_hash!(RESERVE_FINALIZER_PUZZLE => RESERVE_FINALIZER_PUZZLE_HASH);
        assert_puzzle_hash!(P2_DELEGATED_BY_SINGLETON_PUZZLE => P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH);
        assert_puzzle_hash!(STATE_SCHEDULER_PUZZLE => STATE_SCHEDULER_PUZZLE_HASH);
        assert_puzzle_hash!(P2_M_OF_N_DELEGATE_DIRECT_PUZZLE => P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH);
        assert_puzzle_hash!(
            RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM =>
                RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH
        );
        assert_puzzle_hash!(VERIFICATION_ASSERTER_PUZZLE => VERIFICATION_ASSERTER_PUZZLE_HASH);
        assert_puzzle_hash!(CATALOG_VERIFICATION_MAKER_PUZZLE => CATALOG_VERIFICATION_MAKER_PUZZLE_HASH);
        assert_puzzle_hash!(REVOCABLE_CAT_MAKER_PUZZLE => REVOCABLE_CAT_MAKER_PUZZLE_HASH);
        assert_puzzle_hash!(XCH_CAT_MAKER_PUZZLE => XCH_CAT_MAKER_PUZZLE_HASH);
        Ok(())
    }
}
