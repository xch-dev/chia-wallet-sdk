use chia_protocol::Bytes32;
use chia_puzzle_types::cat::CatArgs;
use chia_sdk_types::{puzzles::RevocationArgs, Mod};
use clvm_utils::TreeHash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatInfo {
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Option<Bytes32>,
    pub p2_puzzle_hash: Bytes32,
}

impl CatInfo {
    pub fn new(
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            asset_id,
            hidden_puzzle_hash,
            p2_puzzle_hash,
        }
    }

    pub fn inner_puzzle_hash(&self) -> Bytes32 {
        let mut inner_puzzle_hash = TreeHash::from(self.p2_puzzle_hash);
        if let Some(hidden_puzzle_hash) = self.hidden_puzzle_hash {
            inner_puzzle_hash =
                RevocationArgs::new(hidden_puzzle_hash, inner_puzzle_hash.into()).curry_tree_hash();
        }
        inner_puzzle_hash.into()
    }

    pub fn puzzle_hash(&self) -> Bytes32 {
        CatArgs::curry_tree_hash(self.asset_id, self.inner_puzzle_hash().into()).into()
    }
}
