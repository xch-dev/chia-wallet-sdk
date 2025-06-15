use chia_protocol::Bytes32;

use crate::{FungibleAsset, SpendKind};

#[derive(Debug, Clone)]
pub struct Spendable<T> {
    pub asset: T,
    pub kind: SpendKind,
}

impl<T> Spendable<T> {
    pub fn new(asset: T, kind: SpendKind) -> Self {
        Self { asset, kind }
    }

    #[must_use]
    pub fn fungible_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self
    where
        T: FungibleAsset,
    {
        Self::new(
            self.asset.make_child(p2_puzzle_hash, amount),
            self.kind.child(),
        )
    }
}
