use std::ops::Add;

use chia_protocol::Bytes32;
use indexmap::IndexMap;

#[derive(Debug, Default, Clone)]
pub struct Arbitrage {
    pub offered: OfferAmounts,
    pub requested: OfferAmounts,
    pub offered_nfts: Vec<Bytes32>,
    pub requested_nfts: Vec<Bytes32>,
}

impl Arbitrage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default, Clone)]
pub struct OfferAmounts {
    pub xch: u64,
    pub cats: IndexMap<Bytes32, u64>,
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
