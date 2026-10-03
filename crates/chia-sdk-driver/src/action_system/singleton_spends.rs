use std::fmt::Debug;

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH};
use chia_sdk_types::{
    Conditions,
    conditions::{AssertPuzzleAnnouncement, CreateCoin, TransferNft, UpdateNftMetadata},
};
use clvmr::NodePtr;

use crate::{
    Asset, Did, DidInfo, DriverError, FungibleSpend, HashedPtr, Launcher, Nft, NftInfo,
    OptionContract, OutputSet, SingletonInfo, Spend, SpendContext, SpendKind, run_metadata_updater,
};

/// The spends of a singleton in the transaction.
///
/// A singleton can be spent more than once in the same transaction, for example to update a DID
/// before sending it, or to move an NFT out of a settlement coin so that it can emit conditions.
#[derive(Debug, Clone)]
pub struct SingletonSpends<A>
where
    A: SingletonAsset,
{
    /// The spends in order, each of which creates the next. Actions apply to the last spend.
    pub lineage: Vec<SingletonSpend<A>>,
    /// Whether the singleton was created in the same transaction.
    pub ephemeral: bool,
}

impl<A> SingletonSpends<A>
where
    A: SingletonAsset,
{
    pub fn new(asset: A, ephemeral: bool) -> Self {
        Self {
            lineage: vec![SingletonSpend::new(asset)],
            ephemeral,
        }
    }

    /// The latest spend in the lineage.
    pub fn last(&self) -> Result<&SingletonSpend<A>, DriverError> {
        self.lineage.last().ok_or(DriverError::NoSourceForOutput)
    }

    /// The latest spend in the lineage, for an action to modify.
    ///
    /// Returns [`DriverError::NoSourceForOutput`] if the singleton has already been recreated or
    /// melted (for example, by locking it in an option), since the action would have no effect.
    pub fn last_mut(&mut self) -> Result<&mut SingletonSpend<A>, DriverError> {
        let last = self
            .lineage
            .last_mut()
            .ok_or(DriverError::NoSourceForOutput)?;

        if !last.kind.missing_singleton_output() {
            return Err(DriverError::NoSourceForOutput);
        }

        Ok(last)
    }

    /// The index of a settlement spend of the singleton, so that it can make notarized payments.
    /// If the latest spend isn't a settlement spend, the singleton is sent to the settlement
    /// payments puzzle, and the child's spend is added to the lineage.
    pub fn last_or_create_settlement(
        &mut self,
        ctx: &mut SpendContext,
    ) -> Result<usize, DriverError> {
        let last = self
            .lineage
            .last_mut()
            .ok_or(DriverError::NoSourceForOutput)?;

        if !last.kind.missing_singleton_output() {
            return Err(DriverError::NoSourceForOutput);
        }

        if last.kind.is_settlement() {
            return Ok(self.lineage.len() - 1);
        }

        let Some(child) = A::finalize(
            ctx,
            last,
            SETTLEMENT_PAYMENT_HASH.into(),
            SETTLEMENT_PAYMENT_HASH.into(),
        )?
        else {
            return Err(DriverError::NoSourceForOutput);
        };

        self.lineage.push(child);

        Ok(self.lineage.len() - 1)
    }

    /// Recreates (or melts) the singleton from the last spend, adding spends to the lineage until
    /// every pending update has been applied. Returns the final singleton, or `None` if it was
    /// melted or already recreated.
    ///
    /// If an action didn't choose a destination, the singleton is sent to the change puzzle hash.
    pub fn finalize(
        &mut self,
        ctx: &mut SpendContext,
        intermediate_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Result<Option<A>, DriverError> {
        let asset = loop {
            let last = self
                .lineage
                .last_mut()
                .ok_or(DriverError::NoSourceForOutput)?;

            if !last.kind.missing_singleton_output() {
                break None;
            }

            let Some(child) = A::finalize(ctx, last, intermediate_puzzle_hash, change_puzzle_hash)?
            else {
                break None;
            };

            if A::needs_additional_spend(&child.child_info) {
                self.lineage.push(child);
            } else {
                break Some(child.asset);
            }
        };

        Ok(asset)
    }

    /// Creates an XCH coin with the intermediate puzzle hash from a spend in the lineage, which can
    /// emit conditions when no other spend can. Returns `None` if no spend can create one.
    pub fn intermediate_fungible_xch_spend(
        &mut self,
        ctx: &mut SpendContext,
        intermediate_puzzle_hash: Bytes32,
    ) -> Result<Option<FungibleSpend<Coin>>, DriverError> {
        let Some((index, amount)) = self.lineage.iter().enumerate().find_map(|(index, item)| {
            item.kind
                .find_amount(intermediate_puzzle_hash, &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Ok(None);
        };

        let source = &mut self.lineage[index];

        let hint = ctx.hint(intermediate_puzzle_hash)?;

        source.kind.create_intermediate_coin(
            source.asset.coin_id(),
            CreateCoin::new(intermediate_puzzle_hash, amount, hint),
        );

        let child = FungibleSpend::new(
            Coin::new(source.asset.coin_id(), intermediate_puzzle_hash, amount),
            true,
        );

        Ok(Some(child))
    }

    /// Finds a spend in the lineage that can create a launcher coin. Settlement spends are skipped,
    /// since the launcher's announcement must be asserted with a condition.
    pub fn launcher_source(&mut self) -> Result<(usize, u64), DriverError> {
        let Some((index, amount)) = self.lineage.iter().enumerate().find_map(|(index, item)| {
            if !item.kind.is_conditions() {
                return None;
            }

            item.kind
                .find_amount(SINGLETON_LAUNCHER_HASH.into(), &item.asset.constraints())
                .map(|amount| (index, amount))
        }) else {
            return Err(DriverError::NoSourceForOutput);
        };

        Ok((index, amount))
    }

    /// Like [`FungibleSpends::create_launcher`](crate::FungibleSpends::create_launcher), but from a
    /// spend of the singleton.
    pub fn create_launcher(
        &mut self,
        singleton_amount: u64,
    ) -> Result<(usize, Launcher), DriverError> {
        let (index, launcher_amount) = self.launcher_source()?;

        let parent_coin_id = self.lineage[index].asset.coin_id();
        let (create_coin, launcher) = Launcher::create_early(parent_coin_id, launcher_amount);

        self.lineage[index]
            .kind
            .create_intermediate_coin(parent_coin_id, create_coin);

        Ok((index, launcher.with_singleton_amount(singleton_amount)))
    }
}

/// A single spend of a singleton.
#[derive(Debug, Clone)]
pub struct SingletonSpend<A>
where
    A: SingletonAsset,
{
    pub asset: A,
    /// What the singleton's p2 puzzle will output, which depends on whether it's a settlement coin.
    pub kind: SpendKind,
    /// The changes that the actions have made to the singleton's child, which are applied when the
    /// spend is finalized.
    pub child_info: A::ChildInfo,
    /// Assertions for the payments made by this spend, if it's a settlement spend.
    pub payment_assertions: Vec<AssertPuzzleAnnouncement>,
}

impl<A> SingletonSpend<A>
where
    A: SingletonAsset,
{
    /// A spend of the singleton with its p2 puzzle, with no changes to its child yet.
    pub fn new(asset: A) -> Self {
        let kind = if asset.p2_puzzle_hash() == SETTLEMENT_PAYMENT_HASH.into() {
            SpendKind::settlement()
        } else {
            SpendKind::conditions()
        };
        let child_info = A::default_child_info(&asset, &kind);

        Self {
            asset,
            kind,
            child_info,
            payment_assertions: Vec::new(),
        }
    }
}

impl SingletonSpend<Nft> {
    /// The child that a settlement spend of the NFT is moved to, so that the child can emit the
    /// transfer and metadata update conditions that the settlement spend can't.
    pub fn intermediate_child(&self, intermediate_puzzle_hash: Bytes32) -> Nft {
        let info = NftInfo {
            p2_puzzle_hash: intermediate_puzzle_hash,
            ..self.asset.info
        };

        self.asset.child_with(info, self.asset.coin.amount)
    }

    /// The puzzle hash of the NFT coin that will emit the pending transfer condition.
    pub fn transfer_puzzle_hash(&self, intermediate_puzzle_hash: Bytes32) -> Bytes32 {
        if self.kind.is_conditions() {
            self.asset.coin.puzzle_hash
        } else {
            self.intermediate_child(intermediate_puzzle_hash)
                .coin
                .puzzle_hash
        }
    }
}

/// A singleton that can be spent by the action system.
pub trait SingletonAsset: Debug + Clone + Asset {
    /// The changes that actions can make to the singleton's child.
    type ChildInfo: Debug + Clone;

    /// The child info for a spend that leaves the singleton unchanged.
    fn default_child_info(asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo;

    /// Whether the child has pending changes that require it to be spent again.
    fn needs_additional_spend(child_info: &Self::ChildInfo) -> bool;

    /// Adds the conditions that recreate (or melt) the singleton to the spend, and returns the
    /// spend of the child, or `None` if the singleton was melted.
    fn finalize(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        intermediate_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Result<Option<SingletonSpend<Self>>, DriverError>;
}

impl SingletonAsset for Did {
    type ChildInfo = ChildDidInfo;

    fn default_child_info(asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo {
        ChildDidInfo {
            recovery_list_hash: asset.info.recovery_list_hash,
            num_verifications_required: asset.info.num_verifications_required,
            metadata: asset.info.metadata,
            destination: None,
            new_spend_kind: spend_kind.empty_copy(),
            needs_update: false,
        }
    }

    fn needs_additional_spend(child_info: &Self::ChildInfo) -> bool {
        child_info.needs_update
    }

    fn finalize(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        _conditions_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Result<Option<SingletonSpend<Self>>, DriverError> {
        let change_hint = ctx.hint(change_puzzle_hash)?;

        let current_info = singleton.asset.info;
        let child_info = &singleton.child_info;

        // If the DID layer has changed, the DID is recreated with its current p2 puzzle hash first,
        // so that wallets can sync the update before it's sent elsewhere.
        let needs_update = current_info.recovery_list_hash != child_info.recovery_list_hash
            || current_info.num_verifications_required != child_info.num_verifications_required
            || current_info.metadata != child_info.metadata;

        let final_destination = child_info.destination;

        let destination = if needs_update {
            let p2_puzzle_hash = current_info.p2_puzzle_hash;
            let hint = ctx.hint(p2_puzzle_hash)?;
            SingletonDestination::CreateCoin(CreateCoin::new(
                p2_puzzle_hash,
                singleton.asset.coin.amount,
                hint,
            ))
        } else {
            child_info
                .destination
                .unwrap_or(SingletonDestination::CreateCoin(CreateCoin::new(
                    change_puzzle_hash,
                    singleton.asset.coin.amount,
                    change_hint,
                )))
        };

        match destination {
            SingletonDestination::CreateCoin(destination) => {
                let child_info = DidInfo::new(
                    current_info.launcher_id,
                    child_info.recovery_list_hash,
                    child_info.num_verifications_required,
                    child_info.metadata,
                    destination.puzzle_hash,
                );

                // Create the new DID coin with the updated DID info. The DID puzzle does not automatically wrap the output.
                let create_coin = CreateCoin::new(
                    child_info.inner_puzzle_hash().into(),
                    destination.amount,
                    destination.memos,
                );
                let parent_coin = singleton.asset.coin;
                singleton.kind.create_coin_with_assertion(
                    ctx,
                    parent_coin,
                    &mut singleton.payment_assertions,
                    create_coin,
                );

                // This is only added to the lineage if an additional spend is required.
                let mut new_spend = SingletonSpend::new(
                    singleton
                        .asset
                        .child_with(child_info, singleton.asset.coin.amount),
                );

                new_spend.child_info.needs_update = needs_update;

                if needs_update {
                    new_spend.child_info.destination = final_destination;
                }

                Ok(Some(new_spend))
            }
            SingletonDestination::Melt => {
                match &mut singleton.kind {
                    SpendKind::Conditions(conditions) => {
                        conditions.add_conditions(Conditions::new().melt_singleton());
                    }
                    SpendKind::Settlement(_) => {
                        return Err(DriverError::CannotEmitConditions);
                    }
                }

                Ok(None)
            }
        }
    }
}

impl SingletonAsset for Nft {
    type ChildInfo = ChildNftInfo;

    fn default_child_info(_asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo {
        ChildNftInfo {
            metadata_update_spends: Vec::new(),
            transfer_condition: None,
            destination: None,
            new_spend_kind: spend_kind.empty_copy(),
        }
    }

    fn needs_additional_spend(child_info: &Self::ChildInfo) -> bool {
        !child_info.metadata_update_spends.is_empty() || child_info.transfer_condition.is_some()
    }

    fn finalize(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        intermediate_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Result<Option<SingletonSpend<Self>>, DriverError> {
        if !singleton.kind.is_conditions()
            && (!singleton.child_info.metadata_update_spends.is_empty()
                || singleton.child_info.transfer_condition.is_some())
        {
            let create_coin = CreateCoin::new(
                intermediate_puzzle_hash,
                singleton.asset.coin.amount,
                ctx.hint(intermediate_puzzle_hash)?,
            );
            let parent_coin = singleton.asset.coin;
            singleton.kind.create_coin_with_assertion(
                ctx,
                parent_coin,
                &mut singleton.payment_assertions,
                create_coin,
            );

            let mut spend =
                SingletonSpend::new(singleton.intermediate_child(intermediate_puzzle_hash));

            spend.child_info = singleton.child_info.clone();

            return Ok(Some(spend));
        }

        let change_hint = ctx.hint(change_puzzle_hash)?;

        let mut new_child_info = singleton.child_info.clone();

        let metadata_update_spend = if new_child_info.metadata_update_spends.is_empty() {
            None
        } else {
            Some(new_child_info.metadata_update_spends.remove(0))
        };
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

        let parent_coin = singleton.asset.coin;

        singleton.kind.create_coin_with_assertion(
            ctx,
            parent_coin,
            &mut singleton.payment_assertions,
            destination,
        );

        let mut conditions = Conditions::new();

        if let Some(spend) = metadata_update_spend {
            conditions.push(UpdateNftMetadata::new(spend.puzzle, spend.solution));

            let metadata_info = run_metadata_updater(
                ctx,
                &singleton.asset.info.metadata,
                singleton.asset.info.metadata_updater_puzzle_hash,
                spend.puzzle,
                spend.solution,
            )?;

            nft_info.metadata = metadata_info.new_metadata;
            nft_info.metadata_updater_puzzle_hash = metadata_info.new_updater_puzzle_hash;
        }

        if let Some(transfer_condition) = transfer_condition {
            nft_info.current_owner = transfer_condition.launcher_id;
            conditions.push(transfer_condition);
        }

        if !conditions.is_empty() {
            match &mut singleton.kind {
                SpendKind::Conditions(spend) => {
                    spend.add_conditions(conditions);
                }
                SpendKind::Settlement(_) => {
                    return Err(DriverError::CannotEmitConditions);
                }
            }
        }

        let mut spend = SingletonSpend::new(
            singleton
                .asset
                .child_with(nft_info, singleton.asset.coin.amount),
        );

        spend.child_info = new_child_info;

        Ok(Some(spend))
    }
}

impl SingletonAsset for OptionContract {
    type ChildInfo = ChildOptionInfo;

    fn default_child_info(_asset: &Self, spend_kind: &SpendKind) -> Self::ChildInfo {
        ChildOptionInfo {
            destination: None,
            new_spend_kind: spend_kind.empty_copy(),
        }
    }

    fn needs_additional_spend(_child_info: &Self::ChildInfo) -> bool {
        false
    }

    fn finalize(
        ctx: &mut SpendContext,
        singleton: &mut SingletonSpend<Self>,
        _conditions_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Result<Option<SingletonSpend<Self>>, DriverError> {
        let change_hint = ctx.hint(change_puzzle_hash)?;

        let default_destination = SingletonDestination::CreateCoin(CreateCoin::new(
            change_puzzle_hash,
            singleton.asset.coin.amount,
            change_hint,
        ));

        let destination = singleton
            .child_info
            .destination
            .unwrap_or(default_destination);

        match destination {
            SingletonDestination::CreateCoin(destination) => {
                let parent_coin = singleton.asset.coin;
                singleton.kind.create_coin_with_assertion(
                    ctx,
                    parent_coin,
                    &mut singleton.payment_assertions,
                    destination,
                );

                Ok(Some(SingletonSpend::new(singleton.asset.child(
                    destination.puzzle_hash,
                    singleton.asset.coin.amount,
                ))))
            }
            SingletonDestination::Melt => {
                // Melting the option exercises it, which requires a message to the underlying coin.
                let message = singleton.asset.info.underlying_delegated_puzzle_hash.into();
                let data = ctx.alloc(&singleton.asset.info.underlying_coin_id)?;

                match &mut singleton.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(Conditions::new().melt_singleton().send_message(
                            23,
                            message,
                            vec![data],
                        ));
                    }
                    SpendKind::Settlement(_) => {
                        return Err(DriverError::CannotEmitConditions);
                    }
                }

                Ok(None)
            }
        }
    }
}

/// Where a singleton goes when it's spent.
#[derive(Debug, Clone, Copy)]
pub enum SingletonDestination {
    /// Recreates the singleton with the inner puzzle hash and amount of the condition.
    CreateCoin(CreateCoin<NodePtr>),
    /// Melts the singleton, which returns its amount to the transaction as XCH.
    Melt,
}

/// The changes that actions have made to a DID's child.
#[derive(Debug, Clone)]
pub struct ChildDidInfo {
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: HashedPtr,
    /// Where the DID is sent, or `None` to send it to the change puzzle hash.
    pub destination: Option<SingletonDestination>,
    /// Not read by the action system. The child's spend kind is determined by its p2 puzzle hash.
    pub new_spend_kind: SpendKind,
    /// Whether the child is an update spend, which must be spent again to reach the destination.
    pub needs_update: bool,
}

/// The changes that actions have made to an NFT's child.
#[derive(Debug, Clone)]
pub struct ChildNftInfo {
    /// Metadata updater spends that haven't been applied yet. Each spend of the NFT applies one,
    /// in order.
    pub metadata_update_spends: Vec<Spend>,
    /// A transfer to (or away from) a DID that hasn't been applied yet.
    pub transfer_condition: Option<TransferNft>,
    /// Where the NFT is sent after every update is applied, or `None` to send it to the change
    /// puzzle hash.
    pub destination: Option<CreateCoin<NodePtr>>,
    /// Not read by the action system. The child's spend kind is determined by its p2 puzzle hash.
    pub new_spend_kind: SpendKind,
}

/// The changes that actions have made to an option contract's child.
#[derive(Debug, Clone)]
pub struct ChildOptionInfo {
    /// Where the option is sent (or [`SingletonDestination::Melt`] to exercise it), or `None` to
    /// send it to the change puzzle hash.
    pub destination: Option<SingletonDestination>,
    /// Not read by the action system. The child's spend kind is determined by its p2 puzzle hash.
    pub new_spend_kind: SpendKind,
}
