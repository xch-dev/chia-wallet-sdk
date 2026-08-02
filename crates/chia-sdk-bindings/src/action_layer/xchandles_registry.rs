use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::{SecretKey, Signature};
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzle_types::{LineageProof, singleton::SingletonStruct};
use chia_sdk_driver::{
    DelegatedStateAction, Offer, PrecommitCoin, SpendContext, XchandlesConstants,
    XchandlesExecuteUpdateAction, XchandlesExpireAction, XchandlesExtendAction,
    XchandlesInitiateUpdateAction, XchandlesOracleAction,
    XchandlesPrecommitValue as DriverXchandlesPrecommitValue, XchandlesRefundAction,
    XchandlesRegisterAction, XchandlesRegistry as SdkXchandlesRegistry, XchandlesRegistryState,
    launch_xchandles_registry as driver_launch_xchandles_registry,
};
use chia_sdk_types::{
    Conditions, MAINNET_CONSTANTS, Mod, TESTNET11_CONSTANTS,
    puzzles::{
        CompactCoinProof, XchandlesFactorPricingPuzzleArgs, XchandlesHandleSlotValue,
        XchandlesPricingSolution, XchandlesUpdateSlotValue,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{
    AsProgram, Clvm, NotarizedPayment, Program, Proof, XchandlesHandleSlot, XchandlesUpdateSlot,
};

pub fn xchandles_get_price(base_price: u64, handle: String, num_periods: u64) -> Result<u64> {
    if !XchandlesFactorPricingPuzzleArgs::is_valid_handle(&handle) {
        return Err(Error::Custom(format!(
            "Invalid handle '{handle}': must be 3-63 lowercase ASCII letters and digits"
        )));
    }
    if num_periods == 0 {
        return Err(Error::Custom(
            "num_periods must be greater than 0".to_string(),
        ));
    }
    Ok(XchandlesFactorPricingPuzzleArgs::get_price(
        base_price,
        &handle,
        num_periods,
    ))
}

pub trait XchandlesRegistryStateExt
where
    Self: Sized,
{
    fn from(
        payment_cat_tail_hash_hash: Bytes32,
        base_price: u64,
        registration_period: u64,
    ) -> Result<Self>;
}

impl XchandlesRegistryStateExt for XchandlesRegistryState {
    fn from(
        payment_cat_tail_hash_hash: Bytes32,
        base_price: u64,
        registration_period: u64,
    ) -> Result<Self> {
        Ok(XchandlesRegistryState::from(
            payment_cat_tail_hash_hash,
            base_price,
            registration_period,
        ))
    }
}

pub trait XchandlesConstantsExt
where
    Self: Sized,
{
    fn new(
        launcher_id: Bytes32,
        precommit_payout_puzzle_hash: Bytes32,
        relative_block_height: u32,
        price_singleton_launcher_id: Bytes32,
    ) -> Result<Self>;

    fn with_price_singleton(&self, price_singleton_launcher_id: Bytes32) -> Result<Self>;
    fn with_launcher_id(&self, launcher_id: Bytes32) -> Result<Self>;
}

impl XchandlesConstantsExt for XchandlesConstants {
    fn new(
        launcher_id: Bytes32,
        precommit_payout_puzzle_hash: Bytes32,
        relative_block_height: u32,
        price_singleton_launcher_id: Bytes32,
    ) -> Result<Self> {
        Ok(XchandlesConstants::new(
            launcher_id,
            precommit_payout_puzzle_hash,
            relative_block_height,
            price_singleton_launcher_id,
        ))
    }

    fn with_price_singleton(&self, price_singleton_launcher_id: Bytes32) -> Result<Self> {
        let mut constants = *self;
        constants.price_singleton_launcher_id = price_singleton_launcher_id;
        Ok(constants)
    }

    fn with_launcher_id(&self, launcher_id: Bytes32) -> Result<Self> {
        let mut constants = *self;
        constants.launcher_id = launcher_id;
        Ok(constants)
    }
}

pub trait XchandlesHandleSlotValueExt
where
    Self: Sized,
{
    #[allow(clippy::too_many_arguments)]
    fn new(
        counter: u64,
        handle_hash: Bytes32,
        left_handle_hash: Bytes32,
        right_handle_hash: Bytes32,
        expiration: u64,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
    ) -> Result<Self>;
}

impl XchandlesHandleSlotValueExt for XchandlesHandleSlotValue {
    #[allow(clippy::too_many_arguments)]
    fn new(
        counter: u64,
        handle_hash: Bytes32,
        left_handle_hash: Bytes32,
        right_handle_hash: Bytes32,
        expiration: u64,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
    ) -> Result<Self> {
        Ok(Self::new(
            counter,
            handle_hash,
            left_handle_hash,
            right_handle_hash,
            expiration,
            owner_launcher_id,
            resolved_launcher_id,
        ))
    }
}

pub trait XchandlesUpdateSlotValueExt
where
    Self: Sized,
{
    fn new(
        update_initiator_coin_id: Bytes32,
        min_height: u32,
        handle_hash: Bytes32,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
    ) -> Result<Self>;
}

impl XchandlesUpdateSlotValueExt for XchandlesUpdateSlotValue {
    fn new(
        update_initiator_coin_id: Bytes32,
        min_height: u32,
        handle_hash: Bytes32,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
    ) -> Result<Self> {
        Ok(Self::new(
            update_initiator_coin_id,
            min_height,
            handle_hash,
            new_owner_launcher_id,
            new_resolved_launcher_id,
        ))
    }
}

#[derive(Clone)]
pub struct XchandlesPrecommitValue {
    pub handle: String,
    pub secret: Bytes32,
    pub owner_launcher_id: Bytes32,
    pub resolved_launcher_id: Bytes32,
    pub payment_asset_id: Bytes32,
    pub base_price: u64,
    pub registration_period: u64,
    pub buy_time: u64,
    pub num_periods: u64,
}

impl XchandlesPrecommitValue {
    #[allow(clippy::too_many_arguments)]
    pub fn for_normal_registration(
        handle: String,
        secret: Bytes32,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
        payment_asset_id: Bytes32,
        base_price: u64,
        registration_period: u64,
        buy_time: u64,
        num_periods: u64,
    ) -> Result<Self> {
        Ok(Self {
            handle,
            secret,
            owner_launcher_id,
            resolved_launcher_id,
            payment_asset_id,
            base_price,
            registration_period,
            buy_time,
            num_periods,
        })
    }

    fn to_driver_value(&self) -> DriverXchandlesPrecommitValue {
        DriverXchandlesPrecommitValue::for_normal_registration(
            self.payment_asset_id.tree_hash(),
            XchandlesFactorPricingPuzzleArgs {
                base_price: self.base_price,
                registration_period: self.registration_period,
            }
            .curry_tree_hash(),
            &XchandlesPricingSolution {
                buy_time: self.buy_time,
                current_expiration: 0,
                handle: self.handle.clone(),
                num_periods: self.num_periods,
            },
            self.handle.clone(),
            self.secret,
            self.owner_launcher_id,
            self.resolved_launcher_id,
        )
    }
}

#[derive(Clone)]
pub struct XchandlesPrecommitCoin {
    pub coin: Coin,
    pub asset_id: Bytes32,
    pub proof: LineageProof,
    pub inner_puzzle_hash: Bytes32,
    pub value: XchandlesPrecommitValue,
    controller_singleton_struct_hash: Bytes32,
    relative_block_height: u32,
    payout_puzzle_hash: Bytes32,
    refund_puzzle_hash: Bytes32,
}

impl XchandlesPrecommitCoin {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        clvm: Clvm,
        parent_coin_id: Bytes32,
        proof: LineageProof,
        asset_id: Bytes32,
        controller_singleton_launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value: XchandlesPrecommitValue,
        precommit_amount: u64,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let controller_singleton_struct_hash =
            SingletonStruct::new(controller_singleton_launcher_id)
                .tree_hash()
                .into();
        let precommit = PrecommitCoin::new(
            &mut ctx,
            parent_coin_id,
            proof,
            asset_id,
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value.to_driver_value(),
            precommit_amount,
        )?;

        Ok(Self {
            coin: precommit.coin,
            asset_id: precommit.asset_id,
            proof: precommit.proof,
            inner_puzzle_hash: precommit.inner_puzzle_hash,
            value,
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
        })
    }

    fn to_precommit_coin(&self) -> PrecommitCoin<DriverXchandlesPrecommitValue> {
        PrecommitCoin {
            coin: self.coin,
            asset_id: self.asset_id,
            proof: self.proof,
            inner_puzzle_hash: self.inner_puzzle_hash,
            controller_singleton_struct_hash: self.controller_singleton_struct_hash,
            relative_block_height: self.relative_block_height,
            payout_puzzle_hash: self.payout_puzzle_hash,
            refund_puzzle_hash: self.refund_puzzle_hash,
            value: self.value.to_driver_value(),
        }
    }
}

#[derive(Clone)]
pub struct XchandlesRegistryFinishedSpendResult {
    pub new_registry: XchandlesRegistry,
    pub signature: Signature,
}

#[derive(Clone)]
pub struct XchandlesRegistryLaunchResult {
    pub security_signature: Signature,
    pub security_secret_key: SecretKey,
    pub registry: XchandlesRegistry,
    pub slots: Vec<XchandlesHandleSlot>,
    pub security_coin: Coin,
}

#[derive(Clone)]
pub struct XchandlesRegistryInfoFromLauncher {
    pub registry: XchandlesRegistry,
    pub initial_slots: Vec<XchandlesHandleSlot>,
    pub initial_registration_asset_id: Bytes32,
    pub initial_base_price: u64,
}

#[derive(Clone)]
pub struct XchandlesTripleConditionsResult {
    pub registry_conditions: Vec<Program>,
    pub owner_conditions: Vec<Program>,
    pub resolved_conditions: Option<Vec<Program>>,
}

#[derive(Clone)]
pub struct XchandlesExtendResult {
    pub conditions: Vec<Program>,
    pub notarized_payment: NotarizedPayment,
}

#[derive(Clone)]
pub struct XchandlesExecuteUpdateResult {
    pub registry_conditions: Vec<Program>,
    pub old_owner_conditions: Vec<Program>,
    pub new_owner_conditions: Vec<Program>,
}

#[derive(Clone)]
pub struct XchandlesRegistryActualNeighborsResult {
    pub left_slot: XchandlesHandleSlot,
    pub right_slot: XchandlesHandleSlot,
}

#[derive(Clone)]
pub struct XchandlesRegistry {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) registry: Arc<Mutex<SdkXchandlesRegistry>>,
}

impl XchandlesRegistry {
    pub fn coin(&self) -> Result<Coin> {
        Ok(self.registry.lock().unwrap().coin)
    }

    pub fn proof(&self) -> Result<Proof> {
        Ok(self.registry.lock().unwrap().proof.into())
    }

    pub fn state(&self) -> Result<XchandlesRegistryState> {
        Ok(self.registry.lock().unwrap().info.state)
    }

    pub fn constants(&self) -> Result<XchandlesConstants> {
        Ok(self.registry.lock().unwrap().info.constants)
    }

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.registry.lock().unwrap().info.inner_puzzle_hash())
    }

    pub fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.registry.lock().unwrap().info.puzzle_hash())
    }

    pub fn pending_created_handle_slots(&self) -> Result<Vec<XchandlesHandleSlot>> {
        let registry = self.registry.lock().unwrap();

        Ok(registry
            .pending_spend
            .created_handle_slots
            .clone()
            .into_iter()
            .map(|slot_value| {
                XchandlesHandleSlot::from_slot(
                    registry.created_handle_slot_value_to_slot(slot_value),
                )
            })
            .collect())
    }

    pub fn pending_created_update_slots(&self) -> Result<Vec<XchandlesUpdateSlot>> {
        let registry = self.registry.lock().unwrap();

        Ok(registry
            .pending_spend
            .created_update_slots
            .clone()
            .into_iter()
            .map(|slot_value| {
                XchandlesUpdateSlot::from_slot(
                    registry.created_update_slot_value_to_slot(slot_value),
                )
            })
            .collect())
    }

    pub fn pending_signature(&self) -> Result<Signature> {
        Ok(self
            .registry
            .lock()
            .unwrap()
            .pending_spend
            .signature
            .clone())
    }

    pub fn finish_spend(&self) -> Result<XchandlesRegistryFinishedSpendResult> {
        let mut ctx = self.clvm.lock().unwrap();

        let (registry, signature) = self
            .registry
            .lock()
            .unwrap()
            .clone()
            .finish_spend(&mut ctx)?;

        Ok(XchandlesRegistryFinishedSpendResult {
            new_registry: XchandlesRegistry {
                clvm: self.clvm.clone(),
                registry: Arc::new(Mutex::new(registry)),
            },
            signature,
        })
    }

    fn sdk_conditions_to_program_list(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
    ) -> Result<Vec<Program>> {
        let mut result = Vec::with_capacity(conditions.len());

        for condition in conditions {
            result.push(Program(self.clvm.clone(), ctx.alloc(&condition)?));
        }

        Ok(result)
    }

    fn triple_conditions_to_result(
        &self,
        ctx: &mut SpendContext,
        registry_conditions: Conditions,
        owner_conditions: Conditions,
        resolved_conditions: Option<Conditions>,
    ) -> Result<XchandlesTripleConditionsResult> {
        Ok(XchandlesTripleConditionsResult {
            registry_conditions: self.sdk_conditions_to_program_list(ctx, registry_conditions)?,
            owner_conditions: self.sdk_conditions_to_program_list(ctx, owner_conditions)?,
            resolved_conditions: resolved_conditions
                .map(|conditions| self.sdk_conditions_to_program_list(ctx, conditions))
                .transpose()?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register(
        &self,
        left_slot: XchandlesHandleSlot,
        right_slot: XchandlesHandleSlot,
        precommit_coin: XchandlesPrecommitCoin,
        base_handle_price: u64,
        registration_period: u64,
        start_time: u64,
        owner_inner_puzzle_hash: Bytes32,
        resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<XchandlesTripleConditionsResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let (registry_conditions, owner_conditions, resolved_conditions) =
            registry.new_action::<XchandlesRegisterAction>().spend(
                &mut ctx,
                &mut registry,
                left_slot.to_slot(),
                right_slot.to_slot(),
                &precommit_coin.to_precommit_coin(),
                base_handle_price,
                registration_period,
                start_time,
                owner_inner_puzzle_hash,
                resolved_inner_puzzle_hash,
            )?;

        self.triple_conditions_to_result(
            &mut ctx,
            registry_conditions,
            owner_conditions,
            resolved_conditions,
        )
    }

    pub fn refund(
        &self,
        precommit_coin: XchandlesPrecommitCoin,
        pricing_puzzle_reveal: Program,
        pricing_puzzle_solution: Program,
        slot: Option<XchandlesHandleSlot>,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let conditions = registry.new_action::<XchandlesRefundAction>().spend(
            &mut ctx,
            &mut registry,
            &precommit_coin.to_precommit_coin(),
            pricing_puzzle_reveal.1,
            pricing_puzzle_solution.1,
            slot.map(XchandlesHandleSlot::to_slot),
        )?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn extend(
        &self,
        handle: String,
        slot: XchandlesHandleSlot,
        payment_asset_id: Bytes32,
        base_handle_price: u64,
        registration_period: u64,
        num_periods: u64,
        buy_time: u64,
    ) -> Result<XchandlesExtendResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let (conditions, notarized_payment) =
            registry.new_action::<XchandlesExtendAction>().spend(
                &mut ctx,
                &mut registry,
                &handle,
                slot.to_slot(),
                payment_asset_id,
                base_handle_price,
                registration_period,
                num_periods,
                buy_time,
            )?;

        Ok(XchandlesExtendResult {
            conditions: self.sdk_conditions_to_program_list(&mut ctx, conditions)?,
            notarized_payment: notarized_payment.as_program(&self.clvm),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn expire(
        &self,
        slot: XchandlesHandleSlot,
        num_periods: u64,
        base_handle_price: u64,
        registration_period: u64,
        precommit_coin: XchandlesPrecommitCoin,
        start_time: u64,
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<XchandlesTripleConditionsResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let (registry_conditions, owner_conditions, resolved_conditions) =
            registry.new_action::<XchandlesExpireAction>().spend(
                &mut ctx,
                &mut registry,
                slot.to_slot(),
                num_periods,
                base_handle_price,
                registration_period,
                &precommit_coin.to_precommit_coin(),
                start_time,
                new_owner_inner_puzzle_hash,
                new_resolved_inner_puzzle_hash,
            )?;

        self.triple_conditions_to_result(
            &mut ctx,
            registry_conditions,
            owner_conditions,
            resolved_conditions,
        )
    }

    pub fn oracle(&self, slot: XchandlesHandleSlot) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let conditions = registry.new_action::<XchandlesOracleAction>().spend(
            &mut ctx,
            &mut registry,
            slot.to_slot(),
        )?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    pub fn initiate_update(
        &self,
        slot: XchandlesHandleSlot,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
        current_owner: CompactCoinProof,
        min_height: u32,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let conditions = registry
            .new_action::<XchandlesInitiateUpdateAction>()
            .spend(
                &mut ctx,
                &mut registry,
                slot.to_slot(),
                new_owner_launcher_id,
                new_resolved_launcher_id,
                current_owner,
                min_height,
            )?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_update(
        &self,
        handle_slot: XchandlesHandleSlot,
        update_slot: XchandlesUpdateSlot,
        new_owner_launcher_id: Bytes32,
        new_resolved_launcher_id: Bytes32,
        current_owner: CompactCoinProof,
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Result<XchandlesExecuteUpdateResult> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let (registry_conditions, old_owner_conditions, new_owner_conditions) = registry
            .new_action::<XchandlesExecuteUpdateAction>()
            .spend(
                &mut ctx,
                &mut registry,
                handle_slot.to_slot(),
                update_slot.to_slot(),
                new_owner_launcher_id,
                new_resolved_launcher_id,
                current_owner,
                new_owner_inner_puzzle_hash,
                new_resolved_inner_puzzle_hash,
            )?;

        Ok(XchandlesExecuteUpdateResult {
            registry_conditions: self
                .sdk_conditions_to_program_list(&mut ctx, registry_conditions)?,
            old_owner_conditions: self
                .sdk_conditions_to_program_list(&mut ctx, old_owner_conditions)?,
            new_owner_conditions: self
                .sdk_conditions_to_program_list(&mut ctx, new_owner_conditions)?,
        })
    }

    pub fn delegated_state(
        &self,
        new_state: XchandlesRegistryState,
        other_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let (conditions, action_spend) = registry.new_action::<DelegatedStateAction>().spend(
            &mut ctx,
            registry.coin,
            new_state,
            other_singleton_inner_puzzle_hash,
        )?;

        registry.insert_action_spend(&mut ctx, action_spend)?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    pub fn actual_neighbors(
        &self,
        new_handle_hash: Bytes32,
        on_chain_left_slot: XchandlesHandleSlot,
        on_chain_right_slot: XchandlesHandleSlot,
    ) -> Result<XchandlesRegistryActualNeighborsResult> {
        let registry = self.registry.lock().unwrap();
        let (left, right) = registry.actual_neigbors(
            new_handle_hash,
            on_chain_left_slot.to_slot(),
            on_chain_right_slot.to_slot(),
        );

        Ok(XchandlesRegistryActualNeighborsResult {
            left_slot: XchandlesHandleSlot::from_slot(left),
            right_slot: XchandlesHandleSlot::from_slot(right),
        })
    }

    pub fn actual_handle_slot(&self, slot: XchandlesHandleSlot) -> Result<XchandlesHandleSlot> {
        let registry = self.registry.lock().unwrap();
        Ok(XchandlesHandleSlot::from_slot(
            registry.actual_handle_slot(slot.to_slot()),
        ))
    }

    pub fn actual_update_slot(&self, slot: XchandlesUpdateSlot) -> Result<XchandlesUpdateSlot> {
        let registry = self.registry.lock().unwrap();
        Ok(XchandlesUpdateSlot::from_slot(
            registry.actual_update_slot(slot.to_slot()),
        ))
    }

    pub fn parse_launcher_solution(
        launcher_coin: Coin,
        launcher_solution: Program,
    ) -> Result<Option<XchandlesRegistryInfoFromLauncher>> {
        let mut ctx = launcher_solution.0.lock().unwrap();

        Ok(SdkXchandlesRegistry::from_launcher_solution(
            &mut ctx,
            launcher_coin,
            launcher_solution.1,
        )?
        .map(
            |(registry, slots, initial_registration_asset_id, initial_base_price)| {
                XchandlesRegistryInfoFromLauncher {
                    registry: XchandlesRegistry {
                        clvm: launcher_solution.0.clone(),
                        registry: Arc::new(Mutex::new(registry)),
                    },
                    initial_slots: slots
                        .into_iter()
                        .map(XchandlesHandleSlot::from_slot)
                        .collect(),
                    initial_registration_asset_id,
                    initial_base_price,
                }
            },
        ))
    }
}

impl Clvm {
    #[allow(clippy::too_many_arguments)]
    pub fn launch_xchandles_registry(
        &self,
        offer: SpendBundle,
        initial_base_registration_price: u64,
        initial_registration_period: u64,
        constants: XchandlesConstants,
        initial_registration_asset_id: Bytes32,
        mainnet: bool,
    ) -> Result<XchandlesRegistryLaunchResult> {
        let mut ctx = self.0.lock().unwrap();
        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;

        let (security_signature, security_secret_key, registry, slots, security_coin) =
            driver_launch_xchandles_registry(
                &mut ctx,
                &offer,
                initial_base_registration_price,
                initial_registration_period,
                |_ctx, launcher_id, _coin, (constants, asset_id)| {
                    Ok((
                        Conditions::new(),
                        constants.with_launcher_id(launcher_id),
                        asset_id,
                    ))
                },
                if mainnet {
                    &MAINNET_CONSTANTS
                } else {
                    &TESTNET11_CONSTANTS
                },
                (constants, initial_registration_asset_id),
            )?;

        Ok(XchandlesRegistryLaunchResult {
            security_signature,
            security_secret_key,
            registry: XchandlesRegistry {
                clvm: self.0.clone(),
                registry: Arc::new(Mutex::new(registry)),
            },
            slots: slots
                .into_iter()
                .map(XchandlesHandleSlot::from_slot)
                .collect(),
            security_coin,
        })
    }

    pub fn xchandles_registry_from_spend(
        &self,
        spend: chia_protocol::CoinSpend,
        constants: XchandlesConstants,
    ) -> Result<Option<XchandlesRegistry>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(
            SdkXchandlesRegistry::from_spend(&mut ctx, &spend, constants, Signature::default())?
                .map(|registry| XchandlesRegistry {
                    clvm: self.0.clone(),
                    registry: Arc::new(Mutex::new(registry)),
                }),
        )
    }

    pub fn xchandles_registry_from_parent_spend(
        &self,
        parent_spend: chia_protocol::CoinSpend,
        constants: XchandlesConstants,
    ) -> Result<Option<XchandlesRegistry>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(
            SdkXchandlesRegistry::from_parent_spend(&mut ctx, &parent_spend, constants)?.map(
                |registry| XchandlesRegistry {
                    clvm: self.0.clone(),
                    registry: Arc::new(Mutex::new(registry)),
                },
            ),
        )
    }

    pub fn xchandles_registry_from_mempool_item(
        &self,
        mempool_item: SpendBundle,
        constants: XchandlesConstants,
    ) -> Result<Option<XchandlesRegistry>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(
            SdkXchandlesRegistry::from_mempool_item(&mut ctx, mempool_item, constants)?.map(
                |registry| XchandlesRegistry {
                    clvm: self.0.clone(),
                    registry: Arc::new(Mutex::new(registry)),
                },
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_price_accepts_valid_handles_and_rejects_invalid() {
        assert_eq!(xchandles_get_price(5, "abc".into(), 1).unwrap(), 640);
        assert_eq!(xchandles_get_price(5, "example1".into(), 2).unwrap(), 10);
        assert_eq!(
            xchandles_get_price(
                1,
                "bigbouncingthicctwerkingthunderclappingbadonkabooty".into(),
                1
            )
            .unwrap(),
            2
        );
        assert_eq!(
            xchandles_get_price(
                1,
                "rolexislandpermutoplatinumlamboempirexrp404inu".into(),
                1
            )
            .unwrap(),
            1
        );

        assert!(xchandles_get_price(5, "aa".into(), 1).is_err());
        assert!(xchandles_get_price(5, "a".repeat(64), 1).is_err());
        assert!(xchandles_get_price(5, "ABC".into(), 1).is_err());
        assert!(xchandles_get_price(5, "abc".into(), 0).is_err());
    }
}
