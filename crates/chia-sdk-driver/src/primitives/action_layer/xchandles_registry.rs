use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::singleton::{LauncherSolution, SingletonArgs, SingletonSolution};
use chia_puzzle_types::{LineageProof, Proof};
use chia_sdk_types::puzzles::{
    DelegatedStateActionSolution, SlotInfo, XchandlesHandleSlotValue, XchandlesSlotNonce,
    XchandlesUpdateSlotValue,
};
use clvm_traits::{clvm_tuple, match_tuple};
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

use crate::{
    ActionLayer, ActionLayerSolution, ActionSingleton, DelegatedStateAction, DriverError, Layer,
    Puzzle, SingletonAction, Spend, SpendContext, XchandlesActionLog,
    XchandlesDelegatedStateActionLog, XchandlesExecuteUpdateAction, XchandlesExpireAction,
    XchandlesExtendAction, XchandlesInitiateUpdateAction, XchandlesOracleAction,
    XchandlesRefundAction, XchandlesRegisterAction, eve_singleton_inner_puzzle,
};

use super::{Slot, XchandlesConstants, XchandlesRegistryInfo, XchandlesRegistryState};

/// The actions queued for the next spend of an [`XchandlesRegistry`], and their effects.
#[derive(Debug, Clone)]
pub struct XchandlesPendingSpendInfo {
    /// The action spends, in the order they will run.
    pub actions: Vec<Spend>,
    /// The values of the handle slots the actions spend.
    pub spent_handle_slots: Vec<XchandlesHandleSlotValue>,
    /// The values of the update slots the actions spend.
    pub spent_update_slots: Vec<XchandlesUpdateSlotValue>,
    /// The values of the handle slots the actions create.
    pub created_handle_slots: Vec<XchandlesHandleSlotValue>,
    /// The values of the update slots the actions create.
    pub created_update_slots: Vec<XchandlesUpdateSlotValue>,

    /// A log entry for each action.
    pub logs: Vec<XchandlesActionLog>,

    /// The ephemeral state and registry state after the queued actions have run.
    pub latest_state: (NodePtr, XchandlesRegistryState),

    /// The aggregated signature that the spend needs, such as one from a parsed mempool item.
    pub signature: Signature,
}

impl XchandlesPendingSpendInfo {
    /// Creates an empty pending spend starting from the given state.
    pub fn new(latest_state: XchandlesRegistryState) -> Self {
        Self {
            actions: vec![],
            created_handle_slots: vec![],
            created_update_slots: vec![],
            spent_handle_slots: vec![],
            spent_update_slots: vec![],
            logs: vec![],
            latest_state: (NodePtr::NIL, latest_state),
            signature: Signature::default(),
        }
    }
}

/// The `XCHandles` registry: an action-layer singleton that registers handles.
///
/// Each registered handle is stored in a handle [`Slot`], and the slots form a sorted linked list
/// keyed by handle hash, starting with the two end slots created at launch (see
/// [`launch_xchandles_registry`](crate::launch_xchandles_registry)). A handle slot records the
/// handle's expiration and the launcher ids of its owner and resolved NFTs. Handles are registered
/// with a [`PrecommitCoin`](crate::PrecommitCoin), can be extended, expire, and can be moved to new
/// owner and resolved NFTs with a two-step update that uses update slots.
///
/// To spend the registry, build actions with [`XchandlesRegistry::new_action`] and spend them. Most
/// actions' `spend` methods call [`XchandlesRegistry::insert_action_spend`] and return conditions or
/// messages that other coins in the bundle should output to approve or secure the action. The
/// [`DelegatedStateAction`] returns its [`Spend`] instead, which you insert yourself. Finally, call
/// [`XchandlesRegistry::finish_spend`]. While actions are pending, use
/// [`XchandlesRegistry::actual_neigbors`], [`XchandlesRegistry::actual_handle_slot`], and
/// [`XchandlesRegistry::actual_update_slot`] to account for slots created earlier in the same spend.
#[derive(Debug, Clone)]
#[must_use]
pub struct XchandlesRegistry {
    /// The registry singleton coin.
    pub coin: Coin,
    /// The proof needed to spend the singleton.
    pub proof: Proof,
    /// The state and constants of the registry.
    pub info: XchandlesRegistryInfo,

    /// The actions queued for the next spend.
    pub pending_spend: XchandlesPendingSpendInfo,
}

impl XchandlesRegistry {
    /// Creates a new [`XchandlesRegistry`] with no pending actions.
    pub fn new(coin: Coin, proof: Proof, info: XchandlesRegistryInfo) -> Self {
        Self {
            coin,
            proof,
            info,
            pending_spend: XchandlesPendingSpendInfo::new(info.state),
        }
    }
}

impl ActionSingleton for XchandlesRegistry {
    type State = XchandlesRegistryState;
    type Constants = XchandlesConstants;
}

impl XchandlesRegistry {
    /// Runs one action spend on top of the given state and identifies it.
    ///
    /// Returns the new ephemeral and registry state, the action's log, the values of the handle and
    /// update slots it creates, and the values of the handle and update slots it spends.
    ///
    /// # Errors
    ///
    /// Returns [`DriverError::InvalidMerkleProof`] if the action isn't one of the registry's actions.
    #[allow(clippy::type_complexity)]
    pub fn pending_info_delta_from_spend(
        ctx: &mut SpendContext,
        action_spend: Spend,
        current_state_and_ephemeral: (NodePtr, XchandlesRegistryState),
        constants: XchandlesConstants,
    ) -> Result<
        (
            (NodePtr, XchandlesRegistryState), // pending state
            XchandlesActionLog,                // action log
            Vec<XchandlesHandleSlotValue>,     // created handle slot values
            Vec<XchandlesUpdateSlotValue>,     // created update slot values
            Vec<XchandlesHandleSlotValue>,     // spent handle slot values
            Vec<XchandlesUpdateSlotValue>,     // spent update slot values
        ),
        DriverError,
    > {
        let mut created_handle_slots = vec![];
        let mut created_update_slots = vec![];
        let mut spent_handle_slots = vec![];
        let mut spent_update_slots = vec![];

        let state = current_state_and_ephemeral.1;

        let expire_action = XchandlesExpireAction::from_constants(&constants);
        let expire_action_hash = expire_action.tree_hash();

        let extend_action = XchandlesExtendAction::from_constants(&constants);
        let extend_action_hash = extend_action.tree_hash();

        let oracle_action = XchandlesOracleAction::from_constants(&constants);
        let oracle_action_hash = oracle_action.tree_hash();

        let register_action = XchandlesRegisterAction::from_constants(&constants);
        let register_action_hash = register_action.tree_hash();

        let initiate_update_action = XchandlesInitiateUpdateAction::from_constants(&constants);
        let initiate_update_action_hash = initiate_update_action.tree_hash();

        let execute_update_action = XchandlesExecuteUpdateAction::from_constants(&constants);
        let execute_update_action_hash = execute_update_action.tree_hash();

        let refund_action = XchandlesRefundAction::from_constants(&constants);
        let refund_action_hash = refund_action.tree_hash();

        let delegated_state_action =
            <DelegatedStateAction as SingletonAction<XchandlesRegistry>>::from_constants(
                &constants,
            );
        let delegated_state_action_hash = delegated_state_action.tree_hash();

        let actual_solution = ctx.alloc(&clvm_tuple!(
            current_state_and_ephemeral,
            action_spend.solution
        ))?;

        let output = ctx.run(action_spend.puzzle, actual_solution)?;
        let (new_state_and_ephemeral, _) =
            ctx.extract::<match_tuple!((NodePtr, XchandlesRegistryState), NodePtr)>(output)?;

        let raw_action_hash = ctx.tree_hash(action_spend.puzzle);

        let log = if raw_action_hash == extend_action_hash {
            let action_log = XchandlesActionLog::Extend(XchandlesExtendAction::get_log(
                ctx,
                action_spend.solution,
            )?);
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);

            action_log
        } else if raw_action_hash == oracle_action_hash {
            let action_log = XchandlesActionLog::Oracle(XchandlesOracleAction::get_log(
                ctx,
                action_spend.solution,
            )?);
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);

            action_log
        } else if raw_action_hash == initiate_update_action_hash {
            let action_log = XchandlesActionLog::InitiateUpdate(
                XchandlesInitiateUpdateAction::get_log(ctx, action_spend.solution, &constants)?,
            );
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);
            action_log.extend_created_update_slots(&mut created_update_slots);

            action_log
        } else if raw_action_hash == execute_update_action_hash {
            let action_log = XchandlesActionLog::ExecuteUpdate(
                XchandlesExecuteUpdateAction::get_log(ctx, action_spend.solution)?,
            );
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_spent_update_slots(&mut spent_update_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);

            action_log
        } else if raw_action_hash == refund_action_hash {
            let action_log = XchandlesActionLog::Refund(XchandlesRefundAction::get_log(
                ctx,
                action_spend.solution,
            )?);
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);

            action_log
        } else if raw_action_hash == expire_action_hash {
            let action_log = XchandlesActionLog::Expire(XchandlesExpireAction::get_log(
                ctx,
                action_spend.solution,
                state,
            )?);
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);

            action_log
        } else if raw_action_hash == register_action_hash {
            let action_log = XchandlesActionLog::Register(XchandlesRegisterAction::get_log(
                ctx,
                action_spend.solution,
            )?);
            action_log.extend_spent_handle_slots(&mut spent_handle_slots);
            action_log.extend_created_handle_slots(&mut created_handle_slots);

            action_log
        } else if raw_action_hash == delegated_state_action_hash {
            // delegated state action has no effect on slots
            let sol = ctx.extract::<DelegatedStateActionSolution<XchandlesRegistryState>>(
                action_spend.solution,
            )?;

            XchandlesActionLog::DelegatedState(XchandlesDelegatedStateActionLog {
                old_state: current_state_and_ephemeral.1,
                new_state: sol.new_state,
            })
        } else {
            return Err(DriverError::InvalidMerkleProof);
        };

        Ok((
            new_state_and_ephemeral,
            log,
            created_handle_slots,
            created_update_slots,
            spent_handle_slots,
            spent_update_slots,
        ))
    }

    /// Parses all of the actions in an action-layer inner solution into an [`XchandlesPendingSpendInfo`].
    pub fn pending_info_from_spend(
        ctx: &mut SpendContext,
        inner_solution: NodePtr,
        initial_state: XchandlesRegistryState,
        constants: XchandlesConstants,
        signature: Signature,
    ) -> Result<XchandlesPendingSpendInfo, DriverError> {
        let mut created_handle_slots = vec![];
        let mut created_update_slots = vec![];
        let mut spent_handle_slots = vec![];
        let mut spent_update_slots = vec![];
        let mut logs = vec![];

        let mut state_incl_ephemeral: (NodePtr, XchandlesRegistryState) =
            (NodePtr::NIL, initial_state);

        let inner_solution =
            ActionLayer::<XchandlesRegistryState, NodePtr>::parse_solution(ctx, inner_solution)?;

        for raw_action in &inner_solution.action_spends {
            let res = Self::pending_info_delta_from_spend(
                ctx,
                *raw_action,
                state_incl_ephemeral,
                constants,
            )?;

            state_incl_ephemeral = res.0;
            logs.push(res.1);
            created_handle_slots.extend(res.2);
            created_update_slots.extend(res.3);
            spent_handle_slots.extend(res.4);
            spent_update_slots.extend(res.5);
        }

        Ok(XchandlesPendingSpendInfo {
            actions: inner_solution.action_spends,
            created_handle_slots,
            created_update_slots,
            spent_handle_slots,
            spent_update_slots,
            logs,
            latest_state: state_incl_ephemeral,
            signature,
        })
    }

    /// Parses a registry and its actions from a spend of the registry coin.
    ///
    /// The returned registry is the spent coin, with its actions in `pending_spend`. Returns
    /// [`None`] if the coin doesn't have amount 1 or isn't a registry with the given constants.
    pub fn from_spend(
        ctx: &mut SpendContext,
        spend: &CoinSpend,
        constants: XchandlesConstants,
        signature: Signature,
    ) -> Result<Option<Self>, DriverError> {
        let coin = spend.coin;
        if coin.amount != 1 {
            return Ok(None);
        }

        let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(ctx, puzzle_ptr);
        let solution_ptr = ctx.alloc(&spend.solution)?;

        let Some(info) = XchandlesRegistryInfo::parse(ctx, puzzle, constants)? else {
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

        Ok(Some(XchandlesRegistry {
            coin,
            proof,
            info,
            pending_spend,
        }))
    }

    /// Sets the signature that will be returned by [`XchandlesRegistry::finish_spend`].
    pub fn set_pending_signature(&mut self, signature: Signature) {
        self.pending_spend.signature = signature;
    }

    /// Returns the lineage proof for the child of this registry coin.
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Parses the registry coin created by a spend of its parent registry coin.
    ///
    /// Returns [`None`] if the parent isn't a registry with the given constants.
    pub fn from_parent_spend(
        ctx: &mut SpendContext,
        parent_spend: &CoinSpend,
        constants: XchandlesConstants,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_registry) =
            Self::from_spend(ctx, parent_spend, constants, Signature::default())?
        else {
            return Ok(None);
        };

        let proof = Proof::Lineage(parent_registry.child_lineage_proof());

        let new_info = parent_registry
            .info
            .with_state(parent_registry.pending_spend.latest_state.1);
        let new_coin = Coin::new(
            parent_registry.coin.coin_id(),
            new_info.puzzle_hash().into(),
            1,
        );

        Ok(Some(XchandlesRegistry {
            coin: new_coin,
            proof,
            info: new_info,
            pending_spend: XchandlesPendingSpendInfo::new(new_info.state),
        }))
    }

    /// Returns the registry coin created by spending this one, with the given state.
    pub fn child(&self, child_state: XchandlesRegistryState) -> Self {
        let new_info = self.info.with_state(child_state);
        let new_coin = Coin::new(self.coin.coin_id(), new_info.puzzle_hash().into(), 1);

        XchandlesRegistry {
            coin: new_coin,
            proof: Proof::Lineage(self.child_lineage_proof()),
            info: new_info,
            pending_spend: XchandlesPendingSpendInfo::new(new_info.state),
        }
    }

    /// Parses a registry launched by [`launch_xchandles_registry`](crate::launch_xchandles_registry) from its launcher solution.
    ///
    /// Returns the first registry coin (created by the eve singleton), its two initial slots, the
    /// initial registration asset id, and the initial base price. Returns [`None`] if the launcher
    /// solution doesn't have the expected key-value list, the state doesn't match the launch
    /// parameters, or the launched puzzle hash doesn't match.
    // Also returns initial registration asset id
    #[allow(clippy::type_complexity)]
    pub fn from_launcher_solution(
        ctx: &mut SpendContext,
        launcher_coin: Coin,
        launcher_solution: NodePtr,
    ) -> Result<Option<(Self, [Slot<XchandlesHandleSlotValue>; 2], Bytes32, u64)>, DriverError>
    where
        Self: Sized,
    {
        let Ok(launcher_solution) = ctx.extract::<LauncherSolution<(
            Bytes32,
            (
                u64,
                (u64, (XchandlesRegistryState, (XchandlesConstants, ()))),
            ),
        )>>(launcher_solution) else {
            return Ok(None);
        };

        let launcher_id = launcher_coin.coin_id();
        let (
            initial_registration_asset_id,
            (initial_base_price, (initial_registration_period, (initial_state, (constants, ())))),
        ) = launcher_solution.key_value_list;

        let info = XchandlesRegistryInfo::new(
            initial_state,
            constants.with_launcher_id(launcher_coin.coin_id()),
        );
        if info.state
            != XchandlesRegistryState::from(
                initial_registration_asset_id.tree_hash().into(),
                initial_base_price,
                initial_registration_period,
            )
        {
            return Ok(None);
        }

        let registry_inner_puzzle_hash = info.inner_puzzle_hash().into();
        let eve_singleton_inner_puzzle = eve_singleton_inner_puzzle(
            ctx,
            launcher_id,
            XchandlesSlotNonce::HANDLE.to_u64(),
            XchandlesHandleSlotValue::initial_left_end(),
            XchandlesHandleSlotValue::initial_right_end(),
            NodePtr::NIL,
            registry_inner_puzzle_hash,
        )?;
        let eve_singleton_inner_puzzle_hash = ctx.tree_hash(eve_singleton_inner_puzzle);

        let eve_coin = Coin::new(
            launcher_id,
            SingletonArgs::curry_tree_hash(launcher_id, eve_singleton_inner_puzzle_hash).into(),
            1,
        );
        let registry_coin = Coin::new(
            eve_coin.coin_id(),
            SingletonArgs::curry_tree_hash(launcher_id, registry_inner_puzzle_hash.into()).into(),
            1,
        );

        if eve_coin.puzzle_hash != launcher_solution.singleton_puzzle_hash {
            return Ok(None);
        }

        // proof for registry, which is created by eve singleton
        let proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: eve_coin.parent_coin_info,
            parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
            parent_amount: eve_coin.amount,
        });

        let slot_proof = LineageProof {
            parent_parent_coin_info: eve_coin.parent_coin_info,
            parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
            parent_amount: 1,
        };
        let slots = [
            Slot::new(
                slot_proof,
                SlotInfo::from_value(
                    launcher_id,
                    XchandlesSlotNonce::HANDLE.to_u64(),
                    XchandlesHandleSlotValue::initial_left_end(),
                ),
            ),
            Slot::new(
                slot_proof,
                SlotInfo::from_value(
                    launcher_id,
                    XchandlesSlotNonce::HANDLE.to_u64(),
                    XchandlesHandleSlotValue::initial_right_end(),
                ),
            ),
        ];

        Ok(Some((
            XchandlesRegistry {
                coin: registry_coin,
                proof,
                info,
                pending_spend: XchandlesPendingSpendInfo::new(info.state),
            },
            slots,
            initial_registration_asset_id,
            initial_base_price,
        )))
    }

    /// Parses the latest registry from a mempool item and prepares to spend it again.
    ///
    /// Finds the first registry spend in the bundle, follows its children through the bundle to the
    /// latest registry coin, and parses that coin's actions into `pending_spend`. All other spends
    /// are inserted into `ctx`, and the bundle's aggregated signature becomes the pending signature.
    /// This lets you add more actions to an unconfirmed registry spend and replace it with
    /// [`XchandlesRegistry::finish_spend`].
    pub fn from_mempool_item(
        ctx: &mut SpendContext,
        mempool_item: SpendBundle,
        constants: XchandlesConstants,
    ) -> Result<Option<Self>, DriverError> {
        let mut registry = None;

        for spend in &mempool_item.coin_spends {
            if let Some(parsed_registry) =
                Self::from_spend(ctx, spend, constants, Signature::default())?
            {
                registry = Some(parsed_registry);
                break;
            }
        }

        let Some(mut registry) = registry else {
            return Ok(None);
        };

        // find next child coin, if it exists
        // amount != 0 makes sure we don't try to parse a slot
        while let Some(registry_spend) = mempool_item
            .coin_spends
            .iter()
            .find(|c| c.coin.amount != 0 && c.coin.parent_coin_info == registry.coin.coin_id())
        {
            let Some(new_registry) = Self::from_spend(
                ctx,
                registry_spend,
                registry.info.constants,
                Signature::default(),
            )?
            else {
                break;
            };

            registry = new_registry;
        }

        // after everything is done, add all other spends to the context
        for spend in mempool_item.coin_spends {
            if spend.coin == registry.coin {
                continue;
            }

            ctx.insert(spend);
        }

        registry.set_pending_signature(mempool_item.aggregated_signature);
        Ok(Some(registry))
    }
}

impl XchandlesRegistry {
    /// Spends the registry with all pending actions.
    ///
    /// Returns the child registry and the pending signature, which must be aggregated into the
    /// final spend bundle.
    ///
    /// # Errors
    ///
    /// Returns an error if a Merkle proof can't be built for one of the pending actions.
    pub fn finish_spend(self, ctx: &mut SpendContext) -> Result<(Self, Signature), DriverError> {
        let layers = self.info.into_layers();

        let puzzle = layers.construct_puzzle(ctx)?;

        let action_puzzle_hashes = self
            .pending_spend
            .actions
            .iter()
            .map(|a| ctx.tree_hash(a.puzzle).into())
            .collect::<Vec<Bytes32>>();

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
                            &XchandlesRegistryInfo::action_puzzle_hashes(&self.info.constants),
                            &action_puzzle_hashes,
                        )
                        .ok_or(DriverError::Custom(
                            "Couldn't build proofs for one or more actions".to_string(),
                        ))?,
                    action_spends: self.pending_spend.actions,
                    finalizer_solution: NodePtr::NIL,
                },
            },
        )?;

        let my_spend = Spend::new(puzzle, solution);
        ctx.spend(self.coin, my_spend)?;

        Ok((child, self.pending_spend.signature))
    }

    /// Creates one of the registry's actions from its constants.
    pub fn new_action<A>(&self) -> A
    where
        A: SingletonAction<Self>,
    {
        A::from_constants(&self.info.constants)
    }

    /// Returns the handle slot that this registry spend creates for `slot_value`.
    pub fn created_handle_slot_value_to_slot(
        &self,
        slot_value: XchandlesHandleSlotValue,
    ) -> Slot<XchandlesHandleSlotValue> {
        let proof = LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: 1,
        };

        Slot::new(
            proof,
            SlotInfo::from_value(
                self.info.constants.launcher_id,
                XchandlesSlotNonce::HANDLE.to_u64(),
                slot_value,
            ),
        )
    }

    /// Returns the update slot that this registry spend creates for `slot_value`.
    pub fn created_update_slot_value_to_slot(
        &self,
        slot_value: XchandlesUpdateSlotValue,
    ) -> Slot<XchandlesUpdateSlotValue> {
        let proof = LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: 1,
        };

        Slot::new(
            proof,
            SlotInfo::from_value(
                self.info.constants.launcher_id,
                XchandlesSlotNonce::UPDATE.to_u64(),
                slot_value,
            ),
        )
    }

    /// Returns the handle slots that will be the neighbors of `new_handle_hash` once the pending actions run.
    ///
    /// `on_chain_left_slot` and `on_chain_right_slot` are the neighbors on chain. If pending actions
    /// create closer slots, those are returned instead.
    pub fn actual_neigbors(
        &self,
        new_handle_hash: Bytes32,
        on_chain_left_slot: Slot<XchandlesHandleSlotValue>,
        on_chain_right_slot: Slot<XchandlesHandleSlotValue>,
    ) -> (
        Slot<XchandlesHandleSlotValue>,
        Slot<XchandlesHandleSlotValue>,
    ) {
        let mut left = on_chain_left_slot;
        let mut right = on_chain_right_slot;

        for slot_value in &self.pending_spend.created_handle_slots {
            if slot_value.handle_hash < new_handle_hash
                && slot_value.handle_hash >= left.info.value.handle_hash
            {
                left = self.created_handle_slot_value_to_slot(*slot_value);
            }

            if slot_value.handle_hash > new_handle_hash
                && slot_value.handle_hash <= right.info.value.handle_hash
            {
                right = self.created_handle_slot_value_to_slot(*slot_value);
            }
        }

        (left, right)
    }

    /// Returns the latest version of a handle slot, matched by handle hash, taking pending actions into account.
    pub fn actual_handle_slot(
        &self,
        slot: Slot<XchandlesHandleSlotValue>,
    ) -> Slot<XchandlesHandleSlotValue> {
        let mut slot = slot;
        for slot_value in &self.pending_spend.created_handle_slots {
            if slot.info.value.handle_hash == slot_value.handle_hash {
                slot = self.created_handle_slot_value_to_slot(*slot_value);
            }
        }

        slot
    }

    /// Returns the latest version of an update slot, matched by initiator coin id, taking pending actions into account.
    pub fn actual_update_slot(
        &self,
        slot: Slot<XchandlesUpdateSlotValue>,
    ) -> Slot<XchandlesUpdateSlotValue> {
        let mut slot = slot;
        for slot_value in &self.pending_spend.created_update_slots {
            if slot.info.value.update_initiator_coin_id == slot_value.update_initiator_coin_id {
                slot = self.created_update_slot_value_to_slot(*slot_value);
            }
        }

        slot
    }

    /// Adds an action spend to the pending spend, updating the state, slots, and logs.
    ///
    /// Actions call this from their `spend` methods, so you rarely need to call it directly.
    pub fn insert_action_spend(
        &mut self,
        ctx: &mut SpendContext,
        action_spend: Spend,
    ) -> Result<(), DriverError> {
        let res = Self::pending_info_delta_from_spend(
            ctx,
            action_spend,
            self.pending_spend.latest_state,
            self.info.constants,
        )?;

        self.pending_spend.latest_state = res.0;
        self.pending_spend.logs.push(res.1);
        self.pending_spend.created_handle_slots.extend(res.2);
        self.pending_spend.created_update_slots.extend(res.3);
        self.pending_spend.spent_handle_slots.extend(res.4);
        self.pending_spend.spent_update_slots.extend(res.5);
        self.pending_spend.actions.push(action_spend);

        Ok(())
    }
}
