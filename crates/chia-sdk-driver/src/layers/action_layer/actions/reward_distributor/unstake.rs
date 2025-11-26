use chia_protocol::Bytes32;
use chia_puzzle_types::{nft::NftRoyaltyTransferPuzzleArgs, singleton::SingletonStruct};
use chia_sdk_types::{
    puzzles::{
        NftToUnlockInfo, NonceWrapperArgs, P2DelegatedBySingletonLayerArgs,
        P2DelegatedBySingletonLayerSolution, RewardDistributorCatUnlockingPuzzleArgs,
        RewardDistributorCatUnlockingPuzzleSolution, RewardDistributorEntrySlotValue,
        RewardDistributorNftsUnlockingPuzzleArgs, RewardDistributorSlotNonce,
        RewardDistributorUnstakeActionArgs, RewardDistributorUnstakeActionSolution,
    },
    Conditions, Mod,
};
use clvm_traits::{clvm_quote, clvm_tuple};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    Cat, CatMaker, CatSpend, DriverError, Layer, Nft, P2DelegatedBySingletonLayer,
    RewardDistributor, RewardDistributorConstants, RewardDistributorType, SingletonAction, Slot,
    Spend, SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorUnstakeAction {
    pub launcher_id: Bytes32,
    pub max_second_offset: u64,
    pub precision: u64,
    pub distributor_type: RewardDistributorType,
}

impl ToTreeHash for RewardDistributorUnstakeAction {
    fn tree_hash(&self) -> TreeHash {
        Self::new_args_treehash(
            self.launcher_id,
            self.max_second_offset,
            self.precision,
            self.distributor_type,
        )
        .curry_tree_hash()
    }
}

impl SingletonAction<RewardDistributor> for RewardDistributorUnstakeAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            max_second_offset: constants.max_seconds_offset,
            precision: constants.precision,
            distributor_type: constants.reward_distributor_type,
        }
    }
}

impl RewardDistributorUnstakeAction {
    pub fn unlock_puzzle(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        distributor_type: RewardDistributorType,
    ) -> Result<NodePtr, DriverError> {
        match distributor_type {
            RewardDistributorType::NftCollection { .. }
            | RewardDistributorType::CuratedNft { .. } => ctx.curry(
                RewardDistributorNftsUnlockingPuzzleArgs::new(Self::my_p2_puzzle_hash(launcher_id)),
            ),
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

                ctx.curry(RewardDistributorCatUnlockingPuzzleArgs::new(
                    cat_maker_puzzle,
                    Self::my_p2_puzzle_hash(launcher_id),
                ))
            }
            RewardDistributorType::Managed { .. } => Err(DriverError::Custom(
                "Unstake action not available in this mode".to_string(),
            )),
        }
    }

    pub fn new_args(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        max_second_offset: u64,
        precision: u64,
        distributor_type: RewardDistributorType,
    ) -> Result<RewardDistributorUnstakeActionArgs<NodePtr>, DriverError> {
        Ok(RewardDistributorUnstakeActionArgs {
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
            precision,
            unlock_puzzle: Self::unlock_puzzle(ctx, launcher_id, distributor_type)?,
        })
    }

    pub fn new_args_treehash(
        launcher_id: Bytes32,
        max_second_offset: u64,
        precision: u64,
        distributor_type: RewardDistributorType,
    ) -> RewardDistributorUnstakeActionArgs<TreeHash> {
        RewardDistributorUnstakeActionArgs {
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
            precision,
            unlock_puzzle: match distributor_type {
                RewardDistributorType::NftCollection { .. }
                | RewardDistributorType::CuratedNft { .. } => {
                    RewardDistributorNftsUnlockingPuzzleArgs::new(Self::my_p2_puzzle_hash(
                        launcher_id,
                    ))
                    .curry_tree_hash()
                }
                RewardDistributorType::Cat {
                    asset_id,
                    hidden_puzzle_hash,
                } => RewardDistributorCatUnlockingPuzzleArgs::new(
                    match hidden_puzzle_hash {
                        Some(hidden_puzzle_hash) => CatMaker::Revocable {
                            tail_hash_hash: asset_id.tree_hash(),
                            hidden_puzzle_hash_hash: hidden_puzzle_hash.tree_hash(),
                        },
                        None => CatMaker::Default {
                            tail_hash_hash: asset_id.tree_hash(),
                        },
                    }
                    .curry_tree_hash(),
                    Self::my_p2_puzzle_hash(launcher_id),
                )
                .curry_tree_hash(),
                RewardDistributorType::Managed { .. } => TreeHash::new([0; 32]),
            },
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
            self.precision,
            self.distributor_type,
        )?;

        ctx.curry(args)
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorUnstakeActionSolution<NodePtr>>(solution)?;

        Ok(solution.entry_slot)
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        distributor_type: RewardDistributorType,
        ephemeral_state: NodePtr,
        solution: NodePtr,
    ) -> Result<Option<RewardDistributorEntrySlotValue>, DriverError> {
        let solution = ctx.extract::<RewardDistributorUnstakeActionSolution<NodePtr>>(solution)?;
        let actual_unlock_solution = ctx.alloc(&clvm_tuple!(
            ephemeral_state,
            clvm_tuple!(
                solution.entry_slot.payout_puzzle_hash,
                solution.unlock_puzzle_solution
            )
        ))?;
        let unlock_puzzle = Self::unlock_puzzle(ctx, launcher_id, distributor_type)?;

        let unlock_puzzle_result = ctx.run(unlock_puzzle, actual_unlock_solution)?;
        let removed_shares = ctx.extract::<(u64, NodePtr)>(unlock_puzzle_result)?.0;

        if solution.entry_slot.shares == removed_shares {
            return Ok(None);
        }

        Ok(Some(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_slot.payout_puzzle_hash,
            initial_cumulative_payout: solution.entry_slot.initial_cumulative_payout,
            shares: solution.entry_slot.shares - removed_shares,
        }))
    }

    pub fn spend_for_locked_nfts(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
        locked_nfts: &[Nft],
        locked_nft_shares: &[u64],
    ) -> Result<(Conditions, u64), DriverError> {
        // u64 = last payment amount
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        // compute messages that the custody puzzle needs to send
        let mut nfts_unlock_info = Vec::with_capacity(locked_nfts.len());
        let mut remove_entry_conditions = Conditions::new();
        let mut removed_shares = 0;

        let distributor_singleton_struct_hash: Bytes32 =
            SingletonStruct::new(self.launcher_id).tree_hash().into();
        let registry_inner_puzzle_hash = distributor.info.inner_puzzle_hash();

        for (locked_nft, locked_nft_share) in locked_nfts.iter().zip(locked_nft_shares.iter()) {
            let mut unstake_message = locked_nft.info.launcher_id.to_vec();
            unstake_message.insert(0, b'u');

            remove_entry_conditions = remove_entry_conditions.send_message(
                18,
                unstake_message.into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            );

            nfts_unlock_info.push(NftToUnlockInfo {
                nft_launcher_id: locked_nft.info.launcher_id,
                nft_parent_id: locked_nft.coin.parent_coin_info,
                nft_metadata_hash: locked_nft.info.metadata.tree_hash().into(),
                nft_metadata_updater_hash_hash: locked_nft
                    .info
                    .metadata_updater_puzzle_hash
                    .tree_hash()
                    .into(),
                nft_owner: locked_nft.info.current_owner,
                nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                    locked_nft.info.launcher_id,
                    locked_nft.info.royalty_puzzle_hash,
                    locked_nft.info.royalty_basis_points,
                )
                .into(),
                nft_shares: *locked_nft_share,
            });

            removed_shares += locked_nft_share;

            // spend locked NFT
            let nft_p2 = P2DelegatedBySingletonLayer::new(distributor_singleton_struct_hash, 1);
            let nft_inner_puzzle = nft_p2.construct_puzzle(ctx)?;
            // don't forget about the nonce wrapper!
            let nft_nonce: (Bytes32, u64) =
                clvm_tuple!(entry_slot.info.value.payout_puzzle_hash, *locked_nft_share);
            let nft_inner_puzzle = ctx.curry(NonceWrapperArgs::<(Bytes32, u64), NodePtr> {
                nonce: nft_nonce,
                inner_puzzle: nft_inner_puzzle,
            })?;

            let hint = ctx.hint(entry_slot.info.value.payout_puzzle_hash)?;
            let delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
                entry_slot.info.value.payout_puzzle_hash,
                1,
                hint,
            )))?;
            let nft_inner_solution = nft_p2.construct_solution(
                ctx,
                P2DelegatedBySingletonLayerSolution::<NodePtr, NodePtr> {
                    singleton_inner_puzzle_hash: registry_inner_puzzle_hash.into(),
                    delegated_puzzle,
                    delegated_solution: NodePtr::NIL,
                },
            )?;

            let _new_nft =
                locked_nft.spend(ctx, Spend::new(nft_inner_puzzle, nft_inner_solution))?;
        }

        remove_entry_conditions =
            remove_entry_conditions.assert_concurrent_puzzle(entry_slot.coin.puzzle_hash);

        // spend self
        let entry_payout_amount_precision = u128::from(removed_shares)
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);
        let entry_payout_amount =
            u64::try_from(entry_payout_amount_precision / u128::from(self.precision))?;
        let action_solution = ctx.alloc(&RewardDistributorUnstakeActionSolution {
            unlock_puzzle_solution: nfts_unlock_info,
            entry_payout_amount,
            payout_rounding_error: entry_payout_amount_precision % u128::from(self.precision),
            entry_slot: entry_slot.info.value,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        Ok((remove_entry_conditions, entry_payout_amount))
    }

    pub fn spend_for_locked_cats(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        entry_slot: Slot<RewardDistributorEntrySlotValue>,
        locked_cat: Cat,
    ) -> Result<(Conditions, u64), DriverError> {
        // u64 = last payment amount
        let my_state = distributor.pending_spend.latest_state.1;
        let entry_slot = distributor.actual_entry_slot_value(entry_slot);

        // compute messages that the custody puzzle needs to send
        let locked_cat_coin = locked_cat.coin;
        let mut unstake_message = locked_cat_coin.parent_coin_info.to_vec();
        unstake_message.insert(0, b'r');

        let remove_entry_conditions = Conditions::new()
            .send_message(
                18,
                unstake_message.into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            )
            .assert_concurrent_puzzle(entry_slot.coin.puzzle_hash);

        let distributor_singleton_struct_hash: Bytes32 =
            SingletonStruct::new(self.launcher_id).tree_hash().into();
        let registry_inner_puzzle_hash = distributor.info.inner_puzzle_hash();

        // spend locked CAT
        let cat_p2 = P2DelegatedBySingletonLayer::new(distributor_singleton_struct_hash, 1);
        let cat_inner_puzzle = cat_p2.construct_puzzle(ctx)?;
        // don't forget about the nonce wrapper!
        let cat_nonce: Bytes32 = clvm_tuple!(clvm_tuple!(
            entry_slot.info.value.payout_puzzle_hash,
            locked_cat_coin.amount
        ))
        .tree_hash()
        .into();
        let cat_inner_puzzle = ctx.curry(NonceWrapperArgs::<Bytes32, NodePtr> {
            nonce: cat_nonce,
            inner_puzzle: cat_inner_puzzle,
        })?;

        let hint = ctx.hint(entry_slot.info.value.payout_puzzle_hash)?;
        let delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
            entry_slot.info.value.payout_puzzle_hash,
            locked_cat_coin.amount,
            hint,
        )))?;
        let cat_inner_solution = cat_p2.construct_solution(
            ctx,
            P2DelegatedBySingletonLayerSolution::<NodePtr, NodePtr> {
                singleton_inner_puzzle_hash: registry_inner_puzzle_hash.into(),
                delegated_puzzle,
                delegated_solution: NodePtr::NIL,
            },
        )?;

        let _new_cats = Cat::spend_all(
            ctx,
            &[CatSpend::new(
                locked_cat,
                Spend::new(cat_inner_puzzle, cat_inner_solution),
            )],
        )?;

        // spend self
        let entry_payout_amount_precision = u128::from(locked_cat_coin.amount)
            * (my_state.round_reward_info.cumulative_payout
                - entry_slot.info.value.initial_cumulative_payout);
        let entry_payout_amount =
            u64::try_from(entry_payout_amount_precision / u128::from(self.precision))?;
        let action_solution = ctx.alloc(&RewardDistributorUnstakeActionSolution {
            unlock_puzzle_solution: RewardDistributorCatUnlockingPuzzleSolution {
                cat_parent_id: locked_cat_coin.parent_coin_info,
                cat_amount: locked_cat_coin.amount,
                cat_shares: locked_cat_coin.amount,
                cat_maker_solution_rest: (),
            },
            entry_payout_amount,
            payout_rounding_error: entry_payout_amount_precision % u128::from(self.precision),
            entry_slot: entry_slot.info.value,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend entry slot
        entry_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;

        Ok((remove_entry_conditions, entry_payout_amount))
    }
}
