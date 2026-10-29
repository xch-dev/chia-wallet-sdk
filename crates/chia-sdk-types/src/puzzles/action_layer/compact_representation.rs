use chia_protocol::Bytes32;
use chia_puzzle_types::{CoinProof, LineageProof};
use clvm_traits::{FromClvm, ToClvm};

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CompactLineageProof {
    pub parent_parent_coin_info: Bytes32,
    pub parent_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub parent_amount: u64,
}

impl From<LineageProof> for CompactLineageProof {
    fn from(value: LineageProof) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info,
            parent_inner_puzzle_hash: value.parent_inner_puzzle_hash,
            parent_amount: value.parent_amount,
        }
    }
}

impl From<CompactLineageProof> for LineageProof {
    fn from(value: CompactLineageProof) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info,
            parent_inner_puzzle_hash: value.parent_inner_puzzle_hash,
            parent_amount: value.parent_amount,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CompactCoinProof {
    pub parent_coin_info: Bytes32,
    pub inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub amount: u64,
}

impl From<CoinProof> for CompactCoinProof {
    fn from(value: CoinProof) -> Self {
        Self {
            parent_coin_info: value.parent_coin_info,
            inner_puzzle_hash: value.inner_puzzle_hash,
            amount: value.amount,
        }
    }
}

impl From<CompactCoinProof> for CoinProof {
    fn from(value: CompactCoinProof) -> Self {
        Self {
            parent_coin_info: value.parent_coin_info,
            inner_puzzle_hash: value.inner_puzzle_hash,
            amount: value.amount,
        }
    }
}
