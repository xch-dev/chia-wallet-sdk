use std::mem;

use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_sdk_types::{conditions::AssertPuzzleAnnouncement, Conditions};
use indexmap::IndexMap;

use crate::{
    Action, Asset, Cat, CatSpend, ConditionsSpend, Delta, Deltas, Did, DriverError, FungibleSpend,
    FungibleSpends, HashedPtr, Id, Layer, Nft, OptionContract, Relation, SettlementLayer,
    SingletonSpends, Spend, SpendAction, SpendContext, SpendKind, SpendWithConditions,
    StandardLayer,
};

#[derive(Debug, Clone)]
pub struct Spends {
    pub xch: FungibleSpends<Coin>,
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    pub dids: IndexMap<Id, SingletonSpends<Did<HashedPtr>>>,
    pub nfts: IndexMap<Id, SingletonSpends<Nft<HashedPtr>>>,
    pub options: IndexMap<Id, SingletonSpends<OptionContract>>,
    pub intermediate_puzzle_hash: Bytes32,
    pub change_puzzle_hash: Bytes32,
    pub outputs: Outputs,
    pub conditions: ConditionConfig,
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
    pub dids: IndexMap<Id, Did<HashedPtr>>,
    pub nfts: IndexMap<Id, Nft<HashedPtr>>,
    pub options: IndexMap<Id, OptionContract>,
    pub fee: u64,
}

impl Spends {
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

    fn emit_conditions(&mut self, ctx: &mut SpendContext) -> Result<(), DriverError> {
        let mut conditions = self.conditions.required.clone().extend(
            if self.conditions.disable_settlement_assertions {
                vec![]
            } else {
                self.payment_assertions()
            },
        );

        let required = !conditions.is_empty();

        conditions = conditions.extend(self.conditions.optional.clone());

        if self.outputs.fee > 0 {
            conditions = conditions.reserve_fee(self.outputs.fee);
        }

        for (_, spend) in self.iter_conditions_spends() {
            spend.add_conditions(mem::take(&mut conditions));
        }

        if conditions.is_empty() || !required {
            return Ok(());
        }

        if let Some(index) = self
            .xch
            .intermediate_conditions_source(ctx, self.intermediate_puzzle_hash)?
        {
            match &mut self.xch.items[index].kind {
                SpendKind::Conditions(spend) => {
                    spend.add_conditions(mem::take(&mut conditions));
                }
                SpendKind::Settlement(_) => {}
            }
        }

        for cat in self.cats.values_mut() {
            if let Some(index) =
                cat.intermediate_conditions_source(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut cat.items[index].kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
            }
        }

        for did in self.dids.values_mut() {
            if let Some(mut item) =
                did.intermediate_fungible_xch_spend(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut item.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
                self.xch.items.push(item);
            }
        }

        for nft in self.nfts.values_mut() {
            if let Some(mut item) =
                nft.intermediate_fungible_xch_spend(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut item.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
                self.xch.items.push(item);
            }
        }

        for option in self.options.values_mut() {
            if let Some(mut item) =
                option.intermediate_fungible_xch_spend(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut item.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
                self.xch.items.push(item);
            }
        }

        if conditions.is_empty() {
            Ok(())
        } else {
            Err(DriverError::CannotEmitConditions)
        }
    }

    fn emit_relation(&mut self, relation: Relation) {
        match relation {
            Relation::None => {}
            Relation::AssertConcurrent => {
                let coin_ids: Vec<Bytes32> = self
                    .iter_conditions_spends()
                    .map(|(coin, _)| coin.coin_id())
                    .collect();

                if coin_ids.len() <= 1 {
                    return;
                }

                self.iter_conditions_spends()
                    .enumerate()
                    .for_each(|(i, (_, spend))| {
                        spend.add_conditions(Conditions::new().assert_concurrent_spend(
                            if i == 0 {
                                coin_ids[coin_ids.len() - 1]
                            } else {
                                coin_ids[i - 1]
                            },
                        ));
                    });
            }
        }
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

    pub fn finish(
        mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        f: impl Fn(&mut SpendContext, Bytes32, SpendKind) -> Result<Spend, DriverError>,
    ) -> Result<Outputs, DriverError> {
        self.create_change(ctx, deltas)?;
        self.emit_conditions(ctx)?;
        self.emit_relation(relation);

        for item in self.xch.items {
            let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
            ctx.spend(item.asset, spend)?;
        }

        for cat in self.cats.into_values() {
            let mut cat_spends = Vec::new();
            for item in cat.items {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                cat_spends.push(CatSpend::new(item.asset, spend));
            }
            Cat::spend_all(ctx, &cat_spends)?;
        }

        for did in self.dids.into_values() {
            for item in did.lineage {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                item.asset.spend(ctx, spend)?;
            }
        }

        for nft in self.nfts.into_values() {
            for item in nft.lineage {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                let _nft = item.asset.spend(ctx, spend)?;
            }
        }

        for option in self.options.into_values() {
            for item in option.lineage {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                let _option = item.asset.spend(ctx, spend)?;
            }
        }

        Ok(self.outputs)
    }

    pub fn finish_with_keys(
        self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
    ) -> Result<Outputs, DriverError> {
        self.finish(
            ctx,
            deltas,
            relation,
            |ctx, p2_puzzle_hash, kind| match kind {
                SpendKind::Conditions(spend) => {
                    let Some(&synthetic_key) = synthetic_keys.get(&p2_puzzle_hash) else {
                        return Err(DriverError::MissingKey);
                    };
                    StandardLayer::new(synthetic_key).spend_with_conditions(ctx, spend.finish())
                }
                SpendKind::Settlement(spend) => SettlementLayer
                    .construct_spend(ctx, SettlementPaymentsSolution::new(spend.finish())),
            },
        )
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

impl AddAsset for Did<HashedPtr> {
    fn add(self, spends: &mut Spends) {
        spends.dids.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}

impl AddAsset for Nft<HashedPtr> {
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
