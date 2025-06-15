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

pub trait SingletonAsset {}

impl SingletonAsset for Did<HashedPtr> {}

impl SingletonAsset for Nft<HashedPtr> {}

impl SingletonAsset for OptionContract {}
