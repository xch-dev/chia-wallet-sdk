use std::ops::Add;

use chia_protocol::Bytes32;
use indexmap::IndexMap;

use crate::DriverError;

/// The difference between what an offer offers and what it requests, for each asset.
///
/// This is calculated by [`Offer::arbitrage`](crate::Offer::arbitrage), from the perspective of
/// whoever takes the offer. If the offer requests more of an asset than it offers, the difference
/// is in [`Arbitrage::offered`], since the taker must provide it. Otherwise, the difference is in
/// [`Arbitrage::requested`], since it's left over for the taker.
#[derive(Debug, Default, Clone)]
pub struct Arbitrage {
    /// What the taker must provide, beyond what the offer offers.
    pub offered: ArbitrageSide,
    /// What is left over for the taker, after the requested payments are made.
    pub requested: ArbitrageSide,
}

impl Arbitrage {
    /// Creates an empty [`Arbitrage`].
    pub fn new() -> Self {
        Self::default()
    }
}

/// One side of an [`Arbitrage`].
#[derive(Debug, Default, Clone)]
pub struct ArbitrageSide {
    /// The amount of XCH.
    pub xch: u128,
    /// The amount of each CAT, keyed by asset id.
    pub cats: IndexMap<Bytes32, u128>,
    /// The launcher ids of the NFTs.
    pub nfts: Vec<Bytes32>,
    /// The launcher ids of the option contracts.
    pub options: Vec<Bytes32>,
}

impl ArbitrageSide {
    /// Creates an empty [`ArbitrageSide`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The amounts of the fungible assets on this side.
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OfferAmounts {
    /// The amount of XCH.
    pub xch: u128,
    /// The amount of each CAT, keyed by asset id.
    pub cats: IndexMap<Bytes32, u128>,
}

impl OfferAmounts {
    /// Creates an empty [`OfferAmounts`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an amount of a CAT to its total, or of XCH if `asset_id` is `None`.
    pub fn add_amount(&mut self, asset_id: Option<Bytes32>, amount: u128) {
        let total = match asset_id {
            Some(asset_id) => self.cats.entry(asset_id).or_default(),
            None => &mut self.xch,
        };

        *total += amount;
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
///
/// Returns [`DriverError::AmountOverflow`] if the amount doesn't fit in a [`u64`].
pub fn coin_amount(amount: u128) -> Result<u64, DriverError> {
    u64::try_from(amount).map_err(|_| DriverError::AmountOverflow)
}
