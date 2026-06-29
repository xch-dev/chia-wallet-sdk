use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::{SecretKey, Signature};
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzle_types::{LineageProof, singleton::SingletonStruct};
use chia_sdk_driver::{
    CatalogPrecommitValue as DriverCatalogPrecommitValue, CatalogRefundAction,
    CatalogRegisterAction, CatalogRegistry as SdkCatalogRegistry, CatalogRegistryConstants,
    CatalogRegistryState, DelegatedStateAction, Offer, PrecommitCoin, SpendContext,
    launch_catalog_registry as driver_launch_catalog_registry,
};
use chia_sdk_types::{
    Conditions, MAINNET_CONSTANTS, TESTNET11_CONSTANTS, puzzles::SlotNeigborsInfo,
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{CatalogSlot, Clvm, Program, Proof, Spend as BindingSpend};

pub trait SlotNeigborsInfoExt {}

impl SlotNeigborsInfoExt for SlotNeigborsInfo {}

pub trait CatalogSlotValueExt
where
    Self: Sized,
{
    fn new(
        counter: u64,
        asset_id: Bytes32,
        left_asset_id: Bytes32,
        right_asset_id: Bytes32,
    ) -> Result<Self>;
}

impl CatalogSlotValueExt for chia_sdk_types::puzzles::CatalogSlotValue {
    fn new(
        counter: u64,
        asset_id: Bytes32,
        left_asset_id: Bytes32,
        right_asset_id: Bytes32,
    ) -> Result<Self> {
        Ok(Self::new(counter, asset_id, left_asset_id, right_asset_id))
    }
}

pub trait CatalogRegistryConstantsExt
where
    Self: Sized,
{
    fn get(testnet11: bool) -> Result<Self>;
    fn with_price_singleton(&self, price_singleton_launcher_id: Bytes32) -> Result<Self>;
    fn with_launcher_id(&self, launcher_id: Bytes32) -> Result<Self>;
}

impl CatalogRegistryConstantsExt for CatalogRegistryConstants {
    fn get(testnet11: bool) -> Result<Self> {
        Ok(Self::get(testnet11))
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

pub trait CatalogRegistryStateExt {}

impl CatalogRegistryStateExt for CatalogRegistryState {}

#[derive(Clone)]
pub struct CatalogPrecommitValue {
    pub tail_reveal: Program,
    pub initial_inner_puzzle_hash: Bytes32,
    pub payment_asset_id: Bytes32,
}

impl CatalogPrecommitValue {
    pub fn with_default_cat_maker(
        _clvm: Clvm,
        payment_asset_id: Bytes32,
        initial_inner_puzzle_hash: Bytes32,
        tail_reveal: Program,
    ) -> Result<Self> {
        Ok(Self {
            tail_reveal,
            initial_inner_puzzle_hash,
            payment_asset_id,
        })
    }

    fn to_driver_value(&self) -> DriverCatalogPrecommitValue<NodePtr> {
        DriverCatalogPrecommitValue::with_default_cat_maker(
            self.payment_asset_id.tree_hash(),
            self.initial_inner_puzzle_hash,
            self.tail_reveal.1,
        )
    }
}

#[derive(Clone)]
pub struct CatalogPrecommitCoin {
    pub coin: Coin,
    pub asset_id: Bytes32,
    pub proof: LineageProof,
    pub inner_puzzle_hash: Bytes32,
    pub value: CatalogPrecommitValue,
    controller_singleton_struct_hash: Bytes32,
    relative_block_height: u32,
    payout_puzzle_hash: Bytes32,
    refund_puzzle_hash: Bytes32,
}

impl CatalogPrecommitCoin {
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
        value: CatalogPrecommitValue,
        precommit_amount: u64,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let controller_singleton_struct_hash =
            SingletonStruct::new(controller_singleton_launcher_id)
                .tree_hash()
                .into();
        let driver_value = value.to_driver_value();
        let precommit = PrecommitCoin::new(
            &mut ctx,
            parent_coin_id,
            proof,
            asset_id,
            controller_singleton_struct_hash,
            relative_block_height,
            payout_puzzle_hash,
            refund_puzzle_hash,
            driver_value,
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

    fn to_precommit_coin(&self) -> PrecommitCoin<DriverCatalogPrecommitValue<NodePtr>> {
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
pub struct CatalogRegistryFinishedSpendResult {
    pub new_catalog: CatalogRegistry,
    pub signature: Signature,
}

#[derive(Clone)]
pub struct CatalogRegistryLaunchResult {
    pub security_signature: Signature,
    pub security_secret_key: SecretKey,
    pub catalog: CatalogRegistry,
    pub slots: Vec<CatalogSlot>,
    pub security_coin: Coin,
}

#[derive(Clone)]
pub struct CatalogRegistryActualNeighborsResult {
    pub left_slot: CatalogSlot,
    pub right_slot: CatalogSlot,
}

#[derive(Clone)]
pub struct CatalogRegistry {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) catalog: Arc<Mutex<SdkCatalogRegistry>>,
}

impl CatalogRegistry {
    pub fn coin(&self) -> Result<Coin> {
        Ok(self.catalog.lock().unwrap().coin)
    }

    pub fn proof(&self) -> Result<Proof> {
        Ok(self.catalog.lock().unwrap().proof.into())
    }

    pub fn state(&self) -> Result<CatalogRegistryState> {
        Ok(self.catalog.lock().unwrap().info.state)
    }

    pub fn constants(&self) -> Result<CatalogRegistryConstants> {
        Ok(self.catalog.lock().unwrap().info.constants)
    }

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.catalog.lock().unwrap().info.inner_puzzle_hash())
    }

    pub fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.catalog.lock().unwrap().info.puzzle_hash())
    }

    pub fn pending_created_slots(&self) -> Result<Vec<CatalogSlot>> {
        let catalog = self.catalog.lock().unwrap();

        Ok(catalog
            .pending_spend
            .created_slots
            .clone()
            .into_iter()
            .map(|slot_value| {
                CatalogSlot::from_slot(catalog.created_slot_value_to_slot(slot_value))
            })
            .collect())
    }

    pub fn pending_signature(&self) -> Result<Signature> {
        Ok(self.catalog.lock().unwrap().pending_spend.signature.clone())
    }

    pub fn finish_spend(&self) -> Result<CatalogRegistryFinishedSpendResult> {
        let mut ctx = self.clvm.lock().unwrap();

        let (catalog, signature) = self
            .catalog
            .lock()
            .unwrap()
            .clone()
            .finish_spend(&mut ctx)?;

        Ok(CatalogRegistryFinishedSpendResult {
            new_catalog: CatalogRegistry {
                clvm: self.clvm.clone(),
                catalog: Arc::new(Mutex::new(catalog)),
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

    pub fn register(
        &self,
        tail_hash: Bytes32,
        left_slot: CatalogSlot,
        right_slot: CatalogSlot,
        precommit_coin: CatalogPrecommitCoin,
        eve_nft_inner_spend: BindingSpend,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut catalog = self.catalog.lock().unwrap();

        let conditions = catalog.new_action::<CatalogRegisterAction>().spend(
            &mut ctx,
            &mut catalog,
            tail_hash,
            left_slot.to_slot(),
            right_slot.to_slot(),
            &precommit_coin.to_precommit_coin(),
            eve_nft_inner_spend.into(),
        )?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    pub fn refund(
        &self,
        tail_hash: Bytes32,
        precommit_coin: CatalogPrecommitCoin,
        neighbors: Option<SlotNeigborsInfo>,
        slot: Option<CatalogSlot>,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut catalog = self.catalog.lock().unwrap();

        let conditions = catalog.new_action::<CatalogRefundAction>().spend(
            &mut ctx,
            &mut catalog,
            tail_hash,
            neighbors,
            &precommit_coin.to_precommit_coin(),
            slot.map(CatalogSlot::to_slot),
        )?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    pub fn delegated_state(
        &self,
        new_state: CatalogRegistryState,
        other_singleton_inner_puzzle_hash: Bytes32,
    ) -> Result<Vec<Program>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut catalog = self.catalog.lock().unwrap();

        let (conditions, action_spend) = catalog.new_action::<DelegatedStateAction>().spend(
            &mut ctx,
            catalog.coin,
            new_state,
            other_singleton_inner_puzzle_hash,
        )?;

        catalog.insert_action_spend(&mut ctx, action_spend)?;

        self.sdk_conditions_to_program_list(&mut ctx, conditions)
    }

    pub fn actual_neighbors(
        &self,
        new_tail_hash: Bytes32,
        on_chain_left_slot: CatalogSlot,
        on_chain_right_slot: CatalogSlot,
    ) -> Result<CatalogRegistryActualNeighborsResult> {
        let catalog = self.catalog.lock().unwrap();
        let (left, right) = catalog.actual_neigbors(
            new_tail_hash,
            on_chain_left_slot.to_slot(),
            on_chain_right_slot.to_slot(),
        );

        Ok(CatalogRegistryActualNeighborsResult {
            left_slot: CatalogSlot::from_slot(left),
            right_slot: CatalogSlot::from_slot(right),
        })
    }

    pub fn actual_slot(&self, slot: CatalogSlot) -> Result<CatalogSlot> {
        let catalog = self.catalog.lock().unwrap();
        Ok(CatalogSlot::from_slot(catalog.actual_slot(slot.to_slot())))
    }
}

impl Clvm {
    #[allow(clippy::too_many_arguments)]
    pub fn launch_catalog_registry(
        &self,
        offer: SpendBundle,
        initial_registration_price: u64,
        constants: CatalogRegistryConstants,
        initial_registration_asset_id: Bytes32,
        mainnet: bool,
    ) -> Result<CatalogRegistryLaunchResult> {
        let mut ctx = self.0.lock().unwrap();
        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;

        let (security_signature, security_secret_key, catalog, slots, security_coin) =
            driver_launch_catalog_registry(
                &mut ctx,
                &offer,
                initial_registration_price,
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

        Ok(CatalogRegistryLaunchResult {
            security_signature,
            security_secret_key,
            catalog: CatalogRegistry {
                clvm: self.0.clone(),
                catalog: Arc::new(Mutex::new(catalog)),
            },
            slots: slots.into_iter().map(CatalogSlot::from_slot).collect(),
            security_coin,
        })
    }

    pub fn catalog_registry_from_spend(
        &self,
        spend: chia_protocol::CoinSpend,
        constants: CatalogRegistryConstants,
    ) -> Result<Option<CatalogRegistry>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(
            SdkCatalogRegistry::from_spend(&mut ctx, &spend, constants, Signature::default())?.map(
                |catalog| CatalogRegistry {
                    clvm: self.0.clone(),
                    catalog: Arc::new(Mutex::new(catalog)),
                },
            ),
        )
    }

    pub fn catalog_registry_from_parent_spend(
        &self,
        parent_spend: chia_protocol::CoinSpend,
        constants: CatalogRegistryConstants,
    ) -> Result<Option<CatalogRegistry>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(
            SdkCatalogRegistry::from_parent_spend(&mut ctx, &parent_spend, constants)?.map(
                |catalog| CatalogRegistry {
                    clvm: self.0.clone(),
                    catalog: Arc::new(Mutex::new(catalog)),
                },
            ),
        )
    }

    pub fn catalog_registry_from_mempool_item(
        &self,
        mempool_item: SpendBundle,
        constants: CatalogRegistryConstants,
    ) -> Result<Option<CatalogRegistry>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(
            SdkCatalogRegistry::from_mempool_item(&mut ctx, mempool_item, constants)?.map(
                |catalog| CatalogRegistry {
                    clvm: self.0.clone(),
                    catalog: Arc::new(Mutex::new(catalog)),
                },
            ),
        )
    }
}
