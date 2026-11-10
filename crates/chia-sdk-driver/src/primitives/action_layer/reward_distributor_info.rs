use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, singleton::SingletonArgs};
use chia_sdk_types::{
    MerkleTree,
    puzzles::{
        ActionLayerArgs, DefaultReserveAmountFromStateProgramArgs, P2DelegatedBySingletonLayerArgs,
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs,
    },
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

/// The layers of a [`RewardDistributor`](crate::RewardDistributor) puzzle: a singleton around an [`ActionLayer`].
pub type RewardDistributorLayers = SingletonLayer<ActionLayer<RewardDistributorState, NodePtr>>;

#[derive(
    Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
/// Tracks the rewards of the current epoch.
pub struct RoundRewardInfo {
    /// The total payout per share since launch, multiplied by the `precision` constant.
    ///
    /// An entry is owed `shares * (cumulative_payout - initial_cumulative_payout) / precision`.
    pub cumulative_payout: u128,
    /// The rewards of the current epoch that haven't been distributed yet.
    #[clvm(rest)]
    pub remaining_rewards: u128,
}

#[derive(
    Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
/// Tracks the timing of the current epoch.
pub struct RoundTimeInfo {
    /// The timestamp up to which rewards have been distributed.
    pub last_update: u64,
    /// The timestamp at which the current epoch ends.
    #[clvm(rest)]
    pub epoch_end: u64,
}

#[must_use]
#[derive(
    Debug, Clone, PartialEq, Eq, ToClvm, FromClvm, Copy, serde::Serialize, serde::Deserialize,
)]
#[clvm(list)]
/// The mutable state of a [`RewardDistributor`](crate::RewardDistributor), stored in its action layer.
pub struct RewardDistributorState {
    /// The amount the [`Reserve`](crate::Reserve) holds, which the finalizer enforces.
    pub total_reserves: u64,
    /// The total number of shares of all entries.
    pub active_shares: u64,
    /// The reward accounting of the current epoch.
    pub round_reward_info: RoundRewardInfo,
    /// The timing of the current epoch.
    #[clvm(rest)]
    pub round_time_info: RoundTimeInfo,
}

impl RewardDistributorState {
    /// Returns the state of a newly launched distributor whose first epoch starts at `first_epoch_start`.
    ///
    /// Both the last update and the epoch end are set to `first_epoch_start`, so the first epoch
    /// begins with a new epoch action.
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
        if index == 0 { self.total_reserves } else { 0 }
    }
}

/// Determines how entries are added to and removed from a [`RewardDistributor`](crate::RewardDistributor).
///
/// A managed distributor uses the add and remove entry actions. The other types use the stake and
/// unstake actions, which lock assets in the distributor in exchange for shares.
///
/// In CLVM, the type is encoded as `(type_id . data)`, with type ids 1 to 4 in declaration order.
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{RewardDistributorType, SpendContext};
///
/// let ctx = &mut SpendContext::new();
///
/// let distributor_type = RewardDistributorType::Cat {
///     asset_id: Bytes32::new([1; 32]),
///     hidden_puzzle_hash: None,
/// };
/// let ptr = ctx.alloc(&distributor_type)?;
/// assert_eq!(ctx.extract::<RewardDistributorType>(ptr)?, distributor_type);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardDistributorType {
    /// Entries are added and removed by a manager singleton.
    Managed {
        /// The launcher id of the manager singleton (for example, an authorizing vault).
        manager_singleton_launcher_id: Bytes32,
    },
    /// Entries are created by staking NFTs launched by a collection DID, one share per NFT.
    NftCollection {
        /// The launcher id of the DID that launched the collection's NFTs.
        collection_did_launcher_id: Bytes32,
    },
    /// Entries are created by staking NFTs listed, with their shares, in a data layer store.
    CuratedNft {
        /// The launcher id of the data layer store that lists the NFTs.
        store_launcher_id: Bytes32,
        /// Whether the refresh action can update staked NFTs' shares from the store.
        refreshable: bool,
    },
    /// Entries are created by staking CATs, one share per CAT mojo.
    Cat {
        /// The asset id of the CAT that can be staked.
        asset_id: Bytes32,
        /// The hidden puzzle hash, if the CAT is revocable.
        hidden_puzzle_hash: Option<Bytes32>,
    },
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for RewardDistributorType {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let type_pair: (u8, Raw<N>) = FromClvm::from_clvm(decoder, node)?;

        match type_pair.0 {
            1 => Ok(RewardDistributorType::Managed {
                manager_singleton_launcher_id: FromClvm::from_clvm(decoder, type_pair.1.0)?,
            }),
            2 => Ok(RewardDistributorType::NftCollection {
                collection_did_launcher_id: FromClvm::from_clvm(decoder, type_pair.1.0)?,
            }),
            3 => {
                let (store_launcher_id, refreshable): (Bytes32, bool) =
                    FromClvm::from_clvm(decoder, type_pair.1.0)?;
                Ok(RewardDistributorType::CuratedNft {
                    store_launcher_id,
                    refreshable,
                })
            }
            4 => {
                let (asset_id, hidden_puzzle_hash): (Bytes32, Option<Bytes32>) =
                    FromClvm::from_clvm(decoder, type_pair.1.0)?;
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
/// The values that are fixed for the lifetime of a [`RewardDistributor`](crate::RewardDistributor).
///
/// These are curried into the distributor's actions and stored in its launcher solution. Create
/// them with [`RewardDistributorConstants::without_launcher_id`], then fill in the launcher id with
/// [`RewardDistributorConstants::with_launcher_id`] once it is known.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{Reserve, RewardDistributorConstants, RewardDistributorType};
/// use chia_puzzle_types::singleton::SingletonStruct;
/// use clvm_utils::ToTreeHash;
///
/// let reserve_asset_id = Bytes32::new([1; 32]);
/// let launcher_id = Bytes32::new([2; 32]);
///
/// let constants = RewardDistributorConstants::without_launcher_id(
///     RewardDistributorType::Managed {
///         manager_singleton_launcher_id: Bytes32::new([3; 32]),
///     },
///     Bytes32::new([4; 32]), // fee payout puzzle hash
///     7 * 24 * 60 * 60,      // one-week epochs
///     10_000,                // precision
///     2 * 60,                // max seconds offset
///     100,                   // payout threshold
///     false,                 // require payout approval
///     700,                   // 7% fee
///     5_000,                 // 50% of withdrawn commitments
///     reserve_asset_id,
/// )
/// .with_launcher_id(launcher_id);
///
/// let controller_singleton_struct_hash = SingletonStruct::new(launcher_id).tree_hash().into();
/// assert_eq!(
///     constants.reserve_full_puzzle_hash,
///     Reserve::puzzle_hash(reserve_asset_id, controller_singleton_struct_hash, 0).into()
/// );
/// ```
pub struct RewardDistributorConstants {
    /// The launcher id of the distributor singleton.
    pub launcher_id: Bytes32,
    /// How entries are added and removed.
    pub reward_distributor_type: RewardDistributorType,
    /// Where the distributor's fees are paid.
    pub fee_payout_puzzle_hash: Bytes32,
    /// The length of an epoch, in seconds.
    pub epoch_seconds: u64,
    /// The factor that `cumulative_payout` is multiplied by to reduce rounding errors.
    pub precision: u64,
    /// The maximum number of seconds since the last update for adding an entry or staking.
    ///
    /// These actions must be confirmed before `last_update + max_seconds_offset`, which forces
    /// the distributor to be synced first.
    pub max_seconds_offset: u64,
    /// The minimum amount that the initiate payout action pays out.
    pub payout_threshold: u64,
    /// Whether the initiate payout action requires an approval message.
    pub require_payout_approval: bool,
    /// The fee, in basis points, taken from added incentives and epoch rewards.
    pub fee_bps: u64,
    /// The share of a commitment, in basis points, that is returned when it is withdrawn.
    pub withdrawal_share_bps: u64,
    /// The asset id of the reward CAT, which the [`Reserve`](crate::Reserve) holds.
    pub reserve_asset_id: Bytes32,
    /// The inner puzzle hash of the reserve. Set by [`RewardDistributorConstants::with_launcher_id`].
    pub reserve_inner_puzzle_hash: Bytes32,
    /// The full CAT puzzle hash of the reserve. Set by [`RewardDistributorConstants::with_launcher_id`].
    pub reserve_full_puzzle_hash: Bytes32,
}

impl RewardDistributorConstants {
    /// Creates the constants with a default launcher id and reserve puzzle hashes.
    ///
    /// Call [`RewardDistributorConstants::with_launcher_id`] before using them.
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

    /// Sets the launcher id and derives the reserve's inner and full puzzle hashes from it.
    ///
    /// The reserve is controlled by the distributor singleton and uses nonce 0.
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

/// The information needed to construct the puzzle of a [`RewardDistributor`](crate::RewardDistributor).
///
/// The distributor uses the reserve finalizer, which recreates the singleton with the new state and
/// spends the [`Reserve`](crate::Reserve) so it holds `total_reserves`.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{
///     Layer, RewardDistributorConstants, RewardDistributorInfo, RewardDistributorState,
///     RewardDistributorType, SpendContext,
/// };
///
/// let ctx = &mut SpendContext::new();
///
/// let constants = RewardDistributorConstants::without_launcher_id(
///     RewardDistributorType::Cat {
///         asset_id: Bytes32::new([1; 32]),
///         hidden_puzzle_hash: None,
///     },
///     Bytes32::new([2; 32]),
///     7 * 24 * 60 * 60,
///     10_000,
///     2 * 60,
///     100,
///     false,
///     700,
///     5_000,
///     Bytes32::new([3; 32]),
/// )
/// .with_launcher_id(Bytes32::new([4; 32]));
/// let info = RewardDistributorInfo::new(RewardDistributorState::initial(1_700_000_000), constants);
///
/// let puzzle = info.into_layers(ctx)?.construct_puzzle(ctx)?;
/// assert_eq!(ctx.tree_hash(puzzle), info.puzzle_hash());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct RewardDistributorInfo {
    /// The current state of the distributor.
    pub state: RewardDistributorState,

    /// The constants of the distributor.
    pub constants: RewardDistributorConstants,
}

impl RewardDistributorInfo {
    /// Creates a new [`RewardDistributorInfo`].
    pub fn new(state: RewardDistributorState, constants: RewardDistributorConstants) -> Self {
        Self { state, constants }
    }

    /// Returns a copy of the info with a different state.
    pub fn with_state(mut self, state: RewardDistributorState) -> Self {
        self.state = state;
        self
    }

    /// Returns the puzzle hashes of the distributor's actions, in the order used for the Merkle tree.
    ///
    /// Managed distributors get the add and remove entry actions, and all other types get the stake
    /// and unstake actions. Refreshable curated NFT distributors also get the refresh action.
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
            && refreshable
        {
            action_puzzle_hashes.push(
                RewardDistributorRefreshAction::from_constants(constants)
                    .tree_hash()
                    .into(),
            );
        }

        action_puzzle_hashes
    }

    /// Returns the layers of the distributor puzzle.
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

    /// Parses a distributor puzzle with the given constants.
    ///
    /// Returns [`None`] if the puzzle isn't an action-layer singleton or its actions don't match the constants.
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

    /// Creates the info from parsed layers, without checking that they match the constants.
    pub fn from_layers(
        layers: &RewardDistributorLayers,
        constants: RewardDistributorConstants,
    ) -> Self {
        Self {
            state: layers.inner_puzzle.state,
            constants,
        }
    }

    /// Returns the full singleton puzzle hash of the distributor.
    pub fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::curry_tree_hash(self.constants.launcher_id, self.inner_puzzle_hash())
    }

    /// Returns the inner (action layer) puzzle hash of the distributor.
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
