use chia_protocol::{Bytes32, Coin};

use crate::{Cat, Did, HashedPtr, Nft, OptionContract};

#[derive(Debug, Clone, Copy)]
pub enum SpendableAsset {
    Xch(Coin),
    Cat(Cat),
    Did(Did<HashedPtr>),
    Nft(Nft<HashedPtr>),
    Option(OptionContract),
}

impl SpendableAsset {
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::Xch(coin) => coin.puzzle_hash,
            Self::Cat(cat) => cat.info.p2_puzzle_hash,
            Self::Did(did) => did.info.p2_puzzle_hash,
            Self::Nft(nft) => nft.info.p2_puzzle_hash,
            Self::Option(option) => option.info.p2_puzzle_hash,
        }
    }

    pub fn coin(&self) -> Coin {
        match self {
            Self::Xch(coin) => *coin,
            Self::Cat(cat) => cat.coin,
            Self::Did(did) => did.coin,
            Self::Nft(nft) => nft.coin,
            Self::Option(option) => option.coin,
        }
    }
}
