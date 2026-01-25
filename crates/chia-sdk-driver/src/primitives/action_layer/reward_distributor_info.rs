use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, singleton::SingletonArgs};
use chia_sdk_types::{
    puzzles::{
        ActionLayerArgs, DefaultReserveAmountFromStateProgramArgs, P2DelegatedBySingletonLayerArgs,
        ReserveFinalizer2ndCurryArgs,
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
    },
    MerkleTree,
};
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    ActionLayer, DriverError, Finalizer, Layer, Puzzle, RewardDistributorAddEntryAction,
    RewardDistributorAddIncentivesAction, RewardDistributorCommitIncentivesAction,
    RewardDistributorInitiatePayoutAction, RewardDistributorNewEpochAction,
    RewardDistributorRefreshAction, RewardDistributorRemoveEntryAction,
    RewardDistributorStakeAction, RewardDistributorSyncAction, RewardDistributorUnstakeAction,
    RewardDistributorWithdrawIncentivesAction, SingletonAction, SingletonLayer, SpendContext,
};

use super::Reserveful;

pub type RewardDistributorLayers = SingletonLayer<ActionLayer<RewardDistributorState, NodePtr>>;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy)]
#[clvm(list)]
pub struct RoundRewardInfo {
    pub cumulative_payout: u128,
    #[clvm(rest)]
    pub remaining_rewards: u128,
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
    #[clvm(rest)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardDistributorType {
    Managed {
        manager_singleton_launcher_id: Bytes32,
    },
    NftCollection {
        collection_did_launcher_id: Bytes32,
    },
    CuratedNft {
        store_launcher_id: Bytes32,
        refreshable: bool,
    },
    Cat {
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
    },
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for RewardDistributorType {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let type_pair: (u8, Raw<N>) = FromClvm::from_clvm(decoder, node)?;

        match type_pair.0 {
            1 => Ok(RewardDistributorType::Managed {
                manager_singleton_launcher_id: FromClvm::from_clvm(decoder, type_pair.1 .0)?,
            }),
            2 => Ok(RewardDistributorType::NftCollection {
                collection_did_launcher_id: FromClvm::from_clvm(decoder, type_pair.1 .0)?,
            }),
            3 => {
                let (store_launcher_id, refreshable): (Bytes32, bool) =
                    FromClvm::from_clvm(decoder, type_pair.1 .0)?;
                Ok(RewardDistributorType::CuratedNft {
                    store_launcher_id,
                    refreshable,
                })
            }
            4 => {
                let (asset_id, hidden_puzzle_hash): (Bytes32, Option<Bytes32>) =
                    FromClvm::from_clvm(decoder, type_pair.1 .0)?;
                Ok(RewardDistributorType::Cat {
                    asset_id,
                    hidden_puzzle_hash,
                })
            }
            _ => Err(FromClvmError::Custom(format!(
                "Invalid RewardDistributorType: {}",
                type_pair.0
            ))),
        }
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for RewardDistributorType {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        match self {
            RewardDistributorType::Managed {
                manager_singleton_launcher_id,
            } => (1, manager_singleton_launcher_id).to_clvm(encoder),
            RewardDistributorType::NftCollection {
                collection_did_launcher_id,
            } => (2, collection_did_launcher_id).to_clvm(encoder),
            RewardDistributorType::CuratedNft {
                store_launcher_id,
                refreshable,
            } => (3, (store_launcher_id, refreshable)).to_clvm(encoder),
            RewardDistributorType::Cat {
                asset_id,
                hidden_puzzle_hash,
            } => (4, (asset_id, hidden_puzzle_hash)).to_clvm(encoder),
        }
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RewardDistributorConstants {
    pub launcher_id: Bytes32,
    pub reward_distributor_type: RewardDistributorType,
    pub fee_payout_puzzle_hash: Bytes32,
    pub epoch_seconds: u64,
    pub precision: u64,
    pub max_seconds_offset: u64,
    pub payout_threshold: u64,
    pub require_payout_approval: bool,
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
        fee_payout_puzzle_hash: Bytes32,
        epoch_seconds: u64,
        precision: u64,
        max_seconds_offset: u64,
        payout_threshold: u64,
        require_payout_approval: bool,
        fee_bps: u64,
        withdrawal_share_bps: u64,
        reserve_asset_id: Bytes32,
    ) -> Self {
        Self {
            launcher_id: Bytes32::default(),
            reward_distributor_type,
            fee_payout_puzzle_hash,
            epoch_seconds,
            precision,
            max_seconds_offset,
            payout_threshold,
            require_payout_approval,
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

    pub fn action_puzzle_hashes(constants: &RewardDistributorConstants) -> Vec<Bytes32> {
        let mut action_puzzle_hashes = vec![
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
                RewardDistributorType::Managed {
                    manager_singleton_launcher_id: _,
                } => RewardDistributorAddEntryAction::from_constants(constants)
                    .tree_hash()
                    .into(),
                RewardDistributorType::NftCollection {
                    collection_did_launcher_id: _,
                }
                | RewardDistributorType::CuratedNft {
                    store_launcher_id: _,
                    refreshable: _,
                }
                | RewardDistributorType::Cat {
                    asset_id: _,
                    hidden_puzzle_hash: _,
                } => RewardDistributorStakeAction::from_constants(constants)
                    .tree_hash()
                    .into(),
            },
            match constants.reward_distributor_type {
                RewardDistributorType::Managed {
                    manager_singleton_launcher_id: _,
                } => RewardDistributorRemoveEntryAction::from_constants(constants)
                    .tree_hash()
                    .into(),
                RewardDistributorType::NftCollection {
                    collection_did_launcher_id: _,
                }
                | RewardDistributorType::CuratedNft {
                    store_launcher_id: _,
                    refreshable: _,
                }
                | RewardDistributorType::Cat {
                    asset_id: _,
                    hidden_puzzle_hash: _,
                } => RewardDistributorUnstakeAction::from_constants(constants)
                    .tree_hash()
                    .into(),
            },
        ];

        if let RewardDistributorType::CuratedNft { refreshable, .. } =
            constants.reward_distributor_type
        {
            if refreshable {
                action_puzzle_hashes.push(
                    RewardDistributorRefreshAction::from_constants(constants)
                        .tree_hash()
                        .into(),
                );
            }
        }

        action_puzzle_hashes
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
                        .alloc_mod::<DefaultReserveAmountFromStateProgramArgs>(
                    )?,
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

        Ok(Some(Self::from_layers(&layers, constants)))
    }

    pub fn from_layers(
        layers: &RewardDistributorLayers,
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
