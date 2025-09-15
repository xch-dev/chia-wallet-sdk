use chia_protocol::Bytes32;
use chia_puzzle_types::{
    nft::NftRoyaltyTransferPuzzleArgs,
    offer::{NotarizedPayment, Payment},
    singleton::SingletonStruct,
};
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use chia_sdk_types::{
    announcement_id,
    puzzles::{
        NftLauncherProof, NonceWrapperArgs, P2DelegatedBySingletonLayerArgs,
        RewardDistributorCatLockingPuzzleArgs, RewardDistributorEntrySlotValue,
        RewardDistributorNftsFromDidLockingPuzzleArgs,
        RewardDistributorNftsFromDlLockingPuzzleArgs, RewardDistributorSlotNonce,
        RewardDistributorStakeActionArgs, RewardDistributorStakeActionSolution,
        NONCE_WRAPPER_PUZZLE_HASH,
    },
    Conditions, Mod,
};
use clvm_traits::clvm_tuple;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    Asset, CatMaker, DriverError, HashedPtr, Nft, RewardDistributor, RewardDistributorConstants,
    RewardDistributorState, RewardDistributorType, SingletonAction, Slot, Spend, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorStakeAction {
    pub launcher_id: Bytes32,
    pub max_second_offset: u64,
    pub distributor_type: RewardDistributorType,
}

impl ToTreeHash for RewardDistributorStakeAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args_treehash(
            self.launcher_id,
            self.max_second_offset,
            self.distributor_type,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorStakeAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            max_second_offset: constants.max_seconds_offset,
            distributor_type: constants.reward_distributor_type,
        }
    }
}

impl RewardDistributorStakeAction {
    pub fn new_args(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        max_second_offset: u64,
        distributor_type: RewardDistributorType,
    ) -> Result<RewardDistributorStakeActionArgs<NodePtr>, DriverError> {
        let lock_puzzle = match distributor_type {
            RewardDistributorType::Managed {
                manager_singleton_launcher_id: _,
            } => Err(DriverError::Custom(
                "Stake action not available in managed mode".to_string(),
            )),
            RewardDistributorType::NftCollection {
                collection_did_launcher_id,
            } => ctx.curry(&RewardDistributorNftsFromDidLockingPuzzleArgs {
                did_singleton_struct: SingletonStruct::new(collection_did_launcher_id),
                nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
                nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
                offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
                nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
                my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
            }),
            RewardDistributorType::WeightedNft { store_launcher_id } => {
                ctx.curry(&RewardDistributorNftsFromDlLockingPuzzleArgs {
                    dl_singleton_struct: SingletonStruct::new(store_launcher_id),
                    nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
                    nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
                    offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
                    nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
                    my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
                })
            }
            RewardDistributorType::Cat {
                asset_id,
                hidden_puzzle_hash,
            } => {
                let cat_maker = if let Some(hidden_puzzle_hash) = hidden_puzzle_hash {
                    CatMaker::Revocable {
                        tail_hash_hash: asset_id.tree_hash(),
                        hidden_puzzle_hash_hash: hidden_puzzle_hash.tree_hash(),
                    }
                } else {
                    CatMaker::Default {
                        tail_hash_hash: asset_id.tree_hash(),
                    }
                };
                let cat_maker_puzzle = cat_maker.get_puzzle(ctx)?;

                ctx.curry(&RewardDistributorCatLockingPuzzleArgs {
                    cat_maker: cat_maker_puzzle,
                    offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
                    nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
                    my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
                })
            }
        }?;

        Ok(RewardDistributorStakeActionArgs {
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
            lock_puzzle,
        })
    }

    pub fn new_args_treehash(
        launcher_id: Bytes32,
        max_second_offset: u64,
        distributor_type: RewardDistributorType,
    ) -> RewardDistributorStakeActionArgs<TreeHash> {
        let lock_puzzle_hash = match distributor_type {
            RewardDistributorType::Managed {
                manager_singleton_launcher_id: _,
            } => TreeHash::new([0; 32]),
            RewardDistributorType::NftCollection {
                collection_did_launcher_id,
            } => RewardDistributorNftsFromDidLockingPuzzleArgs {
                did_singleton_struct: SingletonStruct::new(collection_did_launcher_id),
                nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
                nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
                offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
                nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
                my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
            }
            .curry_tree_hash(),
            RewardDistributorType::WeightedNft { store_launcher_id } => {
                RewardDistributorNftsFromDlLockingPuzzleArgs {
                    dl_singleton_struct: SingletonStruct::new(store_launcher_id),
                    nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
                    nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
                    offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
                    nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
                    my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
                }
                .curry_tree_hash()
            }
            RewardDistributorType::Cat {
                asset_id,
                hidden_puzzle_hash,
            } => {
                let cat_maker = if let Some(hidden_puzzle_hash) = hidden_puzzle_hash {
                    CatMaker::Revocable {
                        tail_hash_hash: asset_id.tree_hash(),
                        hidden_puzzle_hash_hash: hidden_puzzle_hash.tree_hash(),
                    }
                } else {
                    CatMaker::Default {
                        tail_hash_hash: asset_id.tree_hash(),
                    }
                };
                let cat_maker_puzzle_hash = cat_maker.curry_tree_hash();

                RewardDistributorCatLockingPuzzleArgs {
                    cat_maker: cat_maker_puzzle_hash,
                    offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
                    nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
                    my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
                }
                .curry_tree_hash()
            }
        };

        RewardDistributorStakeActionArgs {
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
            lock_puzzle: lock_puzzle_hash,
        }
    }

    pub fn my_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
        P2DelegatedBySingletonLayerArgs::curry_tree_hash(
            SingletonStruct::new(launcher_id).tree_hash().into(),
            1,
        )
        .into()
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let args = Self::new_args(
            ctx,
            self.launcher_id,
            self.max_second_offset,
            self.distributor_type,
        )?;

        ctx.curry(&args)
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        state: &RewardDistributorState,
        distributor_type: RewardDistributorType,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorStakeActionSolution<NodePtr>>(solution)?;

        let lock_puzzle = Self::new_args(ctx, Bytes32::default(), 1, distributor_type)?.lock_puzzle;
        let actual_lock_solution = ctx.alloc(&(
            1,
            (
                solution.entry_custody_puzzle_hash,
                solution.lock_puzzle_solution,
            ),
        ))?;

        let lock_puzzle_output = ctx.run(lock_puzzle, actual_lock_solution)?;
        let (new_shares, _conds): (u64, NodePtr) = ctx.extract(lock_puzzle_output)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_custody_puzzle_hash,
            initial_cumulative_payout: state.round_reward_info.cumulative_payout,
            shares: solution.existing_slot_shares + new_shares,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        current_nft: Nft,
        nft_launcher_proof: NftLauncherProof,
        entry_custody_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, NotarizedPayment, Nft), DriverError> {
        let ephemeral_counter =
            ctx.extract::<HashedPtr>(distributor.pending_spend.latest_state.0)?;
        let my_id = distributor.coin.coin_id();

        // calculate notarized payment
        let my_p2_treehash = Self::my_p2_puzzle_hash(self.launcher_id).into();
        let payment_puzzle_hash: Bytes32 = CurriedProgram {
            program: NONCE_WRAPPER_PUZZLE_HASH,
            args: NonceWrapperArgs::<Bytes32, TreeHash> {
                nonce: entry_custody_puzzle_hash,
                inner_puzzle: my_p2_treehash,
            },
        }
        .tree_hash()
        .into();
        let notarized_payment = NotarizedPayment {
            nonce: clvm_tuple!(ephemeral_counter.tree_hash(), my_id)
                .tree_hash()
                .into(),
            payments: vec![Payment::new(
                payment_puzzle_hash,
                1,
                ctx.hint(payment_puzzle_hash)?,
            )],
        };

        // spend self
        let nft = current_nft.child(
            SETTLEMENT_PAYMENT_HASH.into(),
            None,
            current_nft.info.metadata,
            current_nft.amount(),
        );
        let action_solution = ctx.alloc(&RewardDistributorStakeActionSolution {
            my_id,
            nft_metadata_hash: nft.info.metadata.tree_hash().into(),
            nft_metadata_updater_hash_hash: nft
                .info
                .metadata_updater_puzzle_hash
                .tree_hash()
                .into(),
            nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                nft.info.launcher_id,
                nft.info.royalty_puzzle_hash,
                nft.info.royalty_basis_points,
            )
            .into(),
            nft_launcher_proof,
            entry_custody_puzzle_hash,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
        let msg: Bytes32 = ctx.tree_hash(notarized_payment_ptr).into();
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                announcement_id(nft.coin.puzzle_hash, msg),
            )),
            notarized_payment,
            nft.child(payment_puzzle_hash, None, nft.info.metadata, nft.amount()),
        ))
    }
}
