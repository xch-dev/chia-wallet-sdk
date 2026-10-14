use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::Condition;
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use indexmap::IndexMap;

use crate::{
    AddAsset, AssetInfo, Cat, CatAssetInfo, CostBudget, DriverError, Nft, NftAssetInfo,
    OfferAmounts, OptionAssetInfo, OptionContract, Outputs, Puzzle, Spends,
};

#[derive(Debug, Default, Clone)]
pub struct OfferCoins {
    pub xch: Vec<Coin>,
    pub cats: IndexMap<Bytes32, Vec<Cat>>,
    pub nfts: IndexMap<Bytes32, Nft>,
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
            xch: self.xch.iter().map(|c| u128::from(c.amount)).sum(),
            cats: self
                .cats
                .iter()
                .map(|(&asset_id, cats)| {
                    (
                        asset_id,
                        cats.iter().map(|c| u128::from(c.coin.amount)).sum(),
                    )
                })
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

        self.fee = self
            .fee
            .checked_add(other.fee)
            .ok_or(DriverError::FeeOverflow)?;

        Ok(())
    }

    /// Parses the coins sent to the settlement puzzle by a spend in an offer, and the fee it
    /// reserves.
    ///
    /// The spend is run once, with its cost deducted from the budget. Offered children are found by
    /// comparing the coins it creates to the puzzle hashes they would have if they were sent to the
    /// settlement puzzle, rather than running the inner puzzle again.
    #[allow(clippy::too_many_arguments)]
    pub fn parse(
        &mut self,
        allocator: &mut Allocator,
        asset_info: &mut AssetInfo,
        spent_coin_ids: &HashSet<Bytes32>,
        budget: &mut CostBudget,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<(), DriverError> {
        let (cost, output) = budget.run(allocator, parent_puzzle.ptr(), parent_solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut create_coins = Vec::new();

        for condition in conditions {
            if let Some(reserve_fee) = condition.as_reserve_fee() {
                self.fee = self
                    .fee
                    .checked_add(reserve_fee.amount)
                    .ok_or(DriverError::FeeOverflow)?;
            }

            if let Some(create_coin) = condition.into_create_coin() {
                create_coins.push(create_coin);
            }
        }

        let is_offered = |coin: Coin| !spent_coin_ids.contains(&coin.coin_id());

        for create_coin in &create_coins {
            let coin = Coin::new(
                parent_coin.coin_id(),
                create_coin.puzzle_hash,
                create_coin.amount,
            );

            if is_offered(coin) && coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                self.xch.push(coin);
            }
        }

        if let Some(parsed) = Cat::parse(allocator, parent_coin, parent_puzzle, parent_solution)? {
            let parent = parsed.cat;

            for create_coin in &create_coins {
                let child = parent.child(SETTLEMENT_PAYMENT_HASH.into(), create_coin.amount);
                let unrevocable_child =
                    parent.unrevocable_child(SETTLEMENT_PAYMENT_HASH.into(), create_coin.amount);

                // The children of a revoked CAT can be created with or without the revocation layer.
                let cat = if child.coin.puzzle_hash == create_coin.puzzle_hash {
                    child
                } else if parsed.revoked
                    && unrevocable_child.coin.puzzle_hash == create_coin.puzzle_hash
                {
                    unrevocable_child
                } else {
                    continue;
                };

                if !is_offered(cat.coin) {
                    continue;
                }

                asset_info.insert_cat(
                    cat.info.asset_id,
                    CatAssetInfo::new(cat.info.hidden_puzzle_hash),
                )?;

                self.cats.entry(cat.info.asset_id).or_default().push(cat);
            }
        } else if let Some((parent, _, _)) =
            Nft::parse(allocator, parent_coin, parent_puzzle, parent_solution)?
        {
            let Some(create_coin) = create_coins
                .iter()
                .find(|create_coin| create_coin.amount % 2 == 1)
            else {
                return Ok(());
            };

            // The NFT is usually sent to the settlement puzzle without updating its metadata, and
            // either keeps its owner or has it removed. Otherwise, the p2 spend has to be run again
            // to find the new metadata and owner, which costs no more than the spend did.
            let nft = [parent.info.current_owner, None]
                .into_iter()
                .map(|owner| {
                    parent.child(
                        SETTLEMENT_PAYMENT_HASH.into(),
                        owner,
                        parent.info.metadata,
                        create_coin.amount,
                    )
                })
                .find(|child| child.coin.puzzle_hash == create_coin.puzzle_hash);

            let nft = if let Some(nft) = nft {
                nft
            } else {
                budget.charge(cost)?;

                let Some(nft) =
                    Nft::parse_child(allocator, parent_coin, parent_puzzle, parent_solution)?
                else {
                    return Ok(());
                };

                nft
            };

            if is_offered(nft.coin) && nft.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
                asset_info.insert_nft(
                    nft.info.launcher_id,
                    NftAssetInfo::new(
                        nft.info.metadata,
                        nft.info.metadata_updater_puzzle_hash,
                        nft.info.royalty_puzzle_hash,
                        nft.info.royalty_basis_points,
                    ),
                )?;

                if self.nfts.insert(nft.info.launcher_id, nft).is_some() {
                    return Err(DriverError::ConflictingOfferInputs);
                }
            }
        } else if let Some((parent, _, _)) =
            OptionContract::parse(allocator, parent_coin, parent_puzzle, parent_solution)?
        {
            let option = create_coins
                .iter()
                .filter(|create_coin| create_coin.amount % 2 == 1)
                .map(|create_coin| {
                    (
                        create_coin,
                        parent.child(SETTLEMENT_PAYMENT_HASH.into(), create_coin.amount),
                    )
                })
                .find(|(create_coin, child)| child.coin.puzzle_hash == create_coin.puzzle_hash)
                .map(|(_, child)| child);

            if let Some(option) = option
                && is_offered(option.coin)
            {
                asset_info.insert_option(
                    option.info.launcher_id,
                    OptionAssetInfo::new(
                        option.info.underlying_coin_id,
                        option.info.underlying_delegated_puzzle_hash,
                    ),
                )?;

                if self
                    .options
                    .insert(option.info.launcher_id, option)
                    .is_some()
                {
                    return Err(DriverError::ConflictingOfferInputs);
                }
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
