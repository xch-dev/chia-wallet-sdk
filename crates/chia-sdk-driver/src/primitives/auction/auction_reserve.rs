use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;

use crate::{Cat, CatInfo};

/// The coin that holds the winning bid, which is locked by the auction. Bids can be placed in XCH,
/// or in a CAT (which may be revocable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuctionReserve {
    Xch(Coin),
    Cat(Cat),
}

impl AuctionReserve {
    pub fn coin(&self) -> Coin {
        match self {
            Self::Xch(coin) => *coin,
            Self::Cat(cat) => cat.coin,
        }
    }

    /// The inner puzzle hash of the reserve, which is the auction's lock puzzle hash.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::Xch(coin) => coin.puzzle_hash,
            Self::Cat(cat) => cat.info.p2_puzzle_hash,
        }
    }

    /// The same reserve with a different parent. The reserve finalizer takes the reserve's parent
    /// from the singleton's solution, so any coin with the reserve's puzzle hash and amount can be
    /// spent as the reserve.
    #[must_use]
    pub fn with_parent_coin_info(&self, parent_coin_info: Bytes32) -> Self {
        match self {
            Self::Xch(coin) => {
                Self::Xch(Coin::new(parent_coin_info, coin.puzzle_hash, coin.amount))
            }
            Self::Cat(cat) => Self::Cat(Cat {
                coin: Coin::new(parent_coin_info, cat.coin.puzzle_hash, cat.coin.amount),
                ..*cat
            }),
        }
    }

    /// The new reserve that is created when this reserve is spent.
    #[must_use]
    pub fn child(&self, amount: u64) -> Self {
        match self {
            Self::Xch(coin) => Self::Xch(Coin::new(coin.coin_id(), coin.puzzle_hash, amount)),
            Self::Cat(cat) => Self::Cat(cat.child(cat.info.p2_puzzle_hash, amount)),
        }
    }

    /// The full puzzle hash of the settlement payments puzzle for the reserve's asset, which the
    /// winning bid's NFT royalty is paid through.
    pub fn settlement_puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::Xch(_) => SETTLEMENT_PAYMENT_HASH.into(),
            Self::Cat(cat) => CatInfo::new(
                cat.info.asset_id,
                cat.info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash()
            .into(),
        }
    }
}
