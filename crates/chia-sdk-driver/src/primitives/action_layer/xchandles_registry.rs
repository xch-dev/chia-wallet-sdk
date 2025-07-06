use chia::{
    bls::Signature,
    clvm_utils::ToTreeHash,
    protocol::{Bytes32, Coin, CoinSpend},
    puzzles::{singleton::SingletonSolution, LineageProof, Proof},
};
use chia_puzzle_types::singleton::{LauncherSolution, SingletonArgs};
use chia_wallet_sdk::driver::{DriverError, Layer, Puzzle, Spend, SpendContext};
use clvm_traits::{clvm_list, match_tuple};
use clvmr::NodePtr;

use crate::{
    eve_singleton_inner_puzzle, Action, ActionLayer, ActionLayerSolution, DelegatedStateAction,
    Registry, XchandlesExpireAction, XchandlesExtendAction, XchandlesOracleAction,
    XchandlesRefundAction, XchandlesRegisterAction, XchandlesUpdateAction,
};

use super::{
    Slot, SlotInfo, SlotProof, XchandlesConstants, XchandlesRegistryInfo, XchandlesRegistryState,
    XchandlesSlotValue,
};

#[derive(Debug, Clone)]
pub struct XchandlesPendingSpendInfo {
    pub actions: Vec<Spend>,
    pub spent_slots: Vec<XchandlesSlotValue>,
    pub created_slots: Vec<XchandlesSlotValue>,

    pub latest_state: (NodePtr, XchandlesRegistryState),

    pub signature: Signature,
}

impl XchandlesPendingSpendInfo {
    pub fn new(latest_state: XchandlesRegistryState) -> Self {
        Self {
            actions: vec![],
            created_slots: vec![],
            spent_slots: vec![],
            latest_state: (NodePtr::NIL, latest_state),
            signature: Signature::default(),
        }
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct XchandlesRegistry {
    pub coin: Coin,
    pub proof: Proof,
    pub info: XchandlesRegistryInfo,

    pub pending_spend: XchandlesPendingSpendInfo,
}

impl XchandlesRegistry {
    pub fn new(coin: Coin, proof: Proof, info: XchandlesRegistryInfo) -> Self {
        Self {
            coin,
            proof,
            info,
            pending_spend: XchandlesPendingSpendInfo::new(info.state),
        }
    }
}

impl Registry for XchandlesRegistry {
    type State = XchandlesRegistryState;
    type Constants = XchandlesConstants;
}

impl XchandlesRegistry {
    #[allow(clippy::type_complexity)]
    pub fn pending_info_delta_from_spend(
        ctx: &mut SpendContext,
        action_spend: Spend,
        current_state_and_ephemeral: (NodePtr, XchandlesRegistryState),
        constants: XchandlesConstants,
    ) -> Result<
        (
            (NodePtr, XchandlesRegistryState), // pending state
            Vec<XchandlesSlotValue>,           // created slot values
            Vec<XchandlesSlotValue>,           // spent slot values
        ),
        DriverError,
    > {
        let mut created_slots = vec![];
        let mut spent_slots = vec![];

        let expire_action = XchandlesExpireAction::from_constants(&constants);
        let expire_action_hash = expire_action.tree_hash();

        let extend_action = XchandlesExtendAction::from_constants(&constants);
        let extend_action_hash = extend_action.tree_hash();

        let oracle_action = XchandlesOracleAction::from_constants(&constants);
        let oracle_action_hash = oracle_action.tree_hash();

        let register_action = XchandlesRegisterAction::from_constants(&constants);
        let register_action_hash = register_action.tree_hash();

        let update_action = XchandlesUpdateAction::from_constants(&constants);
        let update_action_hash = update_action.tree_hash();

        let refund_action = XchandlesRefundAction::from_constants(&constants);
        let refund_action_hash = refund_action.tree_hash();

        let delegated_state_action =
            <DelegatedStateAction as Action<XchandlesRegistry>>::from_constants(&constants);
        let delegated_state_action_hash = delegated_state_action.tree_hash();

        let actual_solution = ctx.alloc(&clvm_list!(
            current_state_and_ephemeral,
            action_spend.solution
        ))?;

        let output = ctx.run(action_spend.puzzle, actual_solution)?;
        let (new_state_and_ephemeral, _) =
            ctx.extract::<match_tuple!((NodePtr, XchandlesRegistryState), NodePtr)>(output)?;

        let raw_action_hash = ctx.tree_hash(action_spend.puzzle);

        if raw_action_hash == extend_action_hash {
            spent_slots.push(XchandlesExtendAction::spent_slot_value(
                ctx,
                action_spend.solution,
            )?);
            created_slots.push(XchandlesExtendAction::created_slot_value(
                ctx,
                action_spend.solution,
            )?);
        } else if raw_action_hash == oracle_action_hash {
            let slot_value = XchandlesOracleAction::spent_slot_value(ctx, action_spend.solution)?;

            spent_slots.push(slot_value.clone());
            created_slots.push(slot_value);
        } else if raw_action_hash == update_action_hash {
            spent_slots.push(XchandlesUpdateAction::spent_slot_value(
                ctx,
                action_spend.solution,
            )?);
            created_slots.push(XchandlesUpdateAction::created_slot_value(
                ctx,
                action_spend.solution,
            )?);
        } else if raw_action_hash == refund_action_hash {
            if let Some(slot_value) =
                XchandlesRefundAction::spent_slot_value(ctx, action_spend.solution)?
            {
                spent_slots.push(slot_value.clone());
                created_slots.push(slot_value);
            };
        } else if raw_action_hash == expire_action_hash {
            spent_slots.push(XchandlesExpireAction::spent_slot_value(
                ctx,
                action_spend.solution,
            )?);
            created_slots.push(XchandlesExpireAction::created_slot_value(
                ctx,
                action_spend.solution,
            )?);
        } else if raw_action_hash == register_action_hash {
            spent_slots.extend(XchandlesRegisterAction::spent_slot_values(
                ctx,
                action_spend.solution,
            )?);
            created_slots.extend(XchandlesRegisterAction::created_slot_values(
                ctx,
                action_spend.solution,
            )?);
        } else if raw_action_hash != delegated_state_action_hash {
            // delegated state action has no effect on slots
            return Err(DriverError::InvalidMerkleProof);
        }

        Ok((new_state_and_ephemeral, created_slots, spent_slots))
    }

    pub fn pending_info_from_spend(
        ctx: &mut SpendContext,
        inner_solution: NodePtr,
        initial_state: XchandlesRegistryState,
        constants: XchandlesConstants,
    ) -> Result<XchandlesPendingSpendInfo, DriverError> {
        let mut created_slots = vec![];
        let mut spent_slots = vec![];

        let mut state_incl_ephemeral: (NodePtr, XchandlesRegistryState) =
            (NodePtr::NIL, initial_state);

        let inner_solution =
            ActionLayer::<XchandlesRegistryState, NodePtr>::parse_solution(ctx, inner_solution)?;

        for raw_action in inner_solution.action_spends.iter() {
            let res = Self::pending_info_delta_from_spend(
                ctx,
                *raw_action,
                state_incl_ephemeral,
                constants,
            )?;

            state_incl_ephemeral = res.0;
            created_slots.extend(res.1);
            spent_slots.extend(res.2);
        }

        Ok(XchandlesPendingSpendInfo {
            actions: inner_solution.action_spends,
            created_slots,
            spent_slots,
            latest_state: state_incl_ephemeral,
            signature: Signature::default(),
        })
    }

    pub fn from_spend(
        ctx: &mut SpendContext,
        spend: &CoinSpend,
        constants: XchandlesConstants,
    ) -> Result<Option<Self>, DriverError> {
        let coin = spend.coin;
        let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(ctx, puzzle_ptr);
        let solution_ptr = ctx.alloc(&spend.solution)?;

        let Some(info) = XchandlesRegistryInfo::parse(ctx, puzzle, constants)? else {
            return Ok(None);
        };

        let solution = ctx.extract::<SingletonSolution<NodePtr>>(solution_ptr)?;
        let proof = solution.lineage_proof;

        let pending_spend =
            Self::pending_info_from_spend(ctx, solution.inner_solution, info.state, constants)?;

        Ok(Some(XchandlesRegistry {
            coin,
            proof,
            info,
            pending_spend,
        }))
    }

    pub fn set_pending_signature(&mut self, signature: Signature) {
        self.pending_spend.signature = signature;
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
        constants: XchandlesConstants,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_registry) = Self::from_spend(ctx, parent_spend, constants)? else {
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

    // Also returns initial registration asset id
    #[allow(clippy::type_complexity)]
    pub fn from_launcher_solution(
        ctx: &mut SpendContext,
        launcher_coin: Coin,
        launcher_solution: NodePtr,
    ) -> Result<Option<(Self, [Slot<XchandlesSlotValue>; 2], Bytes32, u64)>, DriverError>
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

        let registry_inner_puzzle_hash: Bytes32 = info.inner_puzzle_hash().into();
        let eve_singleton_inner_puzzle = eve_singleton_inner_puzzle(
            ctx,
            launcher_id,
            XchandlesSlotValue::initial_left_end(),
            XchandlesSlotValue::initial_right_end(),
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

        let slot_proof = SlotProof {
            parent_parent_info: eve_coin.parent_coin_info,
            parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
        };
        let slots = [
            Slot::new(
                slot_proof,
                SlotInfo::from_value(launcher_id, 0, XchandlesSlotValue::initial_left_end()),
            ),
            Slot::new(
                slot_proof,
                SlotInfo::from_value(launcher_id, 0, XchandlesSlotValue::initial_right_end()),
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
}

impl XchandlesRegistry {
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

    pub fn new_action<A>(&self) -> A
    where
        A: Action<Self>,
    {
        A::from_constants(&self.info.constants)
    }

    pub fn created_slot_value_to_slot(
        &self,
        slot_value: XchandlesSlotValue,
    ) -> Slot<XchandlesSlotValue> {
        let proof = SlotProof {
            parent_parent_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
        };

        Slot::new(
            proof,
            SlotInfo::from_value(self.info.constants.launcher_id, 0, slot_value),
        )
    }

    pub fn actual_neigbors(
        &self,
        new_handle_hash: Bytes32,
        on_chain_left_slot: Slot<XchandlesSlotValue>,
        on_chain_right_slot: Slot<XchandlesSlotValue>,
    ) -> (Slot<XchandlesSlotValue>, Slot<XchandlesSlotValue>) {
        let mut left = on_chain_left_slot;
        let mut right = on_chain_right_slot;

        for slot_value in self.pending_spend.created_slots.iter() {
            if slot_value.handle_hash < new_handle_hash
                && slot_value.handle_hash >= left.info.value.handle_hash
            {
                left = self.created_slot_value_to_slot(slot_value.clone());
            }

            if slot_value.handle_hash > new_handle_hash
                && slot_value.handle_hash <= right.info.value.handle_hash
            {
                right = self.created_slot_value_to_slot(slot_value.clone());
            }
        }

        (left, right)
    }

    pub fn actual_slot(&self, slot: Slot<XchandlesSlotValue>) -> Slot<XchandlesSlotValue> {
        let mut slot = slot;
        for slot_value in self.pending_spend.created_slots.iter() {
            if slot.info.value.handle_hash == slot_value.handle_hash {
                slot = self.created_slot_value_to_slot(slot_value.clone());
            }
        }

        slot
    }

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
        self.pending_spend.created_slots.extend(res.1);
        self.pending_spend.spent_slots.extend(res.2);
        self.pending_spend.actions.push(action_spend);

        Ok(())
    }
}
