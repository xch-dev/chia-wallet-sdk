//! Puzzle arguments and solutions for the action layer and the apps built on it.
//!
//! # The action layer
//!
//! The action layer ([`ActionLayerArgs`]) is the inner puzzle of a singleton that holds
//! a persistent state and can run any of a fixed set of actions. The set is committed
//! to as a Merkle root of action puzzle hashes, and each spend reveals the actions it
//! uses with Merkle proofs ([`RawActionLayerSolution`]). Actions run in sequence: each
//! receives the current state and an ephemeral state (nil at the start of every spend)
//! and returns new values along with conditions. A finalizer then turns the final state
//! and conditions into the spend's output, usually re-creating the singleton:
//!
//! - [`DefaultFinalizer1stCurryArgs`] only re-creates the singleton.
//! - [`ReserveFinalizer1stCurryArgs`] also manages a reserve coin, locked with
//!   [`P2DelegatedBySingletonLayerArgs`], whose amount comes from the state. Actions
//!   prefix conditions with `-42` to have the reserve output them, for example to pay
//!   out rewards.
//!
//! # Building blocks
//!
//! - Slots ([`Slot1stCurryArgs`]) are 0-amount coins that store values for their
//!   controller singleton, which can read a value once by spending the slot. Their
//!   values are described [below](#slot-values).
//! - Precommit coins ([`PrecommitLayer1stCurryArgs`]) commit to a value some blocks
//!   before a registration, to prevent front-running.
//! - Uniqueness prelaunchers ([`UniquenessPrelauncher1stCurryArgs`]) tie a singleton
//!   launcher to a registry coin and value.
//! - CAT makers ([`DefaultCatMakerArgs`], [`RevocableCatMakerArgs`], [`XchCatMaker`])
//!   compute the puzzle hash of a payment asset from an inner puzzle hash.
//! - The delegated state action ([`DelegatedStateActionArgs`]) lets another singleton,
//!   such as a state scheduler ([`StateSchedulerLayerArgs`]), set the state.
//! - [`P2MOfNDelegateDirectArgs`] is an M-of-N multisig, used by medieval vaults.
//!
//! # Apps
//!
//! - **`CATalog`**: a registry of CATs. Whoever first reveals a CAT's TAIL through a
//!   precommit coin registers it and receives an NFT that holds its
//!   [`CatNftMetadata`]. Actions: [`CatalogRegisterActionArgs`] and
//!   [`CatalogRefundActionArgs`].
//! - **`XCHandles`**: a registry of handles that expire and can be extended, updated,
//!   and auctioned after expiry. Each handle points to an owner and a resolved
//!   singleton. Actions: [`XchandlesRegisterActionArgs`], [`XchandlesExtendActionArgs`],
//!   [`XchandlesExpireActionArgs`], [`XchandlesOracleActionArgs`],
//!   [`XchandlesInitiateUpdateActionArgs`], [`XchandlesExecuteUpdateActionArgs`], and
//!   [`XchandlesRefundActionArgs`], priced by [`XchandlesFactorPricingPuzzleArgs`] and
//!   [`XchandlesExponentialPremiumRenewPuzzleArgs`].
//! - **Reward distributor**: distributes incentives, epoch by epoch, to entries in
//!   proportion to their shares. Entries are either added by a manager singleton
//!   ([`RewardDistributorAddEntryActionArgs`]) or created by staking NFTs or CATs
//!   ([`RewardDistributorStakeActionArgs`]) with lock and unlock puzzles. Other
//!   actions add and commit incentives, sync, start new epochs, and pay out entries.
//!
//! # Slot values
//!
//! [`CatalogSlotValue`], [`XchandlesHandleSlotValue`], [`XchandlesUpdateSlotValue`],
//! and the `RewardDistributor*SlotValue` types describe what each app stores in its
//! slots. Registries keep their slots in a sorted doubly linked list bounded by
//! [`SLOT32_MIN_VALUE`] and [`SLOT32_MAX_VALUE`], so a key can be proven absent by
//! showing its two neighbors.

mod action_layer_puzzle;
mod actions;
mod any_metadata_updater;
mod cat_nft_metadata;
mod compact_representation;
mod default_finalizer;
mod handle_nft_metadata;
mod p2_delegated_by_singleton;
mod p2_m_of_n_delegate_direct;
mod p2_next_reward_distributor_epoch;
mod precommit_layer;
mod reserve_finalizer;
mod slot;
mod slot_values;
mod state_scheduler;
mod uniqueness_prelauncher;

pub use action_layer_puzzle::*;
pub use actions::*;
pub use any_metadata_updater::*;
pub use cat_nft_metadata::*;
pub use compact_representation::*;
pub use default_finalizer::*;
pub use handle_nft_metadata::*;
pub use p2_delegated_by_singleton::*;
pub use p2_m_of_n_delegate_direct::*;
pub use p2_next_reward_distributor_epoch::*;
pub use precommit_layer::*;
pub use reserve_finalizer::*;
pub use slot::*;
pub use slot_values::*;
pub use state_scheduler::*;
pub use uniqueness_prelauncher::*;

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
        assert_puzzle_hash!(XCHANDLES_REGISTER_PUZZLE => XCHANDLES_REGISTER_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_INITIATE_UPDATE_PUZZLE => XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXECUTE_UPDATE_PUZZLE => XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXTEND_PUZZLE => XCHANDLES_EXTEND_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXPIRE_PUZZLE => XCHANDLES_EXPIRE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_ORACLE_PUZZLE => XCHANDLES_ORACLE_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_REFUND_PUZZLE => XCHANDLES_REFUND_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_FACTOR_PRICING_PUZZLE => XCHANDLES_FACTOR_PRICING_PUZZLE_HASH);
        assert_puzzle_hash!(XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE => XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE => REWARD_DISTRIBUTOR_ADD_INCENTIVES_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE => REWARD_DISTRIBUTOR_ADD_ENTRY_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE => REWARD_DISTRIBUTOR_COMMIT_INCENTIVES_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE => REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITHOUT_APPROVAL_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE => REWARD_DISTRIBUTOR_INITIATE_PAYOUT_WITH_APPROVAL_PUZZLE_HASH);
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
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE => REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE => REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE => REWARD_DISTRIBUTOR_NFTS_FROM_DL_LOCKING_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE => REWARD_DISTRIBUTOR_NFTS_FROM_DID_LOCKING_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE => REWARD_DISTRIBUTOR_NFTS_UNLOCKING_PUZZLE_HASH);
        assert_puzzle_hash!(REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE => REWARD_DISTRIBUTOR_REFRESH_NFTS_FROM_DL_PUZZLE_HASH);
        assert_puzzle_hash!(P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE => P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE_HASH);
        Ok(())
    }
}
