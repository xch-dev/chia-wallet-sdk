use chia_protocol::Bytes32;

use crate::{Did, HashedPtr, Nft, OptionContract, SpendKind, Spendable};

#[derive(Debug, Clone)]
pub struct SingletonSpends<A>
where
    A: SingletonAsset,
{
    pub lineage: Vec<Spendable<A>>,
}

impl<A> SingletonSpends<A>
where
    A: SingletonAsset,
{
    pub fn new(asset: A, spend: SpendKind) -> Self {
        Self {
            lineage: vec![Spendable::new(asset, spend)],
        }
    }
}

pub trait SingletonAsset {
    fn p2_puzzle_hash(&self) -> Bytes32;
}

impl SingletonAsset for Did<HashedPtr> {
    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }
}

impl SingletonAsset for Nft<HashedPtr> {
    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }
}

impl SingletonAsset for OptionContract {
    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }
}
