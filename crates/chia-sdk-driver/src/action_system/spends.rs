use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_sdk_types::Condition;
use clvm_traits::FromClvm;
use indexmap::IndexMap;

use crate::{
    Action, Asset, Cat, CatSpend, Delta, Deltas, Did, DriverError, FungibleSpend, FungibleSpends,
    HashedPtr, Id, Layer, Nft, OptionContract, SettlementLayer, SingletonSpends, Spend,
    SpendAction, SpendContext, SpendKind, SpendWithConditions, StandardLayer,
};

#[derive(Debug, Clone)]
pub struct Spends {
    pub xch: FungibleSpends<Coin>,
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    pub dids: IndexMap<Id, SingletonSpends<Did<HashedPtr>>>,
    pub nfts: IndexMap<Id, SingletonSpends<Nft<HashedPtr>>>,
    pub options: IndexMap<Id, SingletonSpends<OptionContract>>,
    pub conditions_puzzle_hash: Bytes32,
}

#[derive(Debug, Default, Clone)]
pub struct Outputs {
    pub xch: Vec<Coin>,
    pub cats: IndexMap<Id, Vec<Cat>>,
    pub dids: IndexMap<Id, Did<HashedPtr>>,
    pub nfts: IndexMap<Id, Nft<HashedPtr>>,
    pub options: IndexMap<Id, OptionContract>,
}

impl Spends {
    pub fn new(conditions_puzzle_hash: Bytes32) -> Self {
        Self {
            xch: FungibleSpends::new(),
            cats: IndexMap::new(),
            dids: IndexMap::new(),
            nfts: IndexMap::new(),
            options: IndexMap::new(),
            conditions_puzzle_hash,
        }
    }

    pub fn add_xch(&mut self, coin: Coin, spend: SpendKind) {
        self.xch.items.push(FungibleSpend::new(coin, spend, false));
    }

    pub fn add_cat(&mut self, cat: Cat, spend: SpendKind) {
        self.cats
            .entry(Id::Existing(cat.info.asset_id))
            .or_default()
            .items
            .push(FungibleSpend::new(cat, spend, false));
    }

    pub fn add_did(&mut self, did: Did<HashedPtr>, spend: SpendKind) {
        self.dids.insert(
            Id::Existing(did.info.launcher_id),
            SingletonSpends::new(did, spend, false),
        );
    }

    pub fn add_nft(&mut self, nft: Nft<HashedPtr>, spend: SpendKind) {
        self.nfts.insert(
            Id::Existing(nft.info.launcher_id),
            SingletonSpends::new(nft, spend, false),
        );
    }

    pub fn add_option(&mut self, option: OptionContract, spend: SpendKind) {
        self.options.insert(
            Id::Existing(option.info.launcher_id),
            SingletonSpends::new(option, spend, false),
        );
    }

    pub fn resolve_first_cat(&self, id: Id) -> Result<Cat, DriverError> {
        Ok(self
            .cats
            .get(&id)
            .ok_or(DriverError::InvalidAssetId)?
            .items
            .first()
            .ok_or(DriverError::InvalidAssetId)?
            .asset)
    }

    pub fn resolve_did(&self, id: Id) -> Result<Did<HashedPtr>, DriverError> {
        Ok(self
            .dids
            .get(&id)
            .ok_or(DriverError::InvalidAssetId)?
            .lineage
            .last()
            .ok_or(DriverError::InvalidAssetId)?
            .asset)
    }

    pub fn resolve_nft(&self, id: Id) -> Result<Nft<HashedPtr>, DriverError> {
        Ok(self
            .nfts
            .get(&id)
            .ok_or(DriverError::InvalidAssetId)?
            .lineage
            .last()
            .ok_or(DriverError::InvalidAssetId)?
            .asset)
    }

    pub fn resolve_option(&self, id: Id) -> Result<OptionContract, DriverError> {
        Ok(self
            .options
            .get(&id)
            .ok_or(DriverError::InvalidAssetId)?
            .lineage
            .last()
            .ok_or(DriverError::InvalidAssetId)?
            .asset)
    }

    pub fn apply(
        &mut self,
        ctx: &mut SpendContext,
        actions: &[Action],
    ) -> Result<Deltas, DriverError> {
        let mut deltas = Deltas::new();

        for (index, action) in actions.iter().enumerate() {
            action.calculate_delta(&mut deltas, index);
            action.spend(ctx, self, index)?;
        }

        Ok(deltas)
    }

    pub fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        change_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        self.xch.create_change(
            ctx,
            deltas.get_xch().unwrap_or(&Delta::default()),
            change_puzzle_hash,
        )?;

        for (&id, cat) in &mut self.cats {
            cat.create_change(
                ctx,
                deltas.get(id).unwrap_or(&Delta::default()),
                change_puzzle_hash,
            )?;
        }

        for (_, did) in &mut self.dids {
            did.finalize(ctx, self.conditions_puzzle_hash, change_puzzle_hash)?;
        }

        for (_, nft) in &mut self.nfts {
            nft.finalize(ctx, self.conditions_puzzle_hash, change_puzzle_hash)?;
        }

        for (_, option) in &mut self.options {
            option.finalize(ctx, self.conditions_puzzle_hash, change_puzzle_hash)?;
        }

        Ok(())
    }

    pub fn p2_puzzle_hashes(&self) -> Vec<Bytes32> {
        let mut p2_puzzle_hashes = Vec::new();

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

    pub fn finish(
        self,
        ctx: &mut SpendContext,
        f: impl Fn(&mut SpendContext, Bytes32, SpendKind) -> Result<Spend, DriverError>,
    ) -> Result<Outputs, DriverError> {
        let mut outputs = Outputs::default();

        for item in self.xch.items {
            let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
            ctx.spend(item.asset, spend)?;

            let output = ctx.run(spend.puzzle, spend.solution)?;
            let conditions = Vec::<Condition>::from_clvm(ctx, output)?;

            for condition in conditions {
                if let Some(create_coin) = condition.into_create_coin() {
                    outputs.xch.push(Coin::new(
                        item.asset.coin_id(),
                        create_coin.puzzle_hash,
                        create_coin.amount,
                    ));
                }
            }
        }

        for (id, cat) in self.cats {
            let mut cat_spends = Vec::new();
            for item in cat.items {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                cat_spends.push(CatSpend::new(item.asset, spend));
            }
            let cats = Cat::spend_all(ctx, &cat_spends)?;
            if !cats.is_empty() {
                outputs.cats.insert(id, cats);
            }
        }

        for (id, did) in self.dids {
            for item in did.lineage {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                let did = item.asset.spend(ctx, spend)?;
                if let Some(did) = did {
                    outputs.dids.insert(id, did);
                } else {
                    outputs.dids.shift_remove(&id);
                }
            }
        }

        for (id, nft) in self.nfts {
            for item in nft.lineage {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                let nft = item.asset.spend(ctx, spend)?;
                outputs.nfts.insert(id, nft);
            }
        }

        for (id, option) in self.options {
            for item in option.lineage {
                let spend = f(ctx, item.asset.p2_puzzle_hash(), item.kind)?;
                let option = item.asset.spend(ctx, spend)?;
                if let Some(option) = option {
                    outputs.options.insert(id, option);
                } else {
                    outputs.options.shift_remove(&id);
                }
            }
        }

        Ok(outputs)
    }

    pub fn finish_with_keys(
        self,
        ctx: &mut SpendContext,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
    ) -> Result<Outputs, DriverError> {
        self.finish(ctx, |ctx, p2_puzzle_hash, kind| match kind {
            SpendKind::Conditions(spend) => {
                let Some(&synthetic_key) = synthetic_keys.get(&p2_puzzle_hash) else {
                    return Err(DriverError::MissingKey);
                };
                StandardLayer::new(synthetic_key).spend_with_conditions(ctx, spend.finish())
            }
            SpendKind::Settlement(spend) => SettlementLayer
                .construct_spend(ctx, SettlementPaymentsSolution::new(spend.finish())),
        })
    }
}
