use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::cat::CatSolution;
use chia_puzzle_types::singleton::{LauncherSolution, SingletonArgs};
use chia_puzzle_types::{
    LineageProof, Proof,
    singleton::{SingletonSolution, SingletonStruct},
};
use chia_sdk_types::puzzles::{
    RawActionLayerSolution, ReserveFinalizerSolution, RewardDistributorCommitmentSlotValue,
    RewardDistributorEntrySlotValue, RewardDistributorRewardSlotValue, RewardDistributorSlotNonce,
    SlotInfo,
};
use chia_sdk_types::{Condition, Conditions};
use clvm_traits::{FromClvm, clvm_tuple, match_tuple};
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::NodePtr;

use crate::{
    ActionLayer, ActionLayerSolution, ActionSingleton, Cat, CatSpend, DriverError, Layer, Puzzle,
    RewardDistributorActionLog, RewardDistributorAddEntryAction,
    RewardDistributorAddIncentivesAction, RewardDistributorCommitIncentivesAction,
    RewardDistributorInitiatePayoutAction, RewardDistributorNewEpochAction,
    RewardDistributorRefreshAction, RewardDistributorRemoveEntryAction,
    RewardDistributorStakeAction, RewardDistributorStateTransition, RewardDistributorSyncAction,
    RewardDistributorType, RewardDistributorUnstakeAction,
    RewardDistributorWithdrawIncentivesAction, SingletonAction, SingletonLayer, Slot, Spend,
    SpendContext,
};

use super::{Reserve, RewardDistributorConstants, RewardDistributorInfo, RewardDistributorState};

#[derive(Debug, Clone)]
pub struct RewardDistributorPendingSpendInfo {
    pub actions: Vec<Spend>,

    pub spent_reward_slots: Vec<RewardDistributorRewardSlotValue>,
    pub spent_commitment_slots: Vec<RewardDistributorCommitmentSlotValue>,
    pub spent_entry_slots: Vec<RewardDistributorEntrySlotValue>,

    pub created_reward_slots: Vec<RewardDistributorRewardSlotValue>,
    pub created_commitment_slots: Vec<RewardDistributorCommitmentSlotValue>,
    pub created_entry_slots: Vec<RewardDistributorEntrySlotValue>,

    pub logs: Vec<RewardDistributorActionLog>,

    pub latest_state: (NodePtr, RewardDistributorState),

    pub signature: Signature,
    pub other_cats: Vec<CatSpend>,
}

impl RewardDistributorPendingSpendInfo {
    pub fn new(latest_state: RewardDistributorState) -> Self {
        Self {
            actions: vec![],
            created_reward_slots: vec![],
            created_commitment_slots: vec![],
            created_entry_slots: vec![],
            spent_reward_slots: vec![],
            spent_commitment_slots: vec![],
            spent_entry_slots: vec![],
            logs: vec![],
            latest_state: (NodePtr::NIL, latest_state),
            signature: Signature::default(),
            other_cats: vec![],
        }
    }

    pub fn add_delta(&mut self, delta: RewardDistributorPendingSpendInfo) {
        self.actions.extend(delta.actions);

        self.spent_reward_slots.extend(delta.spent_reward_slots);
        self.spent_commitment_slots
            .extend(delta.spent_commitment_slots);
        self.spent_entry_slots.extend(delta.spent_entry_slots);

        self.created_reward_slots.extend(delta.created_reward_slots);
        self.created_commitment_slots
            .extend(delta.created_commitment_slots);
        self.created_entry_slots.extend(delta.created_entry_slots);

        self.logs.extend(delta.logs);

        self.latest_state = delta.latest_state;

        // do not change pending signature
        // or other cats
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct RewardDistributor {
    pub coin: Coin,
    pub proof: Proof,
    pub info: RewardDistributorInfo,
    pub reserve: Reserve,

    pub pending_spend: RewardDistributorPendingSpendInfo,
}

impl RewardDistributor {
    pub fn new(coin: Coin, proof: Proof, info: RewardDistributorInfo, reserve: Reserve) -> Self {
        Self {
            coin,
            proof,
            info,
            reserve,
            pending_spend: RewardDistributorPendingSpendInfo::new(info.state),
        }
    }
}

impl RewardDistributor {
    #[allow(clippy::type_complexity)]
    pub fn pending_info_delta_from_spend(
        ctx: &mut SpendContext,
        action_spend: Spend,
        current_state_and_ephemeral: (NodePtr, RewardDistributorState),
        constants: RewardDistributorConstants,
    ) -> Result<RewardDistributorPendingSpendInfo, DriverError> {
        let mut spent_reward_slots = vec![];
        let mut spent_commitment_slots = vec![];
        let mut spent_entry_slots = vec![];

        let mut created_reward_slots = vec![];
        let mut created_commitment_slots = vec![];
        let mut created_entry_slots = vec![];

        let new_epoch_action = RewardDistributorNewEpochAction::from_constants(&constants);
        let new_epoch_hash = new_epoch_action.tree_hash();

        let commit_incentives_action =
            RewardDistributorCommitIncentivesAction::from_constants(&constants);
        let commit_incentives_hash = commit_incentives_action.tree_hash();

        let add_entry_action = RewardDistributorAddEntryAction::from_constants(&constants);
        let add_entry_hash = add_entry_action.tree_hash();

        let remove_entry_action = RewardDistributorRemoveEntryAction::from_constants(&constants);
        let remove_entry_hash = remove_entry_action.tree_hash();

        let stake_action = RewardDistributorStakeAction::from_constants(&constants);
        let stake_hash = stake_action.tree_hash();

        let unstake_action = RewardDistributorUnstakeAction::from_constants(&constants);
        let unstake_hash = unstake_action.tree_hash();

        let withdraw_incentives_action =
            RewardDistributorWithdrawIncentivesAction::from_constants(&constants);
        let withdraw_incentives_hash = withdraw_incentives_action.tree_hash();

        let initiate_payout_action =
            RewardDistributorInitiatePayoutAction::from_constants(&constants);
        let initiate_payout_hash = initiate_payout_action.tree_hash();

        let add_incentives_action =
            RewardDistributorAddIncentivesAction::from_constants(&constants);
        let add_incentives_hash = add_incentives_action.tree_hash();

        let sync_action = RewardDistributorSyncAction::from_constants(&constants);
        let sync_hash = sync_action.tree_hash();

        let refresh_action = RewardDistributorRefreshAction::from_constants(&constants);
        let refresh_hash = refresh_action.tree_hash();

        let actual_solution = ctx.alloc(&clvm_tuple!(
            current_state_and_ephemeral,
            action_spend.solution
        ))?;

        let output = ctx.run(action_spend.puzzle, actual_solution)?;
        let (new_state_and_ephemeral, _) =
            ctx.extract::<match_tuple!((NodePtr, RewardDistributorState), NodePtr)>(output)?;

        let changes = RewardDistributorStateTransition {
            old_state: current_state_and_ephemeral.1,
            new_state: new_state_and_ephemeral.1,
        };

        let raw_action_hash = ctx.tree_hash(action_spend.puzzle);

        let log = if raw_action_hash == new_epoch_hash {
            RewardDistributorActionLog::NewEpoch(RewardDistributorNewEpochAction::get_log(
                ctx,
                action_spend.solution,
                changes,
            )?)
        } else if raw_action_hash == commit_incentives_hash {
            RewardDistributorActionLog::CommitIncentives(
                RewardDistributorCommitIncentivesAction::get_log(
                    ctx,
                    action_spend.solution,
                    changes,
                    constants.epoch_seconds,
                )?,
            )
        } else if raw_action_hash == add_entry_hash {
            RewardDistributorActionLog::AddEntry(RewardDistributorAddEntryAction::get_log(
                ctx,
                action_spend.solution,
                changes,
            )?)
        } else if raw_action_hash == stake_hash {
            RewardDistributorActionLog::Stake(RewardDistributorStakeAction::get_log(
                ctx,
                action_spend.solution,
                changes,
                constants.reward_distributor_type,
            )?)
        } else if raw_action_hash == remove_entry_hash {
            RewardDistributorActionLog::RemoveEntry(RewardDistributorRemoveEntryAction::get_log(
                ctx,
                action_spend.solution,
                changes,
            )?)
        } else if raw_action_hash == unstake_hash {
            RewardDistributorActionLog::Unstake(RewardDistributorUnstakeAction::get_log(
                ctx,
                action_spend.solution,
                changes,
                constants.launcher_id,
                constants.reward_distributor_type,
                current_state_and_ephemeral.0,
            )?)
        } else if raw_action_hash == withdraw_incentives_hash {
            RewardDistributorActionLog::WithdrawIncentives(
                RewardDistributorWithdrawIncentivesAction::get_log(
                    ctx,
                    action_spend.solution,
                    changes,
                    constants.withdrawal_share_bps,
                )?,
            )
        } else if raw_action_hash == initiate_payout_hash {
            RewardDistributorActionLog::InitiatePayout(
                RewardDistributorInitiatePayoutAction::get_log(
                    ctx,
                    action_spend.solution,
                    changes,
                )?,
            )
        } else if raw_action_hash == refresh_hash {
            let RewardDistributorType::CuratedNft {
                store_launcher_id, ..
            } = constants.reward_distributor_type
            else {
                return Err(DriverError::InvalidMerkleProof);
            };

            RewardDistributorActionLog::RefreshNftsFromDl(RewardDistributorRefreshAction::get_log(
                ctx,
                action_spend.solution,
                changes,
                store_launcher_id,
            )?)
        } else if raw_action_hash == add_incentives_hash {
            RewardDistributorActionLog::AddIncentives(
                RewardDistributorAddIncentivesAction::get_log(ctx, action_spend.solution, changes)?,
            )
        } else if raw_action_hash == sync_hash {
            RewardDistributorActionLog::Sync(RewardDistributorSyncAction::get_log(
                ctx,
                action_spend.solution,
                changes,
            )?)
        } else {
            return Err(DriverError::InvalidMerkleProof);
        };

        log.extend_spent_slots(
            &mut spent_reward_slots,
            &mut spent_commitment_slots,
            &mut spent_entry_slots,
        );
        log.extend_created_slots(
            &mut created_reward_slots,
            &mut created_commitment_slots,
            &mut created_entry_slots,
        );

        Ok(RewardDistributorPendingSpendInfo {
            actions: vec![action_spend],
            spent_reward_slots,
            spent_commitment_slots,
            spent_entry_slots,
            created_reward_slots,
            created_commitment_slots,
            created_entry_slots,
            logs: vec![log],
            latest_state: new_state_and_ephemeral,
            signature: Signature::default(),
            other_cats: vec![],
        })
    }

    pub fn pending_info_from_spend(
        ctx: &mut SpendContext,
        inner_solution: NodePtr,
        initial_state: RewardDistributorState,
        constants: RewardDistributorConstants,
        signature: Signature,
    ) -> Result<RewardDistributorPendingSpendInfo, DriverError> {
        let mut pending_spend_info = RewardDistributorPendingSpendInfo::new(initial_state);

        let inner_solution =
            ActionLayer::<RewardDistributorState, NodePtr>::parse_solution(ctx, inner_solution)?;

        for raw_action in &inner_solution.action_spends {
            let delta = Self::pending_info_delta_from_spend(
                ctx,
                *raw_action,
                pending_spend_info.latest_state,
                constants,
            )?;

            pending_spend_info.add_delta(delta);
        }

        pending_spend_info.signature = signature;
        Ok(pending_spend_info)
    }

    pub fn from_spend(
        ctx: &mut SpendContext,
        spend: &CoinSpend,
        reserve_lineage_proof: Option<LineageProof>,
        constants: RewardDistributorConstants,
        signature: Signature,
    ) -> Result<Option<Self>, DriverError> {
        let coin = spend.coin;
        if coin.amount != 1 {
            return Ok(None);
        }

        let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(ctx, puzzle_ptr);
        let solution_ptr = ctx.alloc(&spend.solution)?;

        let Some(info) = RewardDistributorInfo::parse(ctx, puzzle, constants)? else {
            return Ok(None);
        };

        let solution = ctx.extract::<SingletonSolution<NodePtr>>(solution_ptr)?;
        let proof = solution.lineage_proof;

        let pending_spend = Self::pending_info_from_spend(
            ctx,
            solution.inner_solution,
            info.state,
            constants,
            signature,
        )?;

        let inner_solution =
            RawActionLayerSolution::<NodePtr, NodePtr, ReserveFinalizerSolution>::from_clvm(
                ctx,
                solution.inner_solution,
            )?;

        let reserve = Reserve::new(
            inner_solution.finalizer_solution.reserve_parent_id,
            reserve_lineage_proof.unwrap_or(LineageProof {
                parent_parent_coin_info: Bytes32::default(),
                parent_inner_puzzle_hash: Bytes32::default(),
                parent_amount: 0,
            }), // dummy default value
            constants.reserve_asset_id,
            SingletonStruct::new(info.constants.launcher_id)
                .tree_hash()
                .into(),
            0,
            info.state.total_reserves,
        );

        Ok(Some(RewardDistributor {
            coin,
            proof,
            info,
            reserve,
            pending_spend,
        }))
    }

    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    pub fn from_parent_spend(
        ctx: &mut SpendContext,
        parent_spend: &CoinSpend,
        constants: RewardDistributorConstants,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_registry) =
            Self::from_spend(ctx, parent_spend, None, constants, Signature::default())?
        else {
            return Ok(None);
        };

        let new_info = parent_registry
            .info
            .with_state(parent_registry.pending_spend.latest_state.1);

        Ok(Some(RewardDistributor {
            coin: Coin::new(
                parent_registry.coin.coin_id(),
                new_info.puzzle_hash().into(),
                1,
            ),
            proof: Proof::Lineage(parent_registry.child_lineage_proof()),
            info: new_info,
            reserve: parent_registry.reserve.child(new_info.state.total_reserves),
            pending_spend: RewardDistributorPendingSpendInfo::new(new_info.state),
        }))
    }

    pub fn child(&self, child_state: RewardDistributorState) -> Self {
        let new_info = self.info.with_state(child_state);
        let new_coin = Coin::new(self.coin.coin_id(), new_info.puzzle_hash().into(), 1);
        let new_reserve = self.reserve.child(child_state.total_reserves);

        RewardDistributor {
            coin: new_coin,
            proof: Proof::Lineage(self.child_lineage_proof()),
            info: new_info,
            reserve: new_reserve,
            pending_spend: RewardDistributorPendingSpendInfo::new(new_info.state),
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn from_launcher_solution(
        ctx: &mut SpendContext,
        launcher_coin: Coin,
        launcher_solution: NodePtr,
    ) -> Result<Option<(RewardDistributorConstants, RewardDistributorState, Coin)>, DriverError>
    where
        Self: Sized,
    {
        let Ok(launcher_solution) =
            ctx.extract::<LauncherSolution<(u64, RewardDistributorConstants)>>(launcher_solution)
        else {
            return Ok(None);
        };

        let launcher_id = launcher_coin.coin_id();
        let (first_epoch_start, constants) = launcher_solution.key_value_list;

        if constants != constants.with_launcher_id(launcher_id) {
            return Err(DriverError::Custom(
                "Distributor constants invalid".to_string(),
            ));
        }

        let distributor_eve_coin =
            Coin::new(launcher_id, launcher_solution.singleton_puzzle_hash, 1);

        let initial_state = RewardDistributorState::initial(first_epoch_start);

        Ok(Some((constants, initial_state, distributor_eve_coin)))
    }

    #[allow(clippy::type_complexity)]
    pub fn from_eve_coin_spend(
        ctx: &mut SpendContext,
        constants: RewardDistributorConstants,
        initial_state: RewardDistributorState,
        eve_coin_spend: &CoinSpend,
        reserve_parent_id: Bytes32,
        reserve_lineage_proof: LineageProof,
    ) -> Result<Option<(RewardDistributor, Slot<RewardDistributorRewardSlotValue>)>, DriverError>
    where
        Self: Sized,
    {
        let eve_coin_puzzle_ptr = ctx.alloc(&eve_coin_spend.puzzle_reveal)?;
        let eve_coin_puzzle = Puzzle::parse(ctx, eve_coin_puzzle_ptr);
        let Some(eve_coin_puzzle) = SingletonLayer::<NodePtr>::parse_puzzle(ctx, eve_coin_puzzle)?
        else {
            return Err(DriverError::Custom("Eve coin not a singleton".to_string()));
        };

        let eve_coin_inner_puzzle_hash = tree_hash(ctx, eve_coin_puzzle.inner_puzzle);

        let eve_coin_solution_ptr = ctx.alloc(&eve_coin_spend.solution)?;
        let eve_coin_output = ctx.run(eve_coin_puzzle_ptr, eve_coin_solution_ptr)?;
        let eve_coin_output = ctx.extract::<Conditions<NodePtr>>(eve_coin_output)?;

        let Some(Condition::CreateCoin(odd_create_coin)) = eve_coin_output.into_iter().find(|c| {
            if let Condition::CreateCoin(create_coin) = c {
                // singletons with amount != 1 are weird and I don't support them
                create_coin.amount % 2 == 1
            } else {
                false
            }
        }) else {
            return Err(DriverError::Custom(
                "Eve coin did not create a coin".to_string(),
            ));
        };

        let new_coin = Coin::new(
            eve_coin_spend.coin.coin_id(),
            odd_create_coin.puzzle_hash,
            odd_create_coin.amount,
        );
        let lineage_proof = LineageProof {
            parent_parent_coin_info: eve_coin_spend.coin.parent_coin_info,
            parent_inner_puzzle_hash: eve_coin_inner_puzzle_hash.into(),
            parent_amount: eve_coin_spend.coin.amount,
        };
        let reserve = Reserve::new(
            reserve_parent_id,
            reserve_lineage_proof,
            constants.reserve_asset_id,
            SingletonStruct::new(constants.launcher_id)
                .tree_hash()
                .into(),
            0,
            0,
        );
        let new_distributor = RewardDistributor::new(
            new_coin,
            Proof::Lineage(lineage_proof),
            RewardDistributorInfo::new(initial_state, constants),
            reserve,
        );

        if SingletonArgs::curry_tree_hash(
            constants.launcher_id,
            new_distributor.info.inner_puzzle_hash(),
        ) != new_distributor.coin.puzzle_hash.into()
        {
            return Err(DriverError::Custom(
                "Distributor singleton puzzle hash mismatch".to_string(),
            ));
        }

        let slot_value = RewardDistributorRewardSlotValue {
            counter: 0,
            epoch_start: initial_state.round_time_info.epoch_end,
            next_epoch_initialized: false,
            rewards: 0,
        };

        let slot = Slot::new(
            lineage_proof,
            SlotInfo::from_value(
                constants.launcher_id,
                RewardDistributorSlotNonce::REWARD.to_u64(),
                slot_value,
            ),
        );

        Ok(Some((new_distributor, slot)))
    }

    pub fn set_pending_signature(&mut self, signature: Signature) {
        self.pending_spend.signature = signature;
    }

    pub fn set_pending_other_cats(&mut self, other_cats: Vec<CatSpend>) {
        self.pending_spend.other_cats = other_cats;
    }

    pub fn from_mempool_item(
        ctx: &mut SpendContext,
        mempool_item: SpendBundle,
        constants: RewardDistributorConstants,
    ) -> Result<Option<Self>, DriverError> {
        let mut registry = None;

        let mut other_cats = vec![];

        for spend in &mempool_item.coin_spends {
            if registry.is_none()
                && let Some(parsed_registry) =
                    Self::from_spend(ctx, spend, None, constants, Signature::default())?
            {
                registry = Some(parsed_registry);
            } else {
                // CAT spends are added to other_cats so the ring is built when the registry
                // is spent (so it includes the reserve)
                let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal)?;
                let puzzle = Puzzle::parse(ctx, puzzle_ptr);
                let solution_ptr = ctx.alloc(&spend.solution)?;

                if let Ok(Some(parsed_cat)) = Cat::parse(ctx, spend.coin, puzzle, solution_ptr)
                    && parsed_cat.cat.info.asset_id == constants.reserve_asset_id
                {
                    other_cats.push(CatSpend::new(
                        parsed_cat.cat,
                        Spend::new(parsed_cat.p2_puzzle.ptr(), parsed_cat.p2_solution),
                    ));
                }
            }
        }

        let Some(registry) = registry else {
            return Ok(None);
        };

        // find & set actual reserve
        // note that we initialized the registy with reserve_lineage_proof=None, so
        // only the reserve parent id and amount are correct (but NOT puzzle hash)
        // the puzzle hash can be obtained from the registry constants
        let reserve_coin = Coin::new(
            registry.reserve.coin.parent_coin_info,
            registry.info.constants.reserve_full_puzzle_hash,
            registry.reserve.coin.amount,
        );
        let Some(reserve_spend) = mempool_item
            .coin_spends
            .iter()
            .find(|c| c.coin == reserve_coin)
        else {
            return Err(DriverError::Custom(
                "Reserve spend not found in mempool item".to_string(),
            ));
        };

        let reserve_sol_ptr = ctx.alloc(&reserve_spend.solution)?;
        let reserve_solution = ctx.extract::<CatSolution<NodePtr>>(reserve_sol_ptr)?;

        // Could theoretically be a bit more optimized, but this ensures
        //  consistent behavior even if there are future changes in the
        //  way attributes are calculated for the reward distributor
        let Some(mut registry) = RewardDistributor::from_spend(
            ctx,
            mempool_item
                .coin_spends
                .iter()
                .find(|c| c.coin == registry.coin)
                .ok_or(DriverError::Custom(
                    "Couldn't find distributor spend in mempool item".to_string(),
                ))?,
            reserve_solution.lineage_proof,
            constants,
            Signature::default(),
        )?
        else {
            return Err(DriverError::Custom(
                "Couldn't rebuild distributor from spend a second time - something's pretty off"
                    .to_string(),
            ));
        };

        while let Some(registry_spend) = mempool_item
            .coin_spends
            .iter()
            .find(|c| c.coin.amount != 0 && c.coin.parent_coin_info == registry.coin.coin_id())
        {
            let Some(new_registry) = Self::from_spend(
                ctx,
                registry_spend,
                Some(registry.reserve.child_lineage_proof()),
                registry.info.constants,
                Signature::default(),
            )?
            else {
                break;
            };

            registry = new_registry;
        }

        // insert all other spends into the context
        for spend in mempool_item.coin_spends {
            if spend.coin == registry.coin || other_cats.iter().any(|c| c.cat.coin == spend.coin) {
                continue;
            }

            ctx.insert(spend);
        }

        // filter out 'old' reserve spend from other_cats
        // finish_spend will add the latest reserve spend
        other_cats.retain(|c| c.cat.coin != registry.reserve.coin);

        registry.set_pending_other_cats(other_cats);
        registry.set_pending_signature(mempool_item.aggregated_signature);
        Ok(Some(registry))
    }
}

impl ActionSingleton for RewardDistributor {
    type State = RewardDistributorState;
    type Constants = RewardDistributorConstants;
}

impl RewardDistributor {
    pub fn finish_spend(
        self,
        ctx: &mut SpendContext,
        other_cat_spends: Vec<CatSpend>,
    ) -> Result<(Self, Signature), DriverError> {
        let layers = self.info.into_layers(ctx)?;

        let puzzle = layers.construct_puzzle(ctx)?;

        let action_puzzle_hashes = self
            .pending_spend
            .actions
            .iter()
            .map(|a| ctx.tree_hash(a.puzzle).into())
            .collect::<Vec<Bytes32>>();

        let finalizer_solution = ctx.alloc(&ReserveFinalizerSolution {
            reserve_parent_id: self.reserve.coin.parent_coin_info,
        })?;

        let child = self.child(self.pending_spend.latest_state.1);
        let solution = layers.construct_solution(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: ActionLayerSolution {
                    proofs: layers
                        .inner_puzzle
                        .get_proofs(
                            &RewardDistributorInfo::action_puzzle_hashes(&self.info.constants),
                            &action_puzzle_hashes,
                        )
                        .ok_or(DriverError::Custom(
                            "Couldn't build proofs for one or more actions".to_string(),
                        ))?,
                    action_spends: self.pending_spend.actions,
                    finalizer_solution,
                },
            },
        )?;

        let my_spend = Spend::new(puzzle, solution);
        ctx.spend(self.coin, my_spend)?;

        let cat_spend = self.reserve.cat_spend_for_reserve_finalizer_controller(
            ctx,
            self.info.state,
            self.info.inner_puzzle_hash().into(),
            solution,
        )?;

        let mut cat_spends = other_cat_spends;
        cat_spends.push(cat_spend);
        cat_spends.extend(self.pending_spend.other_cats);
        Cat::spend_all(ctx, &cat_spends)?;

        Ok((child, self.pending_spend.signature))
    }

    pub fn new_action<A>(&self) -> A
    where
        A: SingletonAction<Self>,
    {
        A::from_constants(&self.info.constants)
    }

    pub fn created_slot_value_to_slot<SlotValue>(
        &self,
        slot_value: SlotValue,
        nonce: RewardDistributorSlotNonce,
    ) -> Slot<SlotValue>
    where
        SlotValue: Copy + ToTreeHash,
    {
        Slot::new(
            LineageProof {
                parent_parent_coin_info: self.coin.parent_coin_info,
                parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
                parent_amount: self.coin.amount,
            },
            SlotInfo::from_value(self.info.constants.launcher_id, nonce.to_u64(), slot_value),
        )
    }

    pub fn insert_action_spend(
        &mut self,
        ctx: &mut SpendContext,
        action_spend: Spend,
    ) -> Result<(), DriverError> {
        let delta = Self::pending_info_delta_from_spend(
            ctx,
            action_spend,
            self.pending_spend.latest_state,
            self.info.constants,
        )?;

        self.pending_spend.add_delta(delta);

        Ok(())
    }

    pub fn actual_reward_slot_value(
        &self,
        slot: Slot<RewardDistributorRewardSlotValue>,
    ) -> Slot<RewardDistributorRewardSlotValue> {
        let mut slot = slot;

        for slot_value in &self.pending_spend.created_reward_slots {
            if slot_value.epoch_start == slot.info.value.epoch_start {
                slot = self
                    .created_slot_value_to_slot(*slot_value, RewardDistributorSlotNonce::REWARD);
            }
        }

        slot
    }

    pub fn actual_entry_slot_value(
        &self,
        slot: Slot<RewardDistributorEntrySlotValue>,
    ) -> Slot<RewardDistributorEntrySlotValue> {
        let mut slot = slot;

        for slot_value in &self.pending_spend.created_entry_slots {
            if slot_value.payout_puzzle_hash == slot.info.value.payout_puzzle_hash {
                slot =
                    self.created_slot_value_to_slot(*slot_value, RewardDistributorSlotNonce::ENTRY);
            }
        }

        slot
    }

    pub fn actual_commitment_slot_value(
        &self,
        slot: Slot<RewardDistributorCommitmentSlotValue>,
    ) -> Slot<RewardDistributorCommitmentSlotValue> {
        let mut slot = slot;

        for slot_value in &self.pending_spend.created_commitment_slots {
            if slot_value.epoch_start == slot.info.value.epoch_start {
                slot = self.created_slot_value_to_slot(
                    *slot_value,
                    RewardDistributorSlotNonce::COMMITMENT,
                );
            }
        }

        slot
    }
}
