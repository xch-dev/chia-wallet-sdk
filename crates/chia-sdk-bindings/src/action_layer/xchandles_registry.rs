use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::{PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes, Bytes32, Coin, SpendBundle};
use chia_puzzle_types::{LineageProof, singleton::SingletonStruct, standard::StandardArgs};
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_driver::{
    DelegatedStateAction, HashedPtr, NftInfo, Offer, PrecommitCoin, SingletonInfo, SpendContext,
    XchandlesConstants, XchandlesExecuteUpdateAction, XchandlesExpireAction,
    XchandlesExpirePricingPuzzle, XchandlesExtendAction, XchandlesInitiateUpdateAction,
    XchandlesOracleAction, XchandlesPrecommitValue as DriverXchandlesPrecommitValue,
    XchandlesRefundAction, XchandlesRegisterAction, XchandlesRegistry as SdkXchandlesRegistry,
    XchandlesRegistryReceivedMessagePrefix, XchandlesRegistryState,
    launch_xchandles_registry as driver_launch_xchandles_registry,
};
use chia_sdk_types::{
    Conditions, MAINNET_CONSTANTS, Mod, TESTNET11_CONSTANTS,
    puzzles::{
        ANY_METADATA_UPDATER_HASH, CompactCoinProof, HandleNftMetadata, StateSchedulerLayerArgs,
        XchandlesFactorPricingPuzzleArgs, XchandlesHandleSlotValue, XchandlesPricingSolution,
        XchandlesUpdateSlotValue,
    },
};
use clvm_traits::{ToClvm, clvm_quote};
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

/// Tree hash of `SingletonStruct::new(launcher_id)`.
pub fn singleton_struct_hash(launcher_id: Bytes32) -> Result<Bytes32> {
    Ok(SingletonStruct::new(launcher_id).tree_hash().into())
}

/// `XCHandles` register-owner received-message bytes for a precommit puzzle hash.
pub fn xchandles_register_owner_message(precommit_puzzle_hash: Bytes32) -> Result<Bytes> {
    Ok(Bytes::new(
        XchandlesRegistryReceivedMessagePrefix::register_owner(precommit_puzzle_hash),
    ))
}

/// `XCHandles` expire-owner received-message bytes for a precommit puzzle hash.
pub fn xchandles_expire_owner_message(precommit_puzzle_hash: Bytes32) -> Result<Bytes> {
    Ok(Bytes::new(
        XchandlesRegistryReceivedMessagePrefix::expire_owner(precommit_puzzle_hash),
    ))
}

/// Predict the eve blank Handle NFT coin id (nil metadata, `AnyMetadataUpdater`, royalty).
pub fn predict_blank_handle_nft_coin_id(
    launcher_id: Bytes32,
    synthetic_public_key: PublicKey,
    royalty_puzzle_hash: Bytes32,
    royalty_basis_points: u16,
) -> Result<Bytes32> {
    let p2_puzzle_hash: Bytes32 = StandardArgs::curry_tree_hash(synthetic_public_key).into();
    let mut allocator = clvmr::Allocator::new();
    let metadata_ptr = HandleNftMetadata::default()
        .to_clvm(&mut allocator)
        .map_err(|e| Error::Custom(format!("allocate blank metadata: {e}")))?;
    let metadata = HashedPtr::from_ptr(&allocator, metadata_ptr);
    let info = NftInfo::new(
        launcher_id,
        metadata,
        ANY_METADATA_UPDATER_HASH.into(),
        None,
        royalty_puzzle_hash,
        royalty_basis_points,
        p2_puzzle_hash,
    );
    Ok(Coin::new(launcher_id, info.puzzle_hash().into(), 1).coin_id())
}

/// Build the constrained registration state-scheduler delegated puzzle hash.
///
/// Inner conditions are `create_coin(p2, 1, hint)` + `update_nft_metadata` + `assert_seconds_absolute`,
/// wrapped in the reusable state-scheduler layer with the register/expire owner message.
#[allow(clippy::too_many_arguments)]
pub fn xchandles_registration_delegated_puzzle_hash(
    registry_launcher_id: Bytes32,
    precommit_puzzle_hash: Bytes32,
    p2_puzzle_hash: Bytes32,
    registration_timestamp: u64,
    current_expiration: u64,
    final_handle_nft_metadata: HandleNftMetadata,
) -> Result<Bytes32> {
    let mut ctx = SpendContext::new();
    let message = if current_expiration == 0 {
        XchandlesRegistryReceivedMessagePrefix::register_owner(precommit_puzzle_hash)
    } else {
        XchandlesRegistryReceivedMessagePrefix::expire_owner(precommit_puzzle_hash)
    };

    let metadata_updater = ctx
        .alloc_mod::<chia_sdk_types::puzzles::AnyMetadataUpdater>()
        .map_err(|e| Error::Custom(format!("allocate metadata updater: {e}")))?;
    let metadata_ptr = ctx
        .alloc(&final_handle_nft_metadata)
        .map_err(|e| Error::Custom(format!("alloc final metadata: {e}")))?;
    let hint = ctx
        .hint(p2_puzzle_hash)
        .map_err(|e| Error::Custom(format!("allocate p2 hint: {e}")))?;
    let inner_conditions = Conditions::new()
        .create_coin(p2_puzzle_hash, 1, hint)
        .update_nft_metadata(metadata_updater, metadata_ptr)
        .assert_seconds_absolute(registration_timestamp);
    let inner_puzzle = ctx
        .alloc(&clvm_quote!(inner_conditions))
        .map_err(|e| Error::Custom(format!("allocate inner delegated conditions: {e}")))?;
    let receiver_singleton_struct_hash: Bytes32 = SingletonStruct::new(registry_launcher_id)
        .tree_hash()
        .into();
    let delegated = ctx
        .curry(StateSchedulerLayerArgs::<Bytes, clvmr::NodePtr> {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            receiver_singleton_struct_hash,
            prefix_and_message: Bytes::new(message),
            inner_puzzle,
        })
        .map_err(|e| Error::Custom(format!("allocate state-scheduler delegated puzzle: {e}")))?;
    Ok(ctx.tree_hash(delegated).into())
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
    /// Pricing-solution `current_expiration`. Zero for ordinary available-Handle
    /// registrations; the exact slot expiration for expiry-auction purchases.
    pub current_expiration: u64,
    /// When true, commit the deployed expiry-pricing puzzle. When false, keep the
    /// historical factor-pricing tree hash used by `for_normal_registration`.
    pub use_expire_pricing: bool,
}

/// Validate expiry-pricing precommit fields for ordinary `register`.
///
/// Register reveals the committed pricing solution; that solution must use
/// `current_expiration = 0` (available-Handle registration), not an auction
/// vector intended for `expire`.
fn validate_register_expiry_pricing_commitment(
    value: &XchandlesPrecommitValue,
    base_handle_price: u64,
    registration_period: u64,
    start_time: u64,
) -> Result<()> {
    if base_handle_price != value.base_price || registration_period != value.registration_period {
        return Err(Error::Custom(
            "register base_price/registration_period must match expiry-pricing precommit"
                .to_string(),
        ));
    }
    if start_time != value.buy_time {
        return Err(Error::Custom(
            "register start_time must match committed buy_time".to_string(),
        ));
    }
    if value.current_expiration != 0 {
        return Err(Error::Custom(
            "register requires committed current_expiration = 0 (use expire for auction purchases)"
                .to_string(),
        ));
    }
    Ok(())
}

/// Validate expiry-pricing precommit fields for `expire`.
///
/// Expire rebuilds the pricing solution from the Handle slot's expiration; the
/// precommit must have committed that same `current_expiration` or the spend
/// fails on-chain when reconstructing the precommit value.
fn validate_expire_expiry_pricing_commitment(
    value: &XchandlesPrecommitValue,
    base_handle_price: u64,
    registration_period: u64,
    start_time: u64,
    num_periods: u64,
    slot_expiration: u64,
) -> Result<()> {
    if base_handle_price != value.base_price || registration_period != value.registration_period {
        return Err(Error::Custom(
            "expire base_price/registration_period must match expiry-pricing precommit".to_string(),
        ));
    }
    if start_time != value.buy_time {
        return Err(Error::Custom(
            "expire start_time must match committed buy_time".to_string(),
        ));
    }
    if num_periods != value.num_periods {
        return Err(Error::Custom(
            "expire num_periods must match committed num_periods".to_string(),
        ));
    }
    if value.current_expiration != slot_expiration {
        return Err(Error::Custom(
            "expire current_expiration must match slot expiration".to_string(),
        ));
    }
    Ok(())
}

impl XchandlesPrecommitValue {
    /// Factor-pricing precommit used by historical / non-expiry consumers.
    /// Always commits `current_expiration = 0` and the factor-pricing puzzle hash.
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
            current_expiration: 0,
            use_expire_pricing: false,
        })
    }

    /// Expiry-pricing precommit for the currently deployed pricing puzzle.
    ///
    /// Accepts the explicit `XchandlesPricingSolution` fields (`buy_time`,
    /// `current_expiration`, `handle`, `num_periods`) plus base price and the
    /// fixed registration period. Usable for both ordinary
    /// (`current_expiration = 0`) and expiry-auction (nonzero) vectors, and
    /// remains consumable by `XchandlesPrecommitCoin`.
    #[allow(clippy::too_many_arguments)]
    pub fn for_expiry_pricing_registration(
        handle: String,
        secret: Bytes32,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
        payment_asset_id: Bytes32,
        base_price: u64,
        registration_period: u64,
        buy_time: u64,
        current_expiration: u64,
        num_periods: u64,
    ) -> Result<Self> {
        if num_periods == 0 {
            return Err(Error::Custom(
                "num_periods must be greater than 0".to_string(),
            ));
        }
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
            current_expiration,
            use_expire_pricing: true,
        })
    }

    fn pricing_puzzle_hash(&self) -> TreeHash {
        if self.use_expire_pricing {
            XchandlesExpirePricingPuzzle::curry_tree_hash(self.base_price, self.registration_period)
        } else {
            XchandlesFactorPricingPuzzleArgs {
                base_price: self.base_price,
                registration_period: self.registration_period,
            }
            .curry_tree_hash()
        }
    }

    /// Tree hash of the pricing puzzle this precommit commits.
    pub fn committed_pricing_puzzle_hash(&self) -> TreeHash {
        self.pricing_puzzle_hash()
    }

    /// On-chain pricing solution committed into this precommit value.
    pub fn pricing_solution(&self) -> XchandlesPricingSolution {
        XchandlesPricingSolution {
            buy_time: self.buy_time,
            current_expiration: if self.use_expire_pricing {
                self.current_expiration
            } else {
                0
            },
            handle: self.handle.clone(),
            num_periods: self.num_periods,
        }
    }

    fn to_driver_value(&self) -> DriverXchandlesPrecommitValue {
        DriverXchandlesPrecommitValue::for_normal_registration(
            self.payment_asset_id.tree_hash(),
            self.pricing_puzzle_hash(),
            &self.pricing_solution(),
            self.handle.clone(),
            self.secret,
            self.owner_launcher_id,
            self.resolved_launcher_id,
        )
    }

    /// On-chain precommit value tree hash (same digest `PrecommitCoin` embeds).
    pub fn commitment_hash(&self) -> Result<Bytes32> {
        let mut ctx = SpendContext::new();
        let ptr = ctx.alloc(&self.to_driver_value())?;
        Ok(ctx.tree_hash(ptr).into())
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

    /// Full CAT puzzle hash for an expiry/factor precommit with the given parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn puzzle_hash(
        asset_id: Bytes32,
        controller_singleton_launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
        refund_puzzle_hash: Bytes32,
        value: XchandlesPrecommitValue,
    ) -> Result<Bytes32> {
        let controller_singleton_struct_hash =
            SingletonStruct::new(controller_singleton_launcher_id)
                .tree_hash()
                .into();
        let value_hash = value.commitment_hash()?;
        Ok(PrecommitCoin::<DriverXchandlesPrecommitValue>::puzzle_hash(
            asset_id,
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            value_hash.into(),
        )
        .into())
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

    /// Ordinary Handle registration.
    ///
    /// When `precommit_coin.value.use_expire_pricing` is true, reveals the same
    /// deployed expiry-pricing puzzle and exact committed
    /// `XchandlesPricingSolution` as the precommit. Requires ordinary
    /// `current_expiration = 0` (auction vectors belong on `expire`). Otherwise
    /// preserves the historical factor-pricing helper path.
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
        let action = registry.new_action::<XchandlesRegisterAction>();

        let (registry_conditions, owner_conditions, resolved_conditions) =
            if precommit_coin.value.use_expire_pricing {
                validate_register_expiry_pricing_commitment(
                    &precommit_coin.value,
                    base_handle_price,
                    registration_period,
                    start_time,
                )?;
                let pricing_puzzle = XchandlesRegisterAction::expiry_pricing_puzzle(
                    &mut ctx,
                    base_handle_price,
                    registration_period,
                )?;
                let pricing_solution = precommit_coin.value.pricing_solution();
                action.spend_with_pricing(
                    &mut ctx,
                    &mut registry,
                    left_slot.to_slot(),
                    right_slot.to_slot(),
                    &precommit_coin.to_precommit_coin(),
                    pricing_puzzle,
                    pricing_solution,
                    owner_inner_puzzle_hash,
                    resolved_inner_puzzle_hash,
                )?
            } else {
                action.spend(
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
                )?
            };

        self.triple_conditions_to_result(
            &mut ctx,
            registry_conditions,
            owner_conditions,
            resolved_conditions,
        )
    }

    /// Refund by revealing an explicit pricing puzzle and solution.
    ///
    /// Prefer [`Self::refund`] when the reveal can be derived from the
    /// committed precommit value.
    pub fn refund_with_pricing(
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

    /// Refund using the pricing reveal implied by a committed precommit value.
    ///
    /// Expiry-pricing precommits curry the deployed expire-pricing puzzle and
    /// the exact committed `XchandlesPricingSolution`. Factor-pricing
    /// precommits curry factor pricing with `current_expiration = 0`.
    /// Callers still pass an optional Handle slot when protocol state requires
    /// it (conflicting active registration).
    pub fn refund(
        &self,
        precommit_coin: XchandlesPrecommitCoin,
        slot: Option<XchandlesHandleSlot>,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut registry = self.registry.lock().unwrap();

        let pricing_puzzle = if precommit_coin.value.use_expire_pricing {
            XchandlesRegisterAction::expiry_pricing_puzzle(
                &mut ctx,
                precommit_coin.value.base_price,
                precommit_coin.value.registration_period,
            )?
        } else {
            ctx.curry(XchandlesFactorPricingPuzzleArgs {
                base_price: precommit_coin.value.base_price,
                registration_period: precommit_coin.value.registration_period,
            })?
        };
        let pricing_solution = ctx.alloc(&precommit_coin.value.pricing_solution())?;

        let conditions = registry.new_action::<XchandlesRefundAction>().spend(
            &mut ctx,
            &mut registry,
            &precommit_coin.to_precommit_coin(),
            pricing_puzzle,
            pricing_solution,
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

    /// Expiry-auction purchase (nonzero current expiration).
    ///
    /// When `precommit_coin.value.use_expire_pricing` is true, `start_time`,
    /// `num_periods`, and `current_expiration` must match the committed pricing
    /// solution (`current_expiration` must equal the Handle slot expiration).
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

        let (start_time, num_periods) = if precommit_coin.value.use_expire_pricing {
            validate_expire_expiry_pricing_commitment(
                &precommit_coin.value,
                base_handle_price,
                registration_period,
                start_time,
                num_periods,
                slot.value.expiration,
            )?;
            (
                precommit_coin.value.buy_time,
                precommit_coin.value.num_periods,
            )
        } else {
            (start_time, num_periods)
        };

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

    fn driver_commitment_hash(
        payment_asset_id: Bytes32,
        pricing_puzzle_hash: TreeHash,
        pricing_solution: &XchandlesPricingSolution,
        handle: String,
        secret: Bytes32,
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
    ) -> Bytes32 {
        let mut ctx = SpendContext::new();
        let value = DriverXchandlesPrecommitValue::for_normal_registration(
            payment_asset_id.tree_hash(),
            pricing_puzzle_hash,
            pricing_solution,
            handle,
            secret,
            owner_launcher_id,
            resolved_launcher_id,
        );
        let ptr = ctx.alloc(&value).unwrap();
        ctx.tree_hash(ptr).into()
    }

    #[test]
    fn factor_pricing_factory_preserves_zero_expiration_commitment() {
        let payment = Bytes32::new([0x11; 32]);
        let secret = Bytes32::new([0x22; 32]);
        let owner = Bytes32::new([0x33; 32]);
        let binding = XchandlesPrecommitValue::for_normal_registration(
            "alice".into(),
            secret,
            owner,
            owner,
            payment,
            5_000,
            31_557_600,
            1_787_216_820,
            2,
        )
        .unwrap();

        assert!(!binding.use_expire_pricing);
        assert_eq!(binding.current_expiration, 0);

        let expected = driver_commitment_hash(
            payment,
            XchandlesFactorPricingPuzzleArgs {
                base_price: 5_000,
                registration_period: 31_557_600,
            }
            .curry_tree_hash(),
            &XchandlesPricingSolution {
                buy_time: 1_787_216_820,
                current_expiration: 0,
                handle: "alice".into(),
                num_periods: 2,
            },
            "alice".into(),
            secret,
            owner,
            owner,
        );
        assert_eq!(binding.commitment_hash().unwrap(), expected);
    }

    #[test]
    fn expiry_pricing_factory_matches_driver_for_zero_and_nonzero_expiration() {
        let payment = Bytes32::new([0xaa; 32]);
        let secret = Bytes32::new([0xbb; 32]);
        let owner = Bytes32::new([0xcc; 32]);
        let registration_period = 31_557_600_u64;
        let base_price = 5_000_u64;
        let buy_time = 1_787_216_820_u64;

        for current_expiration in [0_u64, 1_800_000_000_u64] {
            let binding = XchandlesPrecommitValue::for_expiry_pricing_registration(
                "alice".into(),
                secret,
                owner,
                owner,
                payment,
                base_price,
                registration_period,
                buy_time,
                current_expiration,
                1,
            )
            .unwrap();

            assert!(binding.use_expire_pricing);
            assert_eq!(binding.current_expiration, current_expiration);

            let expected = driver_commitment_hash(
                payment,
                XchandlesExpirePricingPuzzle::curry_tree_hash(base_price, registration_period),
                &XchandlesPricingSolution {
                    buy_time,
                    current_expiration,
                    handle: "alice".into(),
                    num_periods: 1,
                },
                "alice".into(),
                secret,
                owner,
                owner,
            );
            assert_eq!(
                binding.commitment_hash().unwrap(),
                expected,
                "commitment mismatch for current_expiration={current_expiration}"
            );
        }
    }

    #[test]
    fn expiry_pricing_precommit_coin_consumes_binding_value() {
        let payment = Bytes32::new([0x01; 32]);
        let secret = Bytes32::new([0x02; 32]);
        let owner = Bytes32::new([0x03; 32]);
        let controller = Bytes32::new([0x04; 32]);
        let payout = Bytes32::new([0x05; 32]);
        let refund = Bytes32::new([0x06; 32]);
        let parent = Bytes32::new([0x07; 32]);

        let value = XchandlesPrecommitValue::for_expiry_pricing_registration(
            "bob".into(),
            secret,
            owner,
            owner,
            payment,
            5_000,
            31_557_600,
            1_787_216_820,
            1_800_000_000,
            1,
        )
        .unwrap();

        let clvm = Clvm::new().unwrap();
        let coin = XchandlesPrecommitCoin::new(
            clvm,
            parent,
            LineageProof {
                parent_parent_coin_info: Bytes32::new([0x08; 32]),
                parent_inner_puzzle_hash: Bytes32::new([0x09; 32]),
                parent_amount: 1_000,
            },
            payment,
            controller,
            32,
            payout,
            refund,
            value.clone(),
            10_000,
        )
        .unwrap();

        assert_eq!(coin.value.handle, "bob");
        assert_eq!(coin.value.current_expiration, 1_800_000_000);
        assert!(coin.value.use_expire_pricing);
        assert_eq!(coin.coin.amount, 10_000);
    }

    #[test]
    fn expiry_pricing_solution_fields_are_exact_for_register_and_expire_vectors() {
        let payment = Bytes32::new([0x11; 32]);
        let secret = Bytes32::new([0x22; 32]);
        let owner = Bytes32::new([0x33; 32]);
        let buy_time = 1_787_216_820_u64;
        let base_price = 5_000_u64;
        let registration_period = 31_557_600_u64;

        let ordinary = XchandlesPrecommitValue::for_expiry_pricing_registration(
            "alice".into(),
            secret,
            owner,
            owner,
            payment,
            base_price,
            registration_period,
            buy_time,
            0,
            2,
        )
        .unwrap();
        let ordinary_solution = ordinary.pricing_solution();
        assert_eq!(ordinary_solution.buy_time, buy_time);
        assert_eq!(ordinary_solution.current_expiration, 0);
        assert_eq!(ordinary_solution.handle, "alice");
        assert_eq!(ordinary_solution.num_periods, 2);
        assert_eq!(
            ordinary.committed_pricing_puzzle_hash(),
            XchandlesExpirePricingPuzzle::curry_tree_hash(base_price, registration_period)
        );

        let auction = XchandlesPrecommitValue::for_expiry_pricing_registration(
            "alice".into(),
            secret,
            owner,
            owner,
            payment,
            base_price,
            registration_period,
            buy_time,
            1_800_000_000,
            1,
        )
        .unwrap();
        let auction_solution = auction.pricing_solution();
        assert_eq!(auction_solution.current_expiration, 1_800_000_000);
        assert_eq!(auction_solution.num_periods, 1);
        assert_ne!(
            ordinary.commitment_hash().unwrap(),
            auction.commitment_hash().unwrap()
        );
    }

    fn sample_expiry_precommit(
        current_expiration: u64,
        num_periods: u64,
    ) -> XchandlesPrecommitValue {
        XchandlesPrecommitValue::for_expiry_pricing_registration(
            "alice".into(),
            Bytes32::new([0x22; 32]),
            Bytes32::new([0x33; 32]),
            Bytes32::new([0x33; 32]),
            Bytes32::new([0x11; 32]),
            5_000,
            31_557_600,
            1_787_216_820,
            current_expiration,
            num_periods,
        )
        .unwrap()
    }

    #[test]
    fn register_rejects_nonzero_committed_expiration() {
        let value = sample_expiry_precommit(1_800_000_000, 1);
        let err =
            validate_register_expiry_pricing_commitment(&value, 5_000, 31_557_600, 1_787_216_820)
                .expect_err("auction precommit must not register");
        assert!(
            err.to_string().contains("current_expiration"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn register_accepts_zero_committed_expiration() {
        let value = sample_expiry_precommit(0, 2);
        validate_register_expiry_pricing_commitment(&value, 5_000, 31_557_600, 1_787_216_820)
            .unwrap();
    }

    #[test]
    fn expire_rejects_committed_expiration_mismatching_slot() {
        let value = sample_expiry_precommit(1_800_000_000, 1);
        let err = validate_expire_expiry_pricing_commitment(
            &value,
            5_000,
            31_557_600,
            1_787_216_820,
            1,
            1_900_000_000,
        )
        .expect_err("slot expiration must match commitment");
        assert!(
            err.to_string().contains("current_expiration"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn expire_accepts_committed_expiration_matching_slot() {
        let value = sample_expiry_precommit(1_800_000_000, 1);
        validate_expire_expiry_pricing_commitment(
            &value,
            5_000,
            31_557_600,
            1_787_216_820,
            1,
            1_800_000_000,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod registration_helpers_tests {
    use super::*;
    use chia_bls::SecretKey;

    fn sk() -> SecretKey {
        let mut bytes = [0u8; 32];
        bytes[0] = 1;
        bytes[31] = 7;
        SecretKey::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn blank_nft_prediction_is_stable() {
        let launcher = Bytes32::new([0x44; 32]);
        let mut royalty_bytes = [0u8; 32];
        royalty_bytes[0] = 0x36;
        royalty_bytes[1] = 0xda;
        let royalty = Bytes32::new(royalty_bytes);
        let a =
            predict_blank_handle_nft_coin_id(launcher, sk().public_key(), royalty, 420).unwrap();
        let b =
            predict_blank_handle_nft_coin_id(launcher, sk().public_key(), royalty, 420).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn registration_delegated_puzzle_differs_for_register_vs_expire() {
        let registry = Bytes32::new([0x11; 32]);
        let precommit = Bytes32::new([0xdd; 32]);
        let p2 = Bytes32::new([0x33; 32]);
        let meta = HandleNftMetadata {
            display_name: Some("alice".into()),
            image_uris: vec!["https://example.com/a.png".into()],
            image_hash: Some(Bytes32::from([0x11; 32])),
            metadata_uris: vec!["https://example.com/a.json".into()],
            metadata_hash: Some(Bytes32::from([0x22; 32])),
            license_uris: vec!["https://example.com/license.txt".into()],
            license_hash: Some(Bytes32::from([0x33; 32])),
        };
        let register = xchandles_registration_delegated_puzzle_hash(
            registry,
            precommit,
            p2,
            1_787_216_820,
            0,
            meta.clone(),
        )
        .unwrap();
        let expire = xchandles_registration_delegated_puzzle_hash(
            registry,
            precommit,
            p2,
            1_787_216_820,
            1_800_000_000,
            meta,
        )
        .unwrap();
        assert_ne!(register, expire);
    }
}
