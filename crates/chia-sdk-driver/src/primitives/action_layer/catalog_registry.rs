use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{LineageProof, Proof, singleton::SingletonSolution};
use chia_sdk_types::puzzles::{CatalogSlotValue, DelegatedStateActionSolution, SlotInfo};
use clvm_traits::{clvm_tuple, match_tuple};
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

use crate::{
    ActionLayer, ActionLayerSolution, ActionSingleton, CatalogActionLog,
    CatalogDelegatedStateActionLog, CatalogRefundAction, CatalogRegisterAction,
    DelegatedStateAction, DriverError, Layer, Puzzle, SingletonAction, Spend, SpendContext,
};

use super::{CatalogRegistryConstants, CatalogRegistryInfo, CatalogRegistryState, Slot};

/// The actions queued for the next spend of a [`CatalogRegistry`], and their effects.
#[derive(Debug, Clone)]
pub struct CatalogPendingSpendInfo {
    /// The action spends, in the order they will run.
    pub actions: Vec<Spend>,
    /// The values of the slots the actions create.
    pub created_slots: Vec<CatalogSlotValue>,
    /// The values of the slots the actions spend.
    pub spent_slots: Vec<CatalogSlotValue>,

    /// A log entry for each action.
    pub logs: Vec<CatalogActionLog>,

    /// The ephemeral state and registry state after the queued actions have run.
    pub latest_state: (NodePtr, CatalogRegistryState),

    /// The aggregated signature that the spend needs, such as one from a parsed mempool item.
    pub signature: Signature,
}

impl CatalogPendingSpendInfo {
    /// Creates an empty pending spend starting from the given state.
    pub fn new(latest_state: CatalogRegistryState) -> Self {
        Self {
            actions: vec![],
            created_slots: vec![],
            spent_slots: vec![],
            logs: vec![],
            latest_state: (NodePtr::NIL, latest_state),
            signature: Signature::default(),
        }
    }
}

/// The `CATalog` registry: an action-layer singleton that registers CATs.
///
/// Registered CATs are stored as a sorted linked list of [`Slot`]s keyed by asset id, starting
/// with the two end slots created at launch (see [`launch_catalog_registry`](crate::launch_catalog_registry)).
/// Registering a CAT spends a [`PrecommitCoin`](crate::PrecommitCoin), inserts a slot between its
/// neighbors, and mints an NFT for the CAT through a
/// [`UniquenessPrelauncher`](crate::UniquenessPrelauncher).
///
/// To spend the registry, build actions with [`CatalogRegistry::new_action`] and spend them. Most
/// actions' `spend` methods call [`CatalogRegistry::insert_action_spend`] and return conditions that
/// another coin in the bundle should output to secure the action. The
/// [`DelegatedStateAction`] returns its [`Spend`] instead, which you insert yourself. Finally, call
/// [`CatalogRegistry::finish_spend`]. While actions are pending, use
/// [`CatalogRegistry::actual_neigbors`] and [`CatalogRegistry::actual_slot`] to account for
/// slots created earlier in the same spend.
#[derive(Debug, Clone)]
#[must_use]
pub struct CatalogRegistry {
    /// The registry singleton coin.
    pub coin: Coin,
    /// The proof needed to spend the singleton.
    pub proof: Proof,
    /// The state and constants of the registry.
    pub info: CatalogRegistryInfo,

    /// The actions queued for the next spend.
    pub pending_spend: CatalogPendingSpendInfo,
}

impl CatalogRegistry {
    /// Creates a new [`CatalogRegistry`] with no pending actions.
    pub fn new(coin: Coin, proof: Proof, info: CatalogRegistryInfo) -> Self {
        Self {
            coin,
            proof,
            info,
            pending_spend: CatalogPendingSpendInfo::new(info.state),
        }
    }
}

impl CatalogRegistry {
    /// Runs one action spend on top of the given state and identifies it.
    ///
    /// Returns the new ephemeral and registry state, the action's log, and the values of the slots
    /// it creates and spends, in that order.
    ///
    /// # Errors
    ///
    /// Returns [`DriverError::InvalidMerkleProof`] if the action isn't one of the registry's actions.
    #[allow(clippy::type_complexity)]
    pub fn pending_info_delta_from_spend(
        ctx: &mut SpendContext,
        action_spend: Spend,
        current_state_and_ephemeral: (NodePtr, CatalogRegistryState),
        constants: CatalogRegistryConstants,
    ) -> Result<
        (
            (NodePtr, CatalogRegistryState),
            CatalogActionLog,
            Vec<CatalogSlotValue>, // created slot values
            Vec<CatalogSlotValue>, // spent slot values
        ),
        DriverError,
    > {
        let mut created_slots = vec![];
        let mut spent_slots = vec![];

        let state = current_state_and_ephemeral.1;

        let register_action = CatalogRegisterAction::from_constants(&constants);
        let register_hash = register_action.tree_hash();

        let refund_action = CatalogRefundAction::from_constants(&constants);
        let refund_hash = refund_action.tree_hash();

        let delegated_state_action =
            <DelegatedStateAction as SingletonAction<CatalogRegistry>>::from_constants(&constants);
        let delegated_state_hash = delegated_state_action.tree_hash();

        let actual_solution = ctx.alloc(&clvm_tuple!(
            current_state_and_ephemeral,
            action_spend.solution
        ))?;

        let output = ctx.run(action_spend.puzzle, actual_solution)?;
        let (new_state_and_ephemeral, _) =
            ctx.extract::<match_tuple!((NodePtr, CatalogRegistryState), NodePtr)>(output)?;

        let raw_action_hash = ctx.tree_hash(action_spend.puzzle);

        let log = if raw_action_hash == register_hash {
            let action_log = CatalogActionLog::Register(CatalogRegisterAction::get_log(
                ctx,
                action_spend.solution,
                state.registration_price,
            )?);
            action_log.extend_spent_slots(&mut spent_slots);
            action_log.extend_created_slots(&mut created_slots);

            action_log
        } else if raw_action_hash == refund_hash {
            let action_log = CatalogActionLog::Refund(CatalogRefundAction::get_log(
                ctx,
                action_spend.solution,
                state,
            )?);
            action_log.extend_spent_slots(&mut spent_slots);
            action_log.extend_created_slots(&mut created_slots);
            action_log
        } else if raw_action_hash == delegated_state_hash {
            let sol = ctx.extract::<DelegatedStateActionSolution<CatalogRegistryState>>(
                action_spend.solution,
            )?;

            CatalogActionLog::DelegatedState(CatalogDelegatedStateActionLog {
                old_state: current_state_and_ephemeral.1,
                new_state: sol.new_state,
            })
        } else {
            return Err(DriverError::InvalidMerkleProof);
        };

        Ok((new_state_and_ephemeral, log, created_slots, spent_slots))
    }

    /// Parses all of the actions in an action-layer inner solution into a [`CatalogPendingSpendInfo`].
    pub fn pending_info_from_spend(
        ctx: &mut SpendContext,
        inner_solution: NodePtr,
        initial_state: CatalogRegistryState,
        constants: CatalogRegistryConstants,
        signature: Signature,
    ) -> Result<CatalogPendingSpendInfo, DriverError> {
        let mut created_slots = vec![];
        let mut spent_slots = vec![];
        let mut logs = vec![];

        let mut state_incl_ephemeral: (NodePtr, CatalogRegistryState) =
            (NodePtr::NIL, initial_state);

        let inner_solution =
            ActionLayer::<CatalogRegistryState, NodePtr>::parse_solution(ctx, inner_solution)?;

        for raw_action in &inner_solution.action_spends {
            let res = Self::pending_info_delta_from_spend(
                ctx,
                *raw_action,
                state_incl_ephemeral,
                constants,
            )?;

            state_incl_ephemeral = res.0;
            logs.push(res.1);
            created_slots.extend(res.2);
            spent_slots.extend(res.3);
        }

        Ok(CatalogPendingSpendInfo {
            actions: inner_solution.action_spends,
            created_slots,
            spent_slots,
            logs,
            latest_state: state_incl_ephemeral,
            signature,
        })
    }

    /// Sets the signature that will be returned by [`CatalogRegistry::finish_spend`].
    pub fn set_pending_signature(&mut self, signature: Signature) {
        self.pending_spend.signature = signature;
    }

    /// Parses a registry and its actions from a spend of the registry coin.
    ///
    /// The returned registry is the spent coin, with its actions in `pending_spend`. Returns
    /// [`None`] if the coin doesn't have amount 1 or isn't a registry with the given constants.
    pub fn from_spend(
        ctx: &mut SpendContext,
        spend: &CoinSpend,
        constants: CatalogRegistryConstants,
        signature: Signature,
    ) -> Result<Option<Self>, DriverError> {
        let coin = spend.coin;

        if coin.amount != 1 {
            return Ok(None);
        }

        let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(ctx, puzzle_ptr);

        let Some(info) = CatalogRegistryInfo::parse(ctx, puzzle, constants)? else {
            return Ok(None);
        };

        let solution_ptr = ctx.alloc(&spend.solution)?;
        let solution = ctx.extract::<SingletonSolution<NodePtr>>(solution_ptr)?;
        let proof = solution.lineage_proof;

        let pending_spend = Self::pending_info_from_spend(
            ctx,
            solution.inner_solution,
            info.state,
            constants,
            signature,
        )?;

        Ok(Some(CatalogRegistry {
            coin,
            proof,
            info,
            pending_spend,
        }))
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
        constants: CatalogRegistryConstants,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_registry) =
            CatalogRegistry::from_spend(ctx, parent_spend, constants, Signature::default())?
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

        Ok(Some(CatalogRegistry {
            coin: new_coin,
            proof,
            info: new_info,
            pending_spend: CatalogPendingSpendInfo::new(new_info.state),
        }))
    }

    /// Returns the registry coin created by spending this one, with the given state.
    pub fn child(&self, child_state: CatalogRegistryState) -> Self {
        let new_info = self.info.with_state(child_state);
        let new_coin = Coin::new(self.coin.coin_id(), new_info.puzzle_hash().into(), 1);

        CatalogRegistry {
            coin: new_coin,
            proof: Proof::Lineage(self.child_lineage_proof()),
            info: new_info,
            pending_spend: CatalogPendingSpendInfo::new(new_info.state),
        }
    }

    /// Parses the latest registry from a mempool item and prepares to spend it again.
    ///
    /// Finds the first registry spend in the bundle, follows its children through the bundle to the
    /// latest registry coin, and parses that coin's actions into `pending_spend`. All other spends
    /// are inserted into `ctx`, and the bundle's aggregated signature becomes the pending signature.
    /// This lets you add more actions to an unconfirmed registry spend and replace it with
    /// [`CatalogRegistry::finish_spend`].
    pub fn from_mempool_item(
        ctx: &mut SpendContext,
        mempool_item: SpendBundle,
        constants: CatalogRegistryConstants,
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

impl ActionSingleton for CatalogRegistry {
    type State = CatalogRegistryState;
    type Constants = CatalogRegistryConstants;
}

impl CatalogRegistry {
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
                            &CatalogRegistryInfo::action_puzzle_hashes(&self.info.constants),
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

    /// Returns the slot that this registry spend creates for `slot_value`.
    pub fn created_slot_value_to_slot(
        &self,
        slot_value: CatalogSlotValue,
    ) -> Slot<CatalogSlotValue> {
        Slot::new(
            LineageProof {
                parent_parent_coin_info: self.coin.parent_coin_info,
                parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
                parent_amount: self.coin.amount,
            },
            SlotInfo::from_value(self.info.constants.launcher_id, 0, slot_value),
        )
    }

    /// Returns the slots that will be the neighbors of `new_tail_hash` once the pending actions run.
    ///
    /// `on_chain_left_slot` and `on_chain_right_slot` are the neighbors on chain. If pending actions
    /// create closer slots, those are returned instead.
    pub fn actual_neigbors(
        &self,
        new_tail_hash: Bytes32,
        on_chain_left_slot: Slot<CatalogSlotValue>,
        on_chain_right_slot: Slot<CatalogSlotValue>,
    ) -> (Slot<CatalogSlotValue>, Slot<CatalogSlotValue>) {
        let mut left = on_chain_left_slot;
        let mut right = on_chain_right_slot;

        for slot_value in &self.pending_spend.created_slots {
            if slot_value.asset_id < new_tail_hash
                && slot_value.asset_id >= left.info.value.asset_id
            {
                left = self.created_slot_value_to_slot(*slot_value);
            }

            if slot_value.asset_id > new_tail_hash
                && slot_value.asset_id <= right.info.value.asset_id
            {
                right = self.created_slot_value_to_slot(*slot_value);
            }
        }

        (left, right)
    }

    /// Returns the latest version of `slot`, taking slots created by pending actions into account.
    pub fn actual_slot(&self, slot: Slot<CatalogSlotValue>) -> Slot<CatalogSlotValue> {
        let mut slot = slot;
        for slot_value in &self.pending_spend.created_slots {
            if slot.info.value.asset_id == slot_value.asset_id {
                slot = self.created_slot_value_to_slot(*slot_value);
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
        self.pending_spend.created_slots.extend(res.2);
        self.pending_spend.spent_slots.extend(res.3);
        self.pending_spend.actions.push(action_spend);

        Ok(())
    }
}
