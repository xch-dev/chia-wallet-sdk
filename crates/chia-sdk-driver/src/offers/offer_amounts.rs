use std::ops::Add;

use chia_protocol::Bytes32;
use indexmap::IndexMap;

use crate::DriverError;

#[derive(Debug, Default, Clone)]
pub struct Arbitrage {
    pub offered: ArbitrageSide,
    pub requested: ArbitrageSide,
}

impl Arbitrage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default, Clone)]
pub struct ArbitrageSide {
    pub xch: u128,
    pub cats: IndexMap<Bytes32, u128>,
    pub nfts: Vec<Bytes32>,
    pub options: Vec<Bytes32>,
}

impl ArbitrageSide {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn amounts(&self) -> OfferAmounts {
        OfferAmounts {
            xch: self.xch,
            cats: self.cats.clone(),
        }
    }
}

/// Aggregate amounts of each fungible asset in an offer.
///
/// These are [`u128`] because a total of many coin amounts can exceed [`u64::MAX`], especially in
/// untrusted offers. Only amounts which end up in a single coin or condition need to fit in a
/// [`u64`], and those are converted with [`coin_amount`].
#[derive(Debug, Default, Clone)]
pub struct OfferAmounts {
    pub xch: u128,
    pub cats: IndexMap<Bytes32, u128>,
}

impl OfferAmounts {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Add for &OfferAmounts {
    type Output = OfferAmounts;

    fn add(self, other: Self) -> Self::Output {
        let mut cats = self.cats.clone();

        for (&asset_id, amount) in &other.cats {
            *cats.entry(asset_id).or_insert(0) += amount;
        }

        Self::Output {
            xch: self.xch + other.xch,
            cats,
        }
    }
}

/// Converts an aggregate amount into an amount that can be used in a coin or condition.
pub fn coin_amount(amount: u128) -> Result<u64, DriverError> {
    u64::try_from(amount).map_err(|_| DriverError::AmountOverflow)
}
