use bindy::Result;
use chia_protocol::Bytes32;
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::{Cat, CatInfo};
use clvm_utils::TreeHash;

use crate::{Puzzle, Spend};

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
}

impl From<CatSpend> for chia_sdk_driver::CatSpend {
    fn from(value: CatSpend) -> Self {
        chia_sdk_driver::CatSpend::new(value.cat, value.spend.into())
    }
}

#[derive(Clone)]
pub struct ParsedCat {
    pub info: CatInfo,
    pub p2_puzzle: Option<Puzzle>,
}
