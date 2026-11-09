use chia_protocol::{Bytes32, Coin};

use crate::{Cat, CatInfo};

/// The asset held by a bag, which determines the outer puzzle that wraps each node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BagAsset {
    Xch,
    Cat {
        asset_id: Bytes32,
    },
    RevocableCat {
        asset_id: Bytes32,
        hidden_puzzle_hash: Bytes32,
    },
}

impl BagAsset {
    /// The full puzzle hash of a coin of this asset with the given p2 puzzle hash.
    pub fn puzzle_hash(&self, p2_puzzle_hash: Bytes32) -> Bytes32 {
        match self.cat_info(p2_puzzle_hash) {
            Some(info) => info.puzzle_hash().into(),
            None => p2_puzzle_hash,
        }
    }

    /// The [`CatInfo`] of a coin of this asset with the given p2 puzzle hash, or [`None`] for XCH.
    pub fn cat_info(&self, p2_puzzle_hash: Bytes32) -> Option<CatInfo> {
        match *self {
            Self::Xch => None,
            Self::Cat { asset_id } => Some(CatInfo::new(asset_id, None, p2_puzzle_hash)),
            Self::RevocableCat {
                asset_id,
                hidden_puzzle_hash,
            } => Some(CatInfo::new(
                asset_id,
                Some(hidden_puzzle_hash),
                p2_puzzle_hash,
            )),
        }
    }
}

/// A coin in a bag, either a node that can be spent to unroll the bag further or a payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagCoin {
    Xch(Coin),
    Cat(Cat),
}

impl BagCoin {
    pub fn coin(&self) -> Coin {
        match self {
            Self::Xch(coin) => *coin,
            Self::Cat(cat) => cat.coin,
        }
    }

    /// The puzzle hash inside of the asset's outer puzzle, which identifies the node or payment.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::Xch(coin) => coin.puzzle_hash,
            Self::Cat(cat) => cat.info.p2_puzzle_hash,
        }
    }

    /// The child coin that this coin creates with the given p2 puzzle hash and amount, if it's spent as a node.
    #[must_use]
    pub fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        match self {
            Self::Xch(coin) => Self::Xch(Coin::new(coin.coin_id(), p2_puzzle_hash, amount)),
            Self::Cat(cat) => Self::Cat(cat.child(p2_puzzle_hash, amount)),
        }
    }

    pub(crate) fn matches(&self, asset: BagAsset) -> bool {
        match self {
            Self::Xch(_) => asset == BagAsset::Xch,
            Self::Cat(cat) => asset.cat_info(cat.info.p2_puzzle_hash) == Some(cat.info),
        }
    }
}
