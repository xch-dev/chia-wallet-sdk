use std::{collections::HashMap, mem};

use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_sdk_types::{Conditions, announcement_id, conditions::AssertPuzzleAnnouncement};
use indexmap::IndexMap;

use crate::{
    Action, Asset, Cat, CatSpend, ConditionsSpend, Delta, Deltas, Did, DriverError, FungibleSpend,
    FungibleSpends, Id, Layer, Nft, OptionContract, Relation, SettlementLayer, SingletonSpends,
    Spend, SpendAction, SpendContext, SpendKind, SpendWithConditions, SpendableAsset,
    StandardLayer,
};

#[derive(Debug, Clone)]
#[must_use]
pub struct Spends<S = Unfinished> {
    pub xch: FungibleSpends<Coin>,
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    pub dids: IndexMap<Id, SingletonSpends<Did>>,
    pub nfts: IndexMap<Id, SingletonSpends<Nft>>,
    pub options: IndexMap<Id, SingletonSpends<OptionContract>>,
    pub intermediate_puzzle_hash: Bytes32,
    pub change_puzzle_hash: Bytes32,
    pub outputs: Outputs,
    pub conditions: ConditionConfig,
    _state: S,
}

#[derive(Debug, Default, Clone)]
pub struct ConditionConfig {
    pub optional: Conditions,
    pub required: Conditions,
    pub disable_settlement_assertions: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Outputs {
    pub xch: Vec<Coin>,
    pub cats: IndexMap<Id, Vec<Cat>>,
    pub dids: IndexMap<Id, Did>,
    pub nfts: IndexMap<Id, Nft>,
    pub options: IndexMap<Id, OptionContract>,
    pub fee: u64,
    pub reserved_fee: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unfinished;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Finished;

impl Spends<Unfinished> {
    pub fn new(change_puzzle_hash: Bytes32) -> Self {
        Self::with_separate_change_puzzle_hash(change_puzzle_hash, change_puzzle_hash)
    }

    pub fn with_separate_change_puzzle_hash(
        intermediate_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            xch: FungibleSpends::new(),
            cats: IndexMap::new(),
            dids: IndexMap::new(),
            nfts: IndexMap::new(),
            options: IndexMap::new(),
            intermediate_puzzle_hash,
            change_puzzle_hash,
            outputs: Outputs::default(),
            conditions: ConditionConfig::default(),
            _state: Unfinished,
        }
    }

    pub fn add(&mut self, asset: impl AddAsset) {
        asset.add(self);
    }

    pub fn apply(
        &mut self,
        ctx: &mut SpendContext,
        actions: &[Action],
    ) -> Result<Deltas, DriverError> {
        let deltas = Deltas::from_actions(actions);
        for (index, action) in actions.iter().enumerate() {
            action.spend(ctx, self, index)?;
        }
        Ok(deltas)
    }

    fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
    ) -> Result<(), DriverError> {
        if let Some(change) = self.xch.create_change(
            ctx,
            deltas.get(&Id::Xch).unwrap_or(&Delta::default()),
            self.change_puzzle_hash,
        )? {
            self.outputs.xch.push(change);
        }

        for (&id, cat) in &mut self.cats {
            if let Some(change) = cat.create_change(
                ctx,
                deltas.get(&id).unwrap_or(&Delta::default()),
                self.change_puzzle_hash,
            )? {
                self.outputs.cats.entry(id).or_default().push(change);
            }
        }

        for (&id, did) in &mut self.dids {
            if let Some(change) =
                did.finalize(ctx, self.intermediate_puzzle_hash, self.change_puzzle_hash)?
            {
                self.outputs.dids.insert(id, change);
            }
        }

        for (&id, nft) in &mut self.nfts {
            if let Some(change) =
                nft.finalize(ctx, self.intermediate_puzzle_hash, self.change_puzzle_hash)?
            {
                self.outputs.nfts.insert(id, change);
            }
        }

        for (&id, option) in &mut self.options {
            if let Some(change) =
                option.finalize(ctx, self.intermediate_puzzle_hash, self.change_puzzle_hash)?
            {
                self.outputs.options.insert(id, change);
            }
        }

        Ok(())
    }

    fn payment_assertions(&self) -> Vec<AssertPuzzleAnnouncement> {
        let mut payment_assertions = self.xch.payment_assertions.clone();

        for cat in self.cats.values() {
            payment_assertions.extend_from_slice(&cat.payment_assertions);
        }

        for did in self.dids.values() {
            for item in &did.lineage {
                payment_assertions.extend_from_slice(&item.payment_assertions);
            }
        }

        for nft in self.nfts.values() {
            for item in &nft.lineage {
                payment_assertions.extend_from_slice(&item.payment_assertions);
            }
        }

        for option in self.options.values() {
            for item in &option.lineage {
                payment_assertions.extend_from_slice(&item.payment_assertions);
            }
        }

        payment_assertions
    }

    fn iter_conditions_spends(&mut self) -> impl Iterator<Item = (Coin, &mut ConditionsSpend)> {
        self.xch
            .items
            .iter_mut()
            .filter_map(|item| {
                if let SpendKind::Conditions(spend) = &mut item.kind {
                    Some((item.asset, spend))
                } else {
                    None
                }
            })
            .chain(self.cats.values_mut().filter_map(|cat| {
                cat.items.iter_mut().find_map(|item| {
                    if let SpendKind::Conditions(spend) = &mut item.kind {
                        Some((item.asset.coin, spend))
                    } else {
                        None
                    }
                })
            }))
            .chain(self.dids.values_mut().filter_map(|did| {
                did.lineage
                    .iter_mut()
                    .filter_map(|item| {
                        if let SpendKind::Conditions(spend) = &mut item.kind {
                            Some((item.asset.coin, spend))
                        } else {
                            None
                        }
                    })
                    .last()
            }))
            .chain(self.nfts.values_mut().filter_map(|nft| {
                nft.lineage
                    .iter_mut()
                    .filter_map(|item| {
                        if let SpendKind::Conditions(spend) = &mut item.kind {
                            Some((item.asset.coin, spend))
                        } else {
                            None
                        }
                    })
                    .last()
            }))
            .chain(self.options.values_mut().filter_map(|option| {
                option
                    .lineage
                    .iter_mut()
                    .filter_map(|item| {
                        if let SpendKind::Conditions(spend) = &mut item.kind {
                            Some((item.asset.coin, spend))
                        } else {
                            None
                        }
                    })
                    .last()
            }))
    }

    /// Attaches the transaction-wide conditions (required and optional conditions, settlement
    /// payment assertions, and the reserved fee) to the first spend that can emit conditions.
    ///
    /// If there is no such spend (for example, when only settlement coins are being spent) and
    /// there are conditions that must be included, an ephemeral coin is created with the
    /// intermediate puzzle hash so that it can be spent to emit them.
    fn emit_conditions(&mut self, ctx: &mut SpendContext) -> Result<(), DriverError> {
        let mut conditions = self.conditions.required.clone().extend(
            if self.conditions.disable_settlement_assertions {
                vec![]
            } else {
                self.payment_assertions()
            },
        );

        let required = !conditions.is_empty() || self.outputs.reserved_fee > 0;

        conditions = conditions.extend(self.conditions.optional.clone());

        if self.outputs.reserved_fee > 0 {
            conditions = conditions.reserve_fee(self.outputs.reserved_fee);
        }

        if let Some((_, spend)) = self.iter_conditions_spends().next() {
            spend.add_conditions(conditions);
            return Ok(());
        }

        if conditions.is_empty() || !required {
            return Ok(());
        }

        let intermediate_puzzle_hash = self.intermediate_puzzle_hash;

        if let Some(index) = self
            .xch
            .intermediate_conditions_source(ctx, intermediate_puzzle_hash)?
            && try_add_conditions(&mut self.xch.items[index].kind, &mut conditions)
        {
            return Ok(());
        }

        for cat in self.cats.values_mut() {
            if let Some(index) =
                cat.intermediate_conditions_source(ctx, intermediate_puzzle_hash)?
                && try_add_conditions(&mut cat.items[index].kind, &mut conditions)
            {
                return Ok(());
            }
        }

        for did in self.dids.values_mut() {
            if let Some(mut item) =
                did.intermediate_fungible_xch_spend(ctx, intermediate_puzzle_hash)?
            {
                let emitted = try_add_conditions(&mut item.kind, &mut conditions);
                self.xch.items.push(item);
                if emitted {
                    return Ok(());
                }
            }
        }

        for nft in self.nfts.values_mut() {
            if let Some(mut item) =
                nft.intermediate_fungible_xch_spend(ctx, intermediate_puzzle_hash)?
            {
                let emitted = try_add_conditions(&mut item.kind, &mut conditions);
                self.xch.items.push(item);
                if emitted {
                    return Ok(());
                }
            }
        }

        for option in self.options.values_mut() {
            if let Some(mut item) =
                option.intermediate_fungible_xch_spend(ctx, intermediate_puzzle_hash)?
            {
                let emitted = try_add_conditions(&mut item.kind, &mut conditions);
                self.xch.items.push(item);
                if emitted {
                    return Ok(());
                }
            }
        }

        Err(DriverError::CannotEmitConditions)
    }

    fn emit_relation(&mut self, relation: Relation) {
        if relation == Relation::None {
            return;
        }

        let coin_ids: Vec<Bytes32> = self
            .iter_conditions_spends()
            .map(|(coin, _)| coin.coin_id())
            .collect();

        if coin_ids.len() <= 1 {
            return;
        }

        let len = coin_ids.len();

        self.iter_conditions_spends()
            .enumerate()
            .for_each(|(i, (_, spend))| {
                let conditions = match relation {
                    Relation::None => Conditions::new(),
                    Relation::AssertConcurrent => {
                        Conditions::new().assert_concurrent_spend(coin_ids[(i + len - 1) % len])
                    }
                    Relation::CoinAnnouncementRing => Conditions::new()
                        .create_coin_announcement(Bytes::default())
                        .assert_coin_announcement(announcement_id(
                            coin_ids[(i + 1) % len],
                            Bytes::default(),
                        )),
                    Relation::CoinAnnouncementHub => {
                        if i == 0 {
                            Conditions::new().create_coin_announcement(Bytes::default())
                        } else {
                            Conditions::new().assert_coin_announcement(announcement_id(
                                coin_ids[0],
                                Bytes::default(),
                            ))
                        }
                    }
                };

                spend.add_conditions(conditions);
            });
    }

    pub fn p2_puzzle_hashes(&self) -> Vec<Bytes32> {
        let mut p2_puzzle_hashes = vec![self.intermediate_puzzle_hash];

        for item in &self.xch.items {
            p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
        }

        for (_, cat) in &self.cats {
            for item in &cat.items {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        for (_, did) in &self.dids {
            for item in &did.lineage {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        for (_, nft) in &self.nfts {
            for item in &nft.lineage {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        for (_, option) in &self.options {
            for item in &option.lineage {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        p2_puzzle_hashes
    }

    pub fn non_settlement_coin_ids(&self) -> Vec<Bytes32> {
        let mut coin_ids = Vec::new();

        for item in &self.xch.items {
            if item.kind.is_conditions() {
                coin_ids.push(item.asset.coin_id());
            }
        }

        for (_, cat) in &self.cats {
            for item in &cat.items {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        for (_, did) in &self.dids {
            for item in &did.lineage {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        for (_, nft) in &self.nfts {
            for item in &nft.lineage {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        for (_, option) in &self.options {
            for item in &option.lineage {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        coin_ids
    }

    pub fn prepare(
        mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
    ) -> Result<Spends<Finished>, DriverError> {
        self.create_change(ctx, deltas)?;
        self.emit_conditions(ctx)?;
        self.emit_relation(relation);

        Ok(Spends {
            xch: self.xch,
            cats: self.cats,
            dids: self.dids,
            nfts: self.nfts,
            options: self.options,
            intermediate_puzzle_hash: self.intermediate_puzzle_hash,
            change_puzzle_hash: self.change_puzzle_hash,
            outputs: self.outputs,
            conditions: self.conditions,
            _state: Finished,
        })
    }

    pub fn finish_with_keys(
        self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
    ) -> Result<Outputs, DriverError> {
        let spends = self.prepare(ctx, deltas, relation)?;
        let mut coin_spends = HashMap::new();

        for (asset, kind) in spends.unspent() {
            match kind {
                SpendKind::Conditions(spend) => {
                    let Some(&synthetic_key) = synthetic_keys.get(&asset.p2_puzzle_hash()) else {
                        return Err(DriverError::MissingKey);
                    };
                    coin_spends.insert(
                        asset.coin().coin_id(),
                        StandardLayer::new(synthetic_key)
                            .spend_with_conditions(ctx, spend.finish())?,
                    );
                }
                SpendKind::Settlement(spend) => {
                    coin_spends.insert(
                        asset.coin().coin_id(),
                        SettlementLayer.construct_spend(
                            ctx,
                            SettlementPaymentsSolution::new(spend.finish()),
                        )?,
                    );
                }
            }
        }

        spends.spend(ctx, coin_spends)
    }
}

impl Spends<Finished> {
    pub fn unspent(&self) -> Vec<(SpendableAsset, SpendKind)> {
        let mut result = Vec::new();

        for item in &self.xch.items {
            result.push((SpendableAsset::Xch(item.asset), item.kind.clone()));
        }

        for cat in self.cats.values() {
            for item in &cat.items {
                result.push((SpendableAsset::Cat(item.asset), item.kind.clone()));
            }
        }

        for did in self.dids.values() {
            for item in &did.lineage {
                result.push((SpendableAsset::Did(item.asset), item.kind.clone()));
            }
        }

        for nft in self.nfts.values() {
            for item in &nft.lineage {
                result.push((SpendableAsset::Nft(item.asset), item.kind.clone()));
            }
        }

        for option in self.options.values() {
            for item in &option.lineage {
                result.push((SpendableAsset::Option(item.asset), item.kind.clone()));
            }
        }

        result
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        mut coin_spends: HashMap<Bytes32, Spend>,
    ) -> Result<Outputs, DriverError> {
        for item in self.xch.items {
            let spend = coin_spends
                .remove(&item.asset.coin_id())
                .ok_or(DriverError::MissingSpend)?;
            ctx.spend(item.asset, spend)?;
        }

        for cat in self.cats.into_values() {
            let mut cat_spends = Vec::new();
            for item in cat.items {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                cat_spends.push(CatSpend::new(item.asset, spend));
            }
            Cat::spend_all(ctx, &cat_spends)?;
        }

        for did in self.dids.into_values() {
            for item in did.lineage {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                item.asset.spend(ctx, spend)?;
            }
        }

        for nft in self.nfts.into_values() {
            for item in nft.lineage {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                let _nft = item.asset.spend(ctx, spend)?;
            }
        }

        for option in self.options.into_values() {
            for item in option.lineage {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                let _option = item.asset.spend(ctx, spend)?;
            }
        }

        Ok(self.outputs)
    }
}

fn try_add_conditions(kind: &mut SpendKind, conditions: &mut Conditions) -> bool {
    match kind {
        SpendKind::Conditions(spend) => {
            spend.add_conditions(mem::take(conditions));
            true
        }
        SpendKind::Settlement(_) => false,
    }
}

pub trait AddAsset {
    fn add(self, spends: &mut Spends);
}

impl AddAsset for Coin {
    fn add(self, spends: &mut Spends) {
        spends.xch.items.push(FungibleSpend::new(self, false));
    }
}

impl AddAsset for Cat {
    fn add(self, spends: &mut Spends) {
        spends
            .cats
            .entry(Id::Existing(self.info.asset_id))
            .or_default()
            .items
            .push(FungibleSpend::new(self, false));
    }
}

impl AddAsset for Did {
    fn add(self, spends: &mut Spends) {
        spends.dids.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}

impl AddAsset for Nft {
    fn add(self, spends: &mut Spends) {
        spends.nfts.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}

impl AddAsset for OptionContract {
    fn add(self, spends: &mut Spends) {
        spends.options.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}
