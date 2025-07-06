use chia::{
    clvm_utils::{ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::{cat::CatArgs, singleton::SingletonArgs},
};
use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, SingletonLayer, SpendContext},
    types::MerkleTree,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{
    Action, ActionLayer, ActionLayerArgs, Finalizer, P2DelegatedBySingletonLayerArgs,
    ReserveFinalizer2ndCurryArgs, RewardDistributorAddEntryAction,
    RewardDistributorAddIncentivesAction, RewardDistributorCommitIncentivesAction,
    RewardDistributorInitiatePayoutAction, RewardDistributorNewEpochAction,
    RewardDistributorRemoveEntryAction, RewardDistributorStakeAction, RewardDistributorSyncAction,
    RewardDistributorUnstakeAction, RewardDistributorWithdrawIncentivesAction, SpendContextExt,
    RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
};

use super::Reserveful;

pub type RewardDistributorLayers = SingletonLayer<ActionLayer<RewardDistributorState, NodePtr>>;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct RoundRewardInfo {
    pub cumulative_payout: u64,
    #[clvm(rest)]
    pub remaining_rewards: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct RoundTimeInfo {
    pub last_update: u64,
    #[clvm(rest)]
    pub epoch_end: u64,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct RewardDistributorState {
    pub total_reserves: u64,
    pub active_shares: u64,
    pub round_reward_info: RoundRewardInfo,
    pub round_time_info: RoundTimeInfo,
}

impl RewardDistributorState {
    pub fn initial(first_epoch_start: u64) -> Self {
        Self {
            total_reserves: 0,
            active_shares: 0,
            round_reward_info: RoundRewardInfo {
                cumulative_payout: 0,
                remaining_rewards: 0,
            },
            round_time_info: RoundTimeInfo {
                last_update: first_epoch_start,
                epoch_end: first_epoch_start,
            },
        }
    }
}

impl Reserveful for RewardDistributorState {
    fn reserve_amount(&self, index: u64) -> u64 {
        if index == 0 {
            self.total_reserves
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[repr(u8)]
#[clvm(atom)]
pub enum RewardDistributorType {
    Manager = 1,
    Nft = 2,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RewardDistributorConstants {
    pub launcher_id: Bytes32,
    pub reward_distributor_type: RewardDistributorType,
    pub manager_or_collection_did_launcher_id: Bytes32,
    pub fee_payout_puzzle_hash: Bytes32,
    pub epoch_seconds: u64,
    pub max_seconds_offset: u64,
    pub payout_threshold: u64,
    pub fee_bps: u64,
    pub withdrawal_share_bps: u64,
    pub reserve_asset_id: Bytes32,
    pub reserve_inner_puzzle_hash: Bytes32,
    pub reserve_full_puzzle_hash: Bytes32,
}

impl RewardDistributorConstants {
    #[allow(clippy::too_many_arguments)]
    pub fn without_launcher_id(
        reward_distributor_type: RewardDistributorType,
        manager_or_collection_did_launcher_id: Bytes32,
        fee_payout_puzzle_hash: Bytes32,
        epoch_seconds: u64,
        max_seconds_offset: u64,
        payout_threshold: u64,
        fee_bps: u64,
        withdrawal_share_bps: u64,
        reserve_asset_id: Bytes32,
    ) -> Self {
        Self {
            launcher_id: Bytes32::default(),
            reward_distributor_type,
            manager_or_collection_did_launcher_id,
            fee_payout_puzzle_hash,
            epoch_seconds,
            max_seconds_offset,
            payout_threshold,
            fee_bps,
            withdrawal_share_bps,
            reserve_asset_id,
            reserve_inner_puzzle_hash: Bytes32::default(),
            reserve_full_puzzle_hash: Bytes32::default(),
        }
    }

    pub fn with_launcher_id(mut self, launcher_id: Bytes32) -> Self {
        self.launcher_id = launcher_id;
        self.reserve_inner_puzzle_hash =
            P2DelegatedBySingletonLayerArgs::curry_tree_hash_with_launcher_id(launcher_id, 0)
                .into();
        self.reserve_full_puzzle_hash =
            CatArgs::curry_tree_hash(self.reserve_asset_id, self.reserve_inner_puzzle_hash.into())
                .into();
        self
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct RewardDistributorInfo {
    pub state: RewardDistributorState,

    pub constants: RewardDistributorConstants,
}

impl RewardDistributorInfo {
    pub fn new(state: RewardDistributorState, constants: RewardDistributorConstants) -> Self {
        Self { state, constants }
    }

    pub fn with_state(mut self, state: RewardDistributorState) -> Self {
        self.state = state;
        self
    }

    pub fn action_puzzle_hashes(constants: &RewardDistributorConstants) -> [Bytes32; 8] {
        [
            RewardDistributorAddIncentivesAction::from_constants(constants)
                .tree_hash()
                .into(),
            RewardDistributorCommitIncentivesAction::from_constants(constants)
                .tree_hash()
                .into(),
            RewardDistributorInitiatePayoutAction::from_constants(constants)
                .tree_hash()
                .into(),
            RewardDistributorNewEpochAction::from_constants(constants)
                .tree_hash()
                .into(),
            RewardDistributorSyncAction::from_constants(constants)
                .tree_hash()
                .into(),
            RewardDistributorWithdrawIncentivesAction::from_constants(constants)
                .tree_hash()
                .into(),
            match constants.reward_distributor_type {
                RewardDistributorType::Manager => {
                    RewardDistributorAddEntryAction::from_constants(constants)
                        .tree_hash()
                        .into()
                }
                RewardDistributorType::Nft => {
                    RewardDistributorStakeAction::from_constants(constants)
                        .tree_hash()
                        .into()
                }
            },
            match constants.reward_distributor_type {
                RewardDistributorType::Manager => {
                    RewardDistributorRemoveEntryAction::from_constants(constants)
                        .tree_hash()
                        .into()
                }
                RewardDistributorType::Nft => {
                    RewardDistributorUnstakeAction::from_constants(constants)
                        .tree_hash()
                        .into()
                }
            },
        ]
    }

    pub fn into_layers(
        self,
        ctx: &mut SpendContext,
    ) -> Result<RewardDistributorLayers, DriverError> {
        Ok(SingletonLayer::new(
            self.constants.launcher_id,
            ActionLayer::<RewardDistributorState, NodePtr>::from_action_puzzle_hashes(
                &Self::action_puzzle_hashes(&self.constants),
                self.state,
                Finalizer::Reserve {
                    reserve_full_puzzle_hash: self.constants.reserve_full_puzzle_hash,
                    reserve_inner_puzzle_hash: self.constants.reserve_inner_puzzle_hash,
                    reserve_amount_from_state_program: ctx
                        .default_reserve_amount_from_state_program()?,
                    hint: self.constants.launcher_id,
                },
            ),
        ))
    }

    pub fn parse(
        allocator: &mut Allocator,
        puzzle: Puzzle,
        constants: RewardDistributorConstants,
    ) -> Result<Option<Self>, DriverError> {
        let Some(layers) = RewardDistributorLayers::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let action_puzzle_hashes = Self::action_puzzle_hashes(&constants);
        let merkle_root = MerkleTree::new(&action_puzzle_hashes).root();
        if layers.inner_puzzle.merkle_root != merkle_root {
            return Ok(None);
        }

        Ok(Some(Self::from_layers(layers, constants)))
    }

    pub fn from_layers(
        layers: RewardDistributorLayers,
        constants: RewardDistributorConstants,
    ) -> Self {
        Self {
            state: layers.inner_puzzle.state,
            constants,
        }
    }

    pub fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::curry_tree_hash(self.constants.launcher_id, self.inner_puzzle_hash())
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            ReserveFinalizer2ndCurryArgs::curry_tree_hash(
                self.constants.reserve_full_puzzle_hash,
                self.constants.reserve_inner_puzzle_hash,
                RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
                self.constants.launcher_id,
            ),
            MerkleTree::new(&Self::action_puzzle_hashes(&self.constants)).root(),
            self.state.tree_hash(),
        )
    }
}
