use std::fmt::Debug;

use chia_protocol::Bytes32;
use chia_sdk_types::{
    conditions::{CreateCoin, NewMetadataOutput, TransferNft, UpdateNftMetadata},
    Conditions,
};
use clvm_traits::clvm_list;
use clvmr::NodePtr;

use crate::{
    Did, DidInfo, DriverError, HashedPtr, Launcher, Nft, OptionContract, Spend, SpendContext,
    SpendKind,
};

#[derive(Debug, Clone)]
pub struct SingletonSpends<A>
where
    A: SingletonAsset,
{
    pub lineage: Vec<SingletonSpend<A>>,
    pub ephemeral: bool,
}

impl<A> SingletonSpends<A>
where
    A: SingletonAsset,
{
    pub fn new(asset: A, spend: SpendKind, ephemeral: bool) -> Self {
        Self {
            lineage: vec![SingletonSpend::new(asset, spend)],
            ephemeral,
        }
    }

    pub fn last(&self) -> Result<&SingletonSpend<A>, DriverError> {
        self.lineage.last().ok_or(DriverError::NoSourceForOutput)
    }

    pub fn last_mut(&mut self) -> Result<&mut SingletonSpend<A>, DriverError> {
        self.lineage
            .last_mut()
            .ok_or(DriverError::NoSourceForOutput)
    }

    pub fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        change_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        loop {
            let last = self
                .lineage
                .last_mut()
                .ok_or(DriverError::NoSourceForOutput)?;

            if last.kind.outputs().has_singleton_output() {
                break;
            }

            let child = A::create_change(ctx, last, change_puzzle_hash)?;

            if A::needs_additional_spend(&child.child_info) {
                self.lineage.push(child);
            } else {
                break;
            }
        }

        Ok(())
    }

    pub fn launcher_source(&mut self) -> Result<(usize, u64), DriverError> {
        let Some((index, amount)) = self.lineage.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .outputs()
                .launcher_amount()
                .map(|amount| (index, amount))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        Ok((index, amount))
    }

    pub fn create_launcher(
        &mut self,
        singleton_amount: u64,
    ) -> Result<(usize, Launcher), DriverError> {
        let (index, launcher_amount) = self.launcher_source()?;

        let (parent_conditions, launcher) =
            Launcher::create_early(self.lineage[index].asset.get_coin_id(), launcher_amount);

        match &mut self.lineage[index].kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions)?;
            }
        }

        Ok((index, launcher.with_singleton_amount(singleton_amount)))
    }
}

#[derive(Debug, Clone)]
pub struct SingletonSpend<A>
where
    A: SingletonAsset,
{
    pub asset: A,
    pub kind: SpendKind,
    pub child_info: A::ChildInfo,
}

impl<A> SingletonSpend<A>
where
    A: SingletonAsset,
{
    pub fn new(asset: A, kind: SpendKind) -> Self {
        let child_info = A::default_child_info(&asset, &kind);

        Self {
            asset,
            kind,
            child_info,
        }
    }
}

pub trait SingletonAsset: Debug + Clone {
    type ChildInfo: Debug + Clone;

    fn get_coin_id(&self) -> Bytes32;
    fn p2_puzzle_hash(&self) -> Bytes32;
    fn default_child_info(asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo;
    fn needs_additional_spend(child_info: &Self::ChildInfo) -> bool;
    fn create_change(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        change_puzzle_hash: Bytes32,
    ) -> Result<SingletonSpend<Self>, DriverError>;
}

impl SingletonAsset for Did<HashedPtr> {
    type ChildInfo = ChildDidInfo;

    fn get_coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn default_child_info(asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo {
        ChildDidInfo {
            recovery_list_hash: asset.info.recovery_list_hash,
            num_verifications_required: asset.info.num_verifications_required,
            metadata: asset.info.metadata,
            destination: None,
            new_spend_kind: spend_kind.child(),
            needs_update: false,
        }
    }

    fn needs_additional_spend(child_info: &Self::ChildInfo) -> bool {
        child_info.needs_update
    }

    fn create_change(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        change_puzzle_hash: Bytes32,
    ) -> Result<SingletonSpend<Self>, DriverError> {
        let change_hint = ctx.hint(change_puzzle_hash)?;

        let current_info = singleton.asset.info;
        let child_info = &singleton.child_info;
        let new_spend_kind = child_info.new_spend_kind.clone();

        // If the DID layer has changed, we need to perform an update spend to ensure wallets can properly sync the coin.
        let needs_update = current_info.recovery_list_hash != child_info.recovery_list_hash
            || current_info.num_verifications_required != child_info.num_verifications_required
            || current_info.metadata != child_info.metadata;

        let final_destination = child_info.destination;

        let destination = if needs_update {
            let p2_puzzle_hash = current_info.p2_puzzle_hash;
            let hint = ctx.hint(p2_puzzle_hash)?;
            CreateCoin::new(p2_puzzle_hash, singleton.asset.coin.amount, hint)
        } else {
            child_info.destination.unwrap_or(CreateCoin::new(
                change_puzzle_hash,
                singleton.asset.coin.amount,
                change_hint,
            ))
        };

        let child_info = DidInfo::new(
            current_info.launcher_id,
            child_info.recovery_list_hash,
            child_info.num_verifications_required,
            child_info.metadata,
            destination.puzzle_hash,
        );

        // Create the new DID coin with the updated DID info. The DID puzzle does not automatically wrap the output.
        match &mut singleton.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().create_coin(
                    child_info.inner_puzzle_hash().into(),
                    destination.amount,
                    destination.memos,
                ))?;
            }
        }

        // Create a new singleton spend with the child and the new spend kind.
        // This will only be added to the lineage if an additional spend is required.
        let mut new_spend =
            SingletonSpend::new(singleton.asset.child_with(child_info), new_spend_kind);

        // Signal that an additional spend is required.
        new_spend.child_info.needs_update = needs_update;

        if needs_update {
            new_spend.child_info.destination = final_destination;
        }

        Ok(new_spend)
    }
}

impl SingletonAsset for Nft<HashedPtr> {
    type ChildInfo = ChildNftInfo;

    fn get_coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn default_child_info(_asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo {
        ChildNftInfo {
            metadata_update_spends: Vec::new(),
            transfer_condition: None,
            destination: None,
            new_spend_kind: spend_kind.child(),
        }
    }

    fn needs_additional_spend(child_info: &Self::ChildInfo) -> bool {
        !child_info.metadata_update_spends.is_empty() || child_info.transfer_condition.is_some()
    }

    fn create_change(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        change_puzzle_hash: Bytes32,
    ) -> Result<SingletonSpend<Self>, DriverError> {
        let change_hint = ctx.hint(change_puzzle_hash)?;

        let mut new_child_info = singleton.child_info.clone();

        let metadata_update_spend = new_child_info.metadata_update_spends.pop();
        let transfer_condition = new_child_info.transfer_condition.take();
        let needs_additional_spend = Self::needs_additional_spend(&new_child_info);

        let destination = if needs_additional_spend {
            let p2_puzzle_hash = singleton.asset.info.p2_puzzle_hash;
            let hint = ctx.hint(p2_puzzle_hash)?;
            CreateCoin::new(p2_puzzle_hash, singleton.asset.coin.amount, hint)
        } else {
            new_child_info.destination.unwrap_or(CreateCoin::new(
                change_puzzle_hash,
                singleton.asset.coin.amount,
                change_hint,
            ))
        };

        let mut nft_info = singleton.asset.info;
        nft_info.p2_puzzle_hash = destination.puzzle_hash;

        // Create the new NFT coin with the updated info.
        match &mut singleton.kind {
            SpendKind::Conditions(spend) => {
                let mut conditions = Conditions::new().with(destination);

                if let Some(spend) = metadata_update_spend {
                    conditions.push(UpdateNftMetadata::new(spend.puzzle, spend.solution));

                    let metadata_updater_solution = ctx.alloc(&clvm_list!(
                        singleton.asset.info.metadata,
                        singleton.asset.info.metadata_updater_puzzle_hash,
                        spend.solution
                    ))?;
                    let ptr = ctx.run(spend.puzzle, metadata_updater_solution)?;
                    let output = ctx.extract::<NewMetadataOutput<HashedPtr, NodePtr>>(ptr)?;

                    nft_info.metadata = output.metadata_info.new_metadata;
                    nft_info.metadata_updater_puzzle_hash =
                        output.metadata_info.new_updater_puzzle_hash;
                }

                if let Some(transfer_condition) = transfer_condition {
                    nft_info.current_owner = transfer_condition.launcher_id;
                    conditions.push(transfer_condition);
                }

                spend.add_conditions(conditions)?;
            }
        }

        // Create a new singleton spend with the child and the new spend kind.
        let mut spend = SingletonSpend::new(
            singleton
                .asset
                .child_with(nft_info, singleton.asset.coin.amount),
            singleton.child_info.new_spend_kind.clone(),
        );

        spend.child_info = new_child_info;

        Ok(spend)
    }
}

impl SingletonAsset for OptionContract {
    type ChildInfo = ChildOptionInfo;

    fn get_coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn default_child_info(_asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo {
        ChildOptionInfo {
            destination: None,
            new_spend_kind: spend_kind.child(),
        }
    }

    fn needs_additional_spend(_child_info: &Self::ChildInfo) -> bool {
        false
    }

    fn create_change(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        change_puzzle_hash: Bytes32,
    ) -> Result<SingletonSpend<Self>, DriverError> {
        let change_hint = ctx.hint(change_puzzle_hash)?;

        let destination = singleton.child_info.destination.unwrap_or(CreateCoin::new(
            change_puzzle_hash,
            singleton.asset.coin.amount,
            change_hint,
        ));

        // Create the new option contract coin.
        match &mut singleton.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().with(destination))?;
            }
        }

        // Create a new singleton spend with the child and the new spend kind.
        Ok(SingletonSpend::new(
            singleton
                .asset
                .child(destination.puzzle_hash, singleton.asset.coin.amount),
            singleton.child_info.new_spend_kind.clone(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ChildDidInfo {
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: HashedPtr,
    pub destination: Option<CreateCoin<NodePtr>>,
    pub new_spend_kind: SpendKind,
    pub needs_update: bool,
}

#[derive(Debug, Clone)]
pub struct ChildNftInfo {
    pub metadata_update_spends: Vec<Spend>,
    pub transfer_condition: Option<TransferNft>,
    pub destination: Option<CreateCoin<NodePtr>>,
    pub new_spend_kind: SpendKind,
}

#[derive(Debug, Clone)]
pub struct ChildOptionInfo {
    pub destination: Option<CreateCoin<NodePtr>>,
    pub new_spend_kind: SpendKind,
}
