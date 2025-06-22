use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{run_puzzle, Condition};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use indexmap::IndexMap;

use crate::{
    AddAsset, AssetInfo, Cat, CatAssetInfo, DriverError, HashedPtr, Nft, NftAssetInfo,
    OfferAmounts, OptionAssetInfo, OptionContract, Outputs, Puzzle, Spends,
};

#[derive(Debug, Default, Clone)]
pub struct OfferCoins {
    pub xch: Vec<Coin>,
    pub cats: IndexMap<Bytes32, Vec<Cat>>,
    pub nfts: IndexMap<Bytes32, Nft<HashedPtr>>,
    pub options: IndexMap<Bytes32, OptionContract>,
    pub fee: u64,
}

impl OfferCoins {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_outputs(outputs: &Outputs) -> Self {
        let mut coins = Self::default();

        for coin in &outputs.xch {
            if coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                coins.xch.push(*coin);
            }
        }

        for cats in outputs.cats.values() {
            for cat in cats {
                if cat.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                    coins
                        .cats
                        .entry(cat.info.asset_id)
                        .or_insert_with(Vec::new)
                        .push(*cat);
                }
            }
        }

        for nft in outputs.nfts.values() {
            if nft.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                coins.nfts.insert(nft.info.launcher_id, *nft);
            }
        }

        for option in outputs.options.values() {
            if option.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                coins.options.insert(option.info.launcher_id, *option);
            }
        }

        coins.fee = outputs.fee;

        coins
    }

    pub fn amounts(&self) -> OfferAmounts {
        OfferAmounts {
            xch: self.xch.iter().map(|c| c.amount).sum(),
            cats: self
                .cats
                .iter()
                .map(|(&launcher_id, cats)| (launcher_id, cats.iter().map(|c| c.coin.amount).sum()))
                .collect(),
        }
    }

    pub fn flatten(&self) -> Vec<Coin> {
        let mut coins = self.xch.clone();

        for cats in self.cats.values() {
            for cat in cats {
                coins.push(cat.coin);
            }
        }

        for nft in self.nfts.values() {
            coins.push(nft.coin);
        }

        for option in self.options.values() {
            coins.push(option.coin);
        }

        coins
    }

    pub fn extend(&mut self, other: Self) -> Result<(), DriverError> {
        for coin in other.xch {
            if self.xch.iter().any(|c| c.coin_id() == coin.coin_id()) {
                return Err(DriverError::ConflictingOfferInputs);
            }

            self.xch.push(coin);
        }

        for (asset_id, cats) in other.cats {
            let existing = self.cats.entry(asset_id).or_default();

            for cat in cats {
                if existing
                    .iter()
                    .any(|c| c.coin.coin_id() == cat.coin.coin_id())
                {
                    return Err(DriverError::ConflictingOfferInputs);
                }

                existing.push(cat);
            }
        }

        for (launcher_id, nft) in other.nfts {
            if self.nfts.insert(launcher_id, nft).is_some() {
                return Err(DriverError::ConflictingOfferInputs);
            }
        }

        for (launcher_id, option) in other.options {
            if self.options.insert(launcher_id, option).is_some() {
                return Err(DriverError::ConflictingOfferInputs);
            }
        }

        self.fee += other.fee;

        Ok(())
    }

    pub fn parse(
        &mut self,
        allocator: &mut Allocator,
        asset_info: &mut AssetInfo,
        spent_coin_ids: &HashSet<Bytes32>,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<(), DriverError> {
        if let Some(cats) =
            Cat::parse_children(allocator, parent_coin, parent_puzzle, parent_solution)?
        {
            for cat in cats {
                if !spent_coin_ids.contains(&cat.coin.coin_id())
                    && cat.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
                {
                    self.cats.entry(cat.info.asset_id).or_default().push(cat);
                }

                let info = CatAssetInfo::new(cat.info.hidden_puzzle_hash);
                asset_info.insert_cat(cat.info.asset_id, info)?;
            }
        }

        if let Some(nft) =
            Nft::<HashedPtr>::parse_child(allocator, parent_coin, parent_puzzle, parent_solution)?
        {
            if !spent_coin_ids.contains(&nft.coin.coin_id())
                && nft.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
            {
                self.nfts.insert(nft.info.launcher_id, nft);

                let info = NftAssetInfo::new(
                    nft.info.metadata,
                    nft.info.metadata_updater_puzzle_hash,
                    nft.info.royalty_puzzle_hash,
                    nft.info.royalty_basis_points,
                );
                asset_info.insert_nft(nft.info.launcher_id, info)?;
            }
        }

        if let Some(option) =
            OptionContract::parse_child(allocator, parent_coin, parent_puzzle, parent_solution)?
        {
            if !spent_coin_ids.contains(&option.coin.coin_id())
                && option.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
            {
                self.options.insert(option.info.launcher_id, option);

                let info = OptionAssetInfo::new(
                    option.info.underlying_coin_id,
                    option.info.underlying_delegated_puzzle_hash,
                );
                asset_info.insert_option(option.info.launcher_id, info)?;
            }
        }

        let output = run_puzzle(allocator, parent_puzzle.ptr(), parent_solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        for condition in conditions {
            if let Some(reserve_fee) = condition.as_reserve_fee() {
                self.fee += reserve_fee.amount;
            }

            let Some(create_coin) = condition.into_create_coin() else {
                continue;
            };

            let coin = Coin::new(
                parent_coin.coin_id(),
                create_coin.puzzle_hash,
                create_coin.amount,
            );

            if !spent_coin_ids.contains(&coin.coin_id())
                && coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
            {
                self.xch.push(coin);
            }
        }

        Ok(())
    }
}

impl AddAsset for OfferCoins {
    fn add(self, spends: &mut Spends) {
        for coin in self.xch {
            spends.add(coin);
        }

        for cats in self.cats.into_values() {
            for cat in cats {
                spends.add(cat);
            }
        }

        for nft in self.nfts.into_values() {
            spends.add(nft);
        }

        for option in self.options.into_values() {
            spends.add(option);
        }
    }
}
