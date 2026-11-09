use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    nft::NftRoyaltyTransferPuzzleArgs,
    offer::{NotarizedPayment, Payment},
    singleton::{SingletonArgs, SingletonStruct},
};
use chia_sdk_types::{
    Conditions, MerkleProof, Mod, announcement_id,
    puzzles::{
        NftLauncherProof, P2DelegatedBySingletonLayerArgs, RewardDistributorCatLockingPuzzleArgs,
        RewardDistributorCatLockingPuzzleSolution, RewardDistributorDepositSlotValue,
        RewardDistributorEntrySlotValue, RewardDistributorNftsFromDidLockingPuzzleArgs,
        RewardDistributorNftsFromDidLockingPuzzleSolution,
        RewardDistributorNftsFromDlLockingPuzzleArgs,
        RewardDistributorNftsFromDlLockingPuzzleSolution, RewardDistributorSlotNonce,
        RewardDistributorStakeActionArgs, RewardDistributorStakeActionSolution,
        StakeNftFromDidInfo, StakeNftFromDlInfo,
    },
};
use clvm_traits::{ToClvm, clvm_tuple};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    Asset, Cat, CatMaker, DriverError, HashedPtr, Nft, RewardDistributor,
    RewardDistributorConstants, RewardDistributorCreatedAnnouncementPrefix,
    RewardDistributorNftStakeEntry, RewardDistributorReceivedMessagePrefix,
    RewardDistributorStakeActionLog, RewardDistributorState, RewardDistributorStateTransition,
    RewardDistributorType, SingletonAction, Slot, Spend, SpendContext,
};

/// The reward distributor action that stakes NFTs or CATs to add shares to an entry.
///
/// The staked assets are locked by sending them to [`Self::my_p2_puzzle_hash`] through
/// settlement payments, and a deposit slot is created for each of them so that they can be
/// unstaked with a [`RewardDistributorUnstakeAction`](crate::RewardDistributorUnstakeAction).
/// The entry belongs to a custody puzzle hash, which receives the payouts. If the custody
/// puzzle hash already has an entry, it's replaced with one that has the combined shares, and
/// its unclaimed rewards are given up.
///
/// The lock puzzle depends on the [`RewardDistributorType`]. Staking isn't available in a
/// [`RewardDistributorType::Managed`] distributor.
///
/// Use [`SingletonAction::from_constants`] to create it from a distributor's constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardDistributorStakeAction {
    /// The launcher id of the reward distributor.
    pub launcher_id: Bytes32,
    /// See [`RewardDistributorConstants::max_seconds_offset`](crate::RewardDistributorConstants::max_seconds_offset).
    pub max_second_offset: u64,
    /// The type of the distributor, which determines the lock or unlock puzzle.
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
    /// Computes the launcher id of an NFT minted by the DID with launcher id `did_launcher_id`,
    /// from an [`NftLauncherProof`].
    ///
    /// The proof starts at a coin of the DID singleton, and each intermediary coin proof is the
    /// next coin down to the NFT launcher.
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

    /// Creates the curried arguments of the action puzzle, including the lock puzzle for
    /// `distributor_type`.
    ///
    /// # Errors
    ///
    /// Returns an error for a [`RewardDistributorType::Managed`] distributor, or if a puzzle
    /// can't be allocated.
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
                Self::deposit_slot_1st_curry_hash(launcher_id),
            )),
            RewardDistributorType::CuratedNft {
                store_launcher_id,
                refreshable: _,
            } => ctx.curry(RewardDistributorNftsFromDlLockingPuzzleArgs::new(
                store_launcher_id,
                Self::my_p2_puzzle_hash(launcher_id),
                Self::deposit_slot_1st_curry_hash(launcher_id),
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
                    Self::deposit_slot_1st_curry_hash(launcher_id),
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

    /// Creates the curried arguments of the action puzzle with the lock puzzle as a tree hash.
    ///
    /// For a [`RewardDistributorType::Managed`] distributor, the lock puzzle hash is all zeros.
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
                Self::deposit_slot_1st_curry_hash(launcher_id),
            )
            .curry_tree_hash(),
            RewardDistributorType::CuratedNft {
                store_launcher_id,
                refreshable: _,
            } => RewardDistributorNftsFromDlLockingPuzzleArgs::new(
                store_launcher_id,
                Self::my_p2_puzzle_hash(launcher_id),
                Self::deposit_slot_1st_curry_hash(launcher_id),
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
                    Self::deposit_slot_1st_curry_hash(launcher_id),
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

    /// Computes the puzzle hash that locked assets are sent to: a
    /// [`P2DelegatedBySingletonLayer`](crate::P2DelegatedBySingletonLayer) controlled by the
    /// distributor, with a nonce of 1.
    pub fn my_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
        P2DelegatedBySingletonLayerArgs::curry_tree_hash(
            SingletonStruct::new(launcher_id).tree_hash().into(),
            1,
        )
        .into()
    }

    fn deposit_slot_1st_curry_hash(launcher_id: Bytes32) -> Bytes32 {
        Slot::<()>::first_curry_hash(launcher_id, RewardDistributorSlotNonce::DEPOSIT.to_u64())
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

    /// Computes the entry slot created by a stake action with the given solution.
    ///
    /// The lock puzzle is run to get the number of new shares, which are added to the existing
    /// entry's shares. The new entry's initial cumulative payout is the one in `state`.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock puzzle can't be constructed or fails.
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

    /// Parses an action solution into a [`RewardDistributorStakeActionLog`].
    ///
    /// `changes` is the distributor state before and after the action, and `distributor_type` is
    /// the distributor's type.
    ///
    /// # Errors
    ///
    /// Returns an error if the solution can't be parsed or the lock puzzle fails.
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

        let custody = stake_solution.entry_custody_puzzle_hash;
        let created_deposit_slots = if let Some(cat_amount) = cat_amount {
            vec![RewardDistributorDepositSlotValue::cat(custody, cat_amount)]
        } else if let Some(nft_entries) = nft_entries.as_ref() {
            nft_entries
                .iter()
                .map(|entry| {
                    RewardDistributorDepositSlotValue::nft(custody, entry.shares, entry.launcher_id)
                })
                .collect()
        } else {
            vec![]
        };

        Ok(RewardDistributorStakeActionLog {
            spent_entry_slot,
            created_entry_slot,
            created_deposit_slots,
            cat_amount,
            nft_entries,
            changes,
        })
    }

    /// Stakes `offered_nfts` in a [`RewardDistributorType::NftCollection`] distributor, in the
    /// pending spend of `distributor`. Each NFT has one share.
    ///
    /// `nft_launcher_proofs` proves that each NFT was minted by the collection's DID, in the same
    /// order as `offered_nfts`. The entry belongs to `entry_custody_puzzle_hash`, and
    /// `existing_slot` is its current entry slot, if it has one.
    ///
    /// Besides the action, this spends `existing_slot` if provided. Returns the conditions that
    /// secure the stake, which assert the distributor's announcements for each locked asset
    /// and the new entry slot and, if `existing_slot` is provided, send a message (mode 18)
    /// acknowledging the unclaimed rewards of the existing entry that are given up.
    /// Also returns the [`NotarizedPayment`]s that must be paid with the NFTs, for example in
    /// an offer, and the locked NFTs that will be created.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the spends can't be created.
    ///
    /// # Panics
    ///
    /// Panics if `nft_launcher_proofs` is shorter than `offered_nfts`.
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
                    my_p2,
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
                my_p2,
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

    /// Stakes `offered_nfts` in a [`RewardDistributorType::CuratedNft`] distributor, in the
    /// pending spend of `distributor`.
    ///
    /// `nft_shares` and `inclusion_proofs` give each NFT's shares and its merkle proof in the
    /// curated `DataLayer` store, in the same order as `offered_nfts`. The `dl_*` arguments
    /// describe the store's current root hash and puzzle. The entry belongs to
    /// `entry_custody_puzzle_hash`, and `existing_slot` is its current entry slot, if it has
    /// one.
    ///
    /// Besides the action, this spends `existing_slot` if provided. Returns the conditions that
    /// secure the stake, which assert the distributor's announcements for each locked asset
    /// and the new entry slot and, if `existing_slot` is provided, send a message (mode 18)
    /// acknowledging the unclaimed rewards of the existing entry that are given up.
    /// Also returns the [`NotarizedPayment`]s that must be paid with the NFTs, for example in
    /// an offer, and the locked NFTs that will be created.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the spends can't be created.
    ///
    /// # Panics
    ///
    /// Panics if `nft_shares` or `inclusion_proofs` is shorter than `offered_nfts`.
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

        let mut notarized_payments = Vec::with_capacity(offered_nfts.len());
        let mut created_nfts = Vec::with_capacity(offered_nfts.len());
        let mut nft_infos = Vec::with_capacity(offered_nfts.len());
        let mut security_conditions = Conditions::new();
        let mut total_shares_until_now = 0;
        for i in 0..offered_nfts.len() {
            let np = NotarizedPayment {
                // NFTs may have different weights in curated NFT mode
                nonce: clvm_tuple!(
                    total_shares_until_now,
                    clvm_tuple!(ephemeral_counter.tree_hash(), my_id)
                )
                .tree_hash()
                .into(),
                payments: vec![Payment::new(
                    my_p2,
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
                my_p2,
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

    /// Stakes `offered_cat` in a [`RewardDistributorType::Cat`] distributor, in the pending
    /// spend of `distributor`. Each mojo is one share.
    ///
    /// The entry belongs to `entry_custody_puzzle_hash`, and `existing_slot` is its current entry
    /// slot, if it has one.
    ///
    /// Besides the action, this spends `existing_slot` if provided. Returns the conditions that
    /// secure the stake, which assert the distributor's announcements for each locked asset
    /// and the new entry slot and, if `existing_slot` is provided, send a message (mode 18)
    /// acknowledging the unclaimed rewards of the existing entry that are given up.
    /// Also returns the [`NotarizedPayment`] that must be paid with the CAT, for example in an
    /// offer, and the locked CAT that will be created.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the spends can't be created.
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

        let np = NotarizedPayment {
            nonce: clvm_tuple!(ephemeral_counter.tree_hash(), my_id)
                .tree_hash()
                .into(),
            payments: vec![Payment::new(
                my_p2,
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
            offered_cat.child(my_p2, offered_cat.amount()),
        ))
    }
}
