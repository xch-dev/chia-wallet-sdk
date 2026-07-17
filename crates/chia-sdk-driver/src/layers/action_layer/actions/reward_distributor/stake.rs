use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    nft::NftRoyaltyTransferPuzzleArgs,
    offer::{NotarizedPayment, Payment},
    singleton::{SingletonArgs, SingletonStruct},
};
use chia_sdk_types::{
    Conditions, MerkleProof, Mod, announcement_id,
    puzzles::{
        NONCE_WRAPPER_PUZZLE_HASH, NftLauncherProof, NonceWrapperArgs,
        P2DelegatedBySingletonLayerArgs, RewardDistributorCatLockingPuzzleArgs,
        RewardDistributorCatLockingPuzzleSolution, RewardDistributorEntrySlotValue,
        RewardDistributorNftsFromDidLockingPuzzleArgs,
        RewardDistributorNftsFromDidLockingPuzzleSolution,
        RewardDistributorNftsFromDlLockingPuzzleArgs,
        RewardDistributorNftsFromDlLockingPuzzleSolution, RewardDistributorSlotNonce,
        RewardDistributorStakeActionArgs, RewardDistributorStakeActionSolution,
        StakeNftFromDidInfo, StakeNftFromDlInfo,
    },
};
use clvm_traits::{ToClvm, clvm_tuple};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    Asset, Cat, CatMaker, DriverError, HashedPtr, Nft, RewardDistributor,
    RewardDistributorConstants, RewardDistributorCreatedAnnouncementPrefix,
    RewardDistributorNftStakeEntry, RewardDistributorReceivedMessagePrefix,
    RewardDistributorStakeActionLog, RewardDistributorState, RewardDistributorStateTransition,
    RewardDistributorType, SingletonAction, Slot, Spend, SpendContext,
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
    pub fn nft_launcher_id_from_proof(
        did_launcher_id: Bytes32,
        proof: &NftLauncherProof,
    ) -> Bytes32 {
        let mut coin_id = Coin::new(
            proof.did_proof.parent_parent_coin_info,
            SingletonArgs::curry_tree_hash(
                did_launcher_id,
                proof.did_proof.parent_inner_puzzle_hash.into(),
            )
            .into(),
            proof.did_proof.parent_amount,
        )
        .coin_id();

        for intermediary in proof.intermediary_coin_proofs.iter().rev() {
            coin_id =
                Coin::new(coin_id, intermediary.full_puzzle_hash, intermediary.amount).coin_id();
        }

        coin_id
    }

    fn nft_entries_from_stake_lock_solution(
        ctx: &SpendContext,
        lock_puzzle_solution: NodePtr,
        distributor_type: RewardDistributorType,
    ) -> Result<Option<Vec<RewardDistributorNftStakeEntry>>, DriverError> {
        match distributor_type {
            RewardDistributorType::NftCollection {
                collection_did_launcher_id,
            } => {
                let lock_solution = ctx
                    .extract::<RewardDistributorNftsFromDidLockingPuzzleSolution>(
                        lock_puzzle_solution,
                    )?;
                let entries = lock_solution
                    .nft_infos
                    .iter()
                    .map(|info| RewardDistributorNftStakeEntry {
                        launcher_id: Self::nft_launcher_id_from_proof(
                            collection_did_launcher_id,
                            &info.nft_launcher_proof,
                        ),
                        shares: 1,
                    })
                    .collect();
                Ok(Some(entries))
            }
            RewardDistributorType::CuratedNft { .. } => {
                let lock_solution = ctx
                    .extract::<RewardDistributorNftsFromDlLockingPuzzleSolution>(
                        lock_puzzle_solution,
                    )?;
                Ok(Some(
                    lock_solution
                        .nft_infos
                        .iter()
                        .map(|info: &StakeNftFromDlInfo| RewardDistributorNftStakeEntry {
                            launcher_id: info.nft_launcher_id,
                            shares: info.nft_shares,
                        })
                        .collect(),
                ))
            }
            _ => Ok(None),
        }
    }

    fn cat_amount_from_stake_lock_solution(
        ctx: &SpendContext,
        lock_puzzle_solution: NodePtr,
        distributor_type: RewardDistributorType,
    ) -> Result<Option<u64>, DriverError> {
        match distributor_type {
            RewardDistributorType::Cat { .. } => {
                let lock_solution = ctx
                    .extract::<RewardDistributorCatLockingPuzzleSolution<NodePtr>>(
                        lock_puzzle_solution,
                    )?;
                Ok(Some(lock_solution.cat_amount))
            }
            _ => Ok(None),
        }
    }

    fn stake_cat_and_nft_from_solution(
        ctx: &SpendContext,
        solution: NodePtr,
        distributor_type: RewardDistributorType,
    ) -> Result<(Option<u64>, Option<Vec<RewardDistributorNftStakeEntry>>), DriverError> {
        let solution = ctx.extract::<RewardDistributorStakeActionSolution<NodePtr>>(solution)?;
        let cat_amount = Self::cat_amount_from_stake_lock_solution(
            ctx,
            solution.lock_puzzle_solution,
            distributor_type,
        )?;
        let nft_entries = Self::nft_entries_from_stake_lock_solution(
            ctx,
            solution.lock_puzzle_solution,
            distributor_type,
        )?;
        Ok((cat_amount, nft_entries))
    }

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
            } => ctx.curry(RewardDistributorNftsFromDidLockingPuzzleArgs::new(
                collection_did_launcher_id,
                Self::my_p2_puzzle_hash(launcher_id),
            )),
            RewardDistributorType::CuratedNft {
                store_launcher_id,
                refreshable: _,
            } => ctx.curry(RewardDistributorNftsFromDlLockingPuzzleArgs::new(
                store_launcher_id,
                Self::my_p2_puzzle_hash(launcher_id),
            )),
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

                ctx.curry(RewardDistributorCatLockingPuzzleArgs::new(
                    cat_maker_puzzle,
                    Self::my_p2_puzzle_hash(launcher_id),
                ))
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
            } => RewardDistributorNftsFromDidLockingPuzzleArgs::new(
                collection_did_launcher_id,
                Self::my_p2_puzzle_hash(launcher_id),
            )
            .curry_tree_hash(),
            RewardDistributorType::CuratedNft {
                store_launcher_id,
                refreshable: _,
            } => RewardDistributorNftsFromDlLockingPuzzleArgs::new(
                store_launcher_id,
                Self::my_p2_puzzle_hash(launcher_id),
            )
            .curry_tree_hash(),
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

                RewardDistributorCatLockingPuzzleArgs::new(
                    cat_maker_puzzle_hash,
                    Self::my_p2_puzzle_hash(launcher_id),
                )
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

        ctx.curry(args)
    }

    pub fn created_slot_value<LPS>(
        ctx: &mut SpendContext,
        state: &RewardDistributorState,
        distributor_type: RewardDistributorType,
        solution: &RewardDistributorStakeActionSolution<LPS>,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError>
    where
        LPS: ToClvm<Allocator> + Clone,
    {
        let lock_puzzle = Self::new_args(ctx, Bytes32::default(), 1, distributor_type)?.lock_puzzle;
        let actual_lock_solution = ctx.alloc(&(
            1,
            (
                solution.entry_custody_puzzle_hash,
                solution.lock_puzzle_solution.clone(),
            ),
        ))?;

        let lock_puzzle_output = ctx.run(lock_puzzle, actual_lock_solution)?;
        let (new_shares, _conds): (u64, NodePtr) = ctx.extract(lock_puzzle_output)?;

        Ok(RewardDistributorEntrySlotValue {
            counter: u64::try_from(solution.existing_slot_counter + 1)?,
            payout_puzzle_hash: solution.entry_custody_puzzle_hash,
            initial_cumulative_payout: state.round_reward_info.cumulative_payout,
            shares: solution.existing_slot_shares + new_shares,
        })
    }

    pub fn get_log(
        ctx: &mut SpendContext,
        solution: NodePtr,
        changes: RewardDistributorStateTransition,
        distributor_type: RewardDistributorType,
    ) -> Result<RewardDistributorStakeActionLog, DriverError> {
        let stake_solution =
            ctx.extract::<RewardDistributorStakeActionSolution<NodePtr>>(solution)?;

        let spent_entry_slot = if stake_solution.existing_slot_counter == -1i128 {
            None
        } else {
            Some(RewardDistributorEntrySlotValue {
                counter: u64::try_from(stake_solution.existing_slot_counter)?,
                payout_puzzle_hash: stake_solution.entry_custody_puzzle_hash,
                initial_cumulative_payout: stake_solution.existing_slot_cumulative_payout,
                shares: stake_solution.existing_slot_shares,
            })
        };

        let created_entry_slot =
            Self::created_slot_value(ctx, &changes.old_state, distributor_type, &stake_solution)?;

        let (cat_amount, nft_entries) =
            Self::stake_cat_and_nft_from_solution(ctx, solution, distributor_type)?;

        Ok(RewardDistributorStakeActionLog {
            spent_entry_slot,
            created_entry_slot,
            cat_amount,
            nft_entries,
            changes,
        })
    }

    #[allow(clippy::cast_possible_wrap)]
    pub fn spend_for_collection_nft_mode(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        offered_nfts: &[Nft],
        nft_launcher_proofs: &[NftLauncherProof],
        entry_custody_puzzle_hash: Bytes32,
        existing_slot: Option<Slot<RewardDistributorEntrySlotValue>>,
    ) -> Result<(Conditions, Vec<NotarizedPayment>, Vec<Nft>), DriverError> {
        let ephemeral_counter =
            ctx.extract::<HashedPtr>(distributor.pending_spend.latest_state.0)?;
        let my_id = distributor.coin.coin_id();

        // calculate notarized payments; spend said nfts
        let my_p2 = Self::my_p2_puzzle_hash(self.launcher_id);
        let my_p2_treehash: TreeHash = my_p2.into();
        let payment_puzzle_hash: Bytes32 = CurriedProgram {
            program: NONCE_WRAPPER_PUZZLE_HASH,
            args: NonceWrapperArgs::<(Bytes32, u64), TreeHash> {
                nonce: clvm_tuple!(entry_custody_puzzle_hash, 1),
                inner_puzzle: my_p2_treehash,
            },
        }
        .tree_hash()
        .into();

        let mut notarized_payments = Vec::with_capacity(offered_nfts.len());
        let mut created_nfts = Vec::with_capacity(offered_nfts.len());
        let mut nft_infos = Vec::with_capacity(offered_nfts.len());
        let mut security_conditions = Conditions::new();
        for i in 0..offered_nfts.len() {
            let nonce: Bytes32 = clvm_tuple!(i, clvm_tuple!(ephemeral_counter.tree_hash(), my_id))
                .tree_hash()
                .into();
            let np = NotarizedPayment {
                // i = cumulative shares until now since each NFT has a weight of 1 in the Collection NFT mode
                nonce,
                payments: vec![Payment::new(
                    payment_puzzle_hash,
                    1,
                    ctx.hint(
                        clvm_tuple!(entry_custody_puzzle_hash, my_p2)
                            .tree_hash()
                            .into(),
                    )?,
                )],
            };
            let notarized_payment_ptr = ctx.alloc(&np)?;
            notarized_payments.push(np);

            created_nfts.push(offered_nfts[i].child(
                payment_puzzle_hash,
                offered_nfts[i].info.current_owner,
                offered_nfts[i].info.metadata,
                offered_nfts[i].coin.amount,
            ));

            nft_infos.push(StakeNftFromDidInfo {
                nft_metadata_hash: offered_nfts[i].info.metadata.tree_hash().into(),
                nft_metadata_updater_hash_hash: offered_nfts[i]
                    .info
                    .metadata_updater_puzzle_hash
                    .tree_hash()
                    .into(),
                nft_owner: offered_nfts[i].info.current_owner,
                nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                    offered_nfts[i].info.launcher_id,
                    offered_nfts[i].info.royalty_puzzle_hash,
                    offered_nfts[i].info.royalty_basis_points,
                )
                .into(),
                nft_launcher_proof: nft_launcher_proofs[i].clone(),
            });

            let msg: Bytes32 = ctx.tree_hash(notarized_payment_ptr).into();
            security_conditions = security_conditions.assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                RewardDistributorCreatedAnnouncementPrefix::stake_lock(announcement_id(
                    offered_nfts[i].coin.puzzle_hash,
                    msg,
                )),
            ));
        }

        let existing_slot = existing_slot.map(|s| distributor.actual_entry_slot_value(s));

        // spend self
        let lock_puzzle_solution = RewardDistributorNftsFromDidLockingPuzzleSolution {
            my_id: distributor.coin.coin_id(),
            nft_infos,
        };
        let action_solution = &RewardDistributorStakeActionSolution {
            lock_puzzle_solution,
            existing_slot_counter: existing_slot
                .as_ref()
                .map_or(-1i128, |s| i128::from(s.info.value.counter)),
            entry_custody_puzzle_hash,
            existing_slot_cumulative_payout: existing_slot
                .as_ref()
                .map_or(0, |s| s.info.value.initial_cumulative_payout),
            existing_slot_shares: existing_slot.as_ref().map_or(0, |s| s.info.value.shares),
        };
        let action_puzzle = self.construct_puzzle(ctx)?;

        // if needed, spend existing slot
        if let Some(existing_slot) = existing_slot {
            let rewards_to_give_up = u128::from(existing_slot.info.value.shares)
                * (distributor
                    .pending_spend
                    .latest_state
                    .1
                    .round_reward_info
                    .cumulative_payout
                    - existing_slot.info.value.initial_cumulative_payout);
            security_conditions = security_conditions.send_message(
                18,
                RewardDistributorReceivedMessagePrefix::stake(rewards_to_give_up).into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            );
            existing_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;
        }

        // ensure new slot is properly created
        let new_slot_value = Self::created_slot_value(
            ctx,
            &distributor.pending_spend.latest_state.1,
            self.distributor_type,
            action_solution,
        )?;
        security_conditions = security_conditions.assert_puzzle_announcement(announcement_id(
            distributor.coin.puzzle_hash,
            RewardDistributorCreatedAnnouncementPrefix::stake_slot(new_slot_value.tree_hash()),
        ));
        let action_solution = ctx.alloc(&action_solution)?;
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((security_conditions, notarized_payments, created_nfts))
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::cast_possible_wrap)]
    pub fn spend_for_curated_nft_mode(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        offered_nfts: &[Nft],
        nft_shares: &[u64],
        inclusion_proofs: &[MerkleProof],
        entry_custody_puzzle_hash: Bytes32,
        existing_slot: Option<Slot<RewardDistributorEntrySlotValue>>,
        dl_root_hash: Bytes32,
        dl_metadata_rest_hash: Option<Bytes32>,
        dl_metadata_updater_hash_hash: Bytes32,
        dl_inner_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, Vec<NotarizedPayment>, Vec<Nft>), DriverError> {
        let ephemeral_counter =
            ctx.extract::<HashedPtr>(distributor.pending_spend.latest_state.0)?;
        let my_id = distributor.coin.coin_id();

        // calculate notarized payments; spend said nfts
        let my_p2 = Self::my_p2_puzzle_hash(self.launcher_id);
        let my_p2_treehash: TreeHash = my_p2.into();

        let mut notarized_payments = Vec::with_capacity(offered_nfts.len());
        let mut created_nfts = Vec::with_capacity(offered_nfts.len());
        let mut nft_infos = Vec::with_capacity(offered_nfts.len());
        let mut security_conditions = Conditions::new();
        let mut total_shares_until_now = 0;
        for i in 0..offered_nfts.len() {
            let payment_puzzle_hash: Bytes32 = CurriedProgram {
                program: NONCE_WRAPPER_PUZZLE_HASH,
                args: NonceWrapperArgs::<(Bytes32, u64), TreeHash> {
                    nonce: clvm_tuple!(entry_custody_puzzle_hash, nft_shares[i]),
                    inner_puzzle: my_p2_treehash,
                },
            }
            .tree_hash()
            .into();

            let np = NotarizedPayment {
                // NFTs may have different weights in curated NFT mode
                nonce: clvm_tuple!(
                    total_shares_until_now,
                    clvm_tuple!(ephemeral_counter.tree_hash(), my_id)
                )
                .tree_hash()
                .into(),
                payments: vec![Payment::new(
                    payment_puzzle_hash,
                    1,
                    ctx.hint(
                        clvm_tuple!(entry_custody_puzzle_hash, my_p2)
                            .tree_hash()
                            .into(),
                    )?,
                )],
            };
            let notarized_payment_ptr = ctx.alloc(&np)?;
            notarized_payments.push(np);
            total_shares_until_now += nft_shares[i];

            created_nfts.push(offered_nfts[i].child(
                payment_puzzle_hash,
                offered_nfts[i].info.current_owner,
                offered_nfts[i].info.metadata,
                offered_nfts[i].coin.amount,
            ));

            nft_infos.push(StakeNftFromDlInfo {
                nft_launcher_id: offered_nfts[i].info.launcher_id,
                nft_metadata_hash: offered_nfts[i].info.metadata.tree_hash().into(),
                nft_metadata_updater_hash_hash: offered_nfts[i]
                    .info
                    .metadata_updater_puzzle_hash
                    .tree_hash()
                    .into(),
                nft_owner: offered_nfts[i].info.current_owner,
                nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                    offered_nfts[i].info.launcher_id,
                    offered_nfts[i].info.royalty_puzzle_hash,
                    offered_nfts[i].info.royalty_basis_points,
                )
                .into(),
                nft_shares: nft_shares[i],
                nft_inclusion_proof: inclusion_proofs[i].clone(),
            });

            let msg: Bytes32 = ctx.tree_hash(notarized_payment_ptr).into();
            security_conditions = security_conditions.assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                RewardDistributorCreatedAnnouncementPrefix::stake_lock(announcement_id(
                    offered_nfts[i].coin.puzzle_hash,
                    msg,
                )),
            ));
        }

        let existing_slot = existing_slot.map(|s| distributor.actual_entry_slot_value(s));

        // spend self
        let lock_puzzle_solution = RewardDistributorNftsFromDlLockingPuzzleSolution {
            my_id: distributor.coin.coin_id(),
            nft_infos,
            dl_root_hash,
            dl_metadata_rest_hash,
            dl_metadata_updater_hash_hash,
            dl_inner_puzzle_hash,
        };
        let action_solution = RewardDistributorStakeActionSolution {
            lock_puzzle_solution,
            existing_slot_counter: existing_slot
                .as_ref()
                .map_or(-1i128, |s| i128::from(s.info.value.counter)),
            entry_custody_puzzle_hash,
            existing_slot_cumulative_payout: existing_slot
                .as_ref()
                .map_or(0, |s| s.info.value.initial_cumulative_payout),
            existing_slot_shares: existing_slot.as_ref().map_or(0, |s| s.info.value.shares),
        };
        let action_puzzle = self.construct_puzzle(ctx)?;

        // if needed, spend existing slot
        if let Some(existing_slot) = existing_slot {
            let rewards_to_give_up = u128::from(existing_slot.info.value.shares)
                * (distributor
                    .pending_spend
                    .latest_state
                    .1
                    .round_reward_info
                    .cumulative_payout
                    - existing_slot.info.value.initial_cumulative_payout);
            security_conditions = security_conditions.send_message(
                18,
                RewardDistributorReceivedMessagePrefix::stake(rewards_to_give_up).into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            );
            existing_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;
        }

        // ensure new slot is properly created
        let new_slot_value = Self::created_slot_value(
            ctx,
            &distributor.pending_spend.latest_state.1,
            self.distributor_type,
            &action_solution,
        )?;
        security_conditions = security_conditions.assert_puzzle_announcement(announcement_id(
            distributor.coin.puzzle_hash,
            RewardDistributorCreatedAnnouncementPrefix::stake_slot(new_slot_value.tree_hash()),
        ));
        let action_solution = ctx.alloc(&action_solution)?;
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((security_conditions, notarized_payments, created_nfts))
    }

    #[allow(clippy::cast_possible_wrap)]
    pub fn spend_for_cat_mode(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        offered_cat: Cat,
        entry_custody_puzzle_hash: Bytes32,
        existing_slot: Option<Slot<RewardDistributorEntrySlotValue>>,
    ) -> Result<(Conditions, NotarizedPayment, Cat), DriverError> {
        let ephemeral_counter =
            ctx.extract::<HashedPtr>(distributor.pending_spend.latest_state.0)?;
        let my_id = distributor.coin.coin_id();

        // calculate notarized payments; spend said nfts
        let my_p2 = Self::my_p2_puzzle_hash(self.launcher_id);
        let my_p2_treehash: TreeHash = my_p2.into();
        let payment_puzzle_hash: Bytes32 = CurriedProgram {
            program: NONCE_WRAPPER_PUZZLE_HASH,
            args: NonceWrapperArgs::<(Bytes32, u64), TreeHash> {
                nonce: clvm_tuple!(entry_custody_puzzle_hash, offered_cat.amount()),
                inner_puzzle: my_p2_treehash,
            },
        }
        .tree_hash()
        .into();

        let np = NotarizedPayment {
            nonce: clvm_tuple!(ephemeral_counter.tree_hash(), my_id)
                .tree_hash()
                .into(),
            payments: vec![Payment::new(
                payment_puzzle_hash,
                offered_cat.amount(),
                ctx.hint(
                    clvm_tuple!(entry_custody_puzzle_hash, my_p2)
                        .tree_hash()
                        .into(),
                )?,
            )],
        };
        let notarized_payment_ptr = ctx.alloc(&np)?;

        let msg: Bytes32 = ctx.tree_hash(notarized_payment_ptr).into();
        let mut security_conditions =
            Conditions::new().assert_puzzle_announcement(announcement_id(
                distributor.coin.puzzle_hash,
                RewardDistributorCreatedAnnouncementPrefix::stake_lock(announcement_id(
                    offered_cat.coin.puzzle_hash,
                    msg,
                )),
            ));

        let existing_slot = existing_slot.map(|s| distributor.actual_entry_slot_value(s));

        // spend self
        let lock_puzzle_solution = RewardDistributorCatLockingPuzzleSolution {
            my_id: distributor.coin.coin_id(),
            cat_amount: offered_cat.amount(),
            cat_maker_solution_rest: (),
        };
        let action_solution = RewardDistributorStakeActionSolution {
            lock_puzzle_solution,
            existing_slot_counter: existing_slot
                .as_ref()
                .map_or(-1i128, |s| i128::from(s.info.value.counter)),
            entry_custody_puzzle_hash,
            existing_slot_cumulative_payout: existing_slot
                .as_ref()
                .map_or(0, |s| s.info.value.initial_cumulative_payout),
            existing_slot_shares: existing_slot.as_ref().map_or(0, |s| s.info.value.shares),
        };
        let action_puzzle = self.construct_puzzle(ctx)?;

        // if needed, spend existing slot
        if let Some(existing_slot) = existing_slot {
            let rewards_to_give_up = u128::from(existing_slot.info.value.shares)
                * (distributor
                    .pending_spend
                    .latest_state
                    .1
                    .round_reward_info
                    .cumulative_payout
                    - existing_slot.info.value.initial_cumulative_payout);
            security_conditions = security_conditions.send_message(
                18,
                RewardDistributorReceivedMessagePrefix::stake(rewards_to_give_up).into(),
                vec![ctx.alloc(&distributor.coin.puzzle_hash)?],
            );
            existing_slot.spend(ctx, distributor.info.inner_puzzle_hash().into())?;
        }

        // ensure new slot is properly created
        let new_slot_value = Self::created_slot_value(
            ctx,
            &distributor.pending_spend.latest_state.1,
            self.distributor_type,
            &action_solution,
        )?;
        security_conditions = security_conditions.assert_puzzle_announcement(announcement_id(
            distributor.coin.puzzle_hash,
            RewardDistributorCreatedAnnouncementPrefix::stake_slot(new_slot_value.tree_hash()),
        ));
        let action_solution = ctx.alloc(&action_solution)?;
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((
            security_conditions,
            np,
            offered_cat.child(payment_puzzle_hash, offered_cat.amount()),
        ))
    }
}
