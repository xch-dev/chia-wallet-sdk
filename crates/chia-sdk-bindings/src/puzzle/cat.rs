use bindy::Result;
use chia_protocol::Bytes32;
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::{Cat, CatInfo, CatSpend as SdkCatSpend};
use chia_sdk_types::puzzles::FeeTradePrice;
use clvm_utils::TreeHash;

use crate::{Program, Puzzle, Spend};

pub trait CatExt {
    fn child_lineage_proof(&self) -> Result<LineageProof>;
    fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Result<Cat>;
    fn unrevocable_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Result<Cat>;
}

impl CatExt for Cat {
    fn child_lineage_proof(&self) -> Result<LineageProof> {
        Ok(self.child_lineage_proof())
    }

    fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Result<Cat> {
        Ok(self.child(p2_puzzle_hash, amount))
    }

    fn unrevocable_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Result<Cat> {
        Ok(self.unrevocable_child(p2_puzzle_hash, amount))
    }
}

pub trait CatInfoExt {
    fn inner_puzzle_hash(&self) -> Result<TreeHash>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
}

impl CatInfoExt for CatInfo {
    fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.inner_puzzle_hash())
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.puzzle_hash())
    }
}

#[derive(Clone)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
    pub hidden: bool,
    pub trade_nonce: Bytes32,
    pub trade_prices: Vec<FeeTradePrice>,
}

impl CatSpend {
    pub fn new(cat: Cat, spend: Spend) -> Result<Self> {
        Ok(Self {
            cat,
            spend,
            hidden: false,
            trade_nonce: Bytes32::default(),
            trade_prices: Vec::new(),
        })
    }

    pub fn revoke(cat: Cat, spend: Spend) -> Result<Self> {
        Ok(Self {
            cat,
            spend,
            hidden: true,
            trade_nonce: Bytes32::default(),
            trade_prices: Vec::new(),
        })
    }

    pub fn with_trade(
        mut self,
        trade_nonce: Bytes32,
        trade_prices: Vec<FeeTradePrice>,
    ) -> Result<Self> {
        self.trade_nonce = trade_nonce;
        self.trade_prices = trade_prices;
        Ok(self)
    }
}

impl From<CatSpend> for SdkCatSpend {
    fn from(value: CatSpend) -> Self {
        SdkCatSpend {
            cat: value.cat,
            spend: value.spend.into(),
            hidden: value.hidden,
            trade_nonce: value.trade_nonce,
            trade_prices: value.trade_prices,
        }
    }
}

#[derive(Clone)]
pub struct ParsedCatInfo {
    pub info: CatInfo,
    pub p2_puzzle: Option<Puzzle>,
}

#[derive(Clone)]
pub struct ParsedCat {
    pub cat: Cat,
    pub p2_puzzle: Puzzle,
    pub p2_solution: Program,
}
