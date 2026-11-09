use chia_protocol::Bytes32;
use chia_puzzle_types::{CoinProof, LineageProof};
use clvm_traits::{FromClvm, ToClvm};

/// Lineage proof encoded as `(parent_parent_coin_info parent_inner_puzzle_hash . parent_amount)`.
///
/// This is the same data as [`LineageProof`], but the last field is the tail of the list
/// instead of a separate element, which makes the encoding slightly smaller. Action
/// layer puzzles use this encoding, and it converts to and from [`LineageProof`] with
/// [`From`].
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CompactLineageProof {
    /// Parent coin id of the parent coin.
    pub parent_parent_coin_info: Bytes32,
    /// Inner puzzle hash of the parent coin.
    pub parent_inner_puzzle_hash: Bytes32,
    /// Amount of the parent coin.
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

/// Coin proof encoded as `(parent_coin_info inner_puzzle_hash . amount)`.
///
/// This is the same data as [`CoinProof`], with the last field as the tail of the list.
/// It converts to and from [`CoinProof`] with [`From`].
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct CompactCoinProof {
    /// Parent coin id of the coin.
    pub parent_coin_info: Bytes32,
    /// Inner puzzle hash of the coin.
    pub inner_puzzle_hash: Bytes32,
    /// Amount of the coin.
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
