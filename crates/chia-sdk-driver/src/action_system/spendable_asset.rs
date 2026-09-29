use chia_protocol::{Bytes32, Coin};

use crate::{Cat, Did, Nft, OptionContract};

/// A coin that needs to be spent by the caller, as returned by [`Spends::unspent`](crate::Spends::unspent).
#[derive(Debug, Clone, Copy)]
pub enum SpendableAsset {
    /// An XCH coin.
    Xch(Coin),
    /// A CAT that is spent with its p2 puzzle.
    Cat(Cat),
    /// A revocable CAT that is spent with its hidden puzzle rather than its p2 puzzle.
    RevokedCat(Cat),
    /// A DID singleton.
    Did(Did),
    /// An NFT singleton.
    Nft(Nft),
    /// An option contract singleton.
    Option(OptionContract),
}

impl SpendableAsset {
    /// The puzzle hash of the puzzle that must be used to authorize the spend. For a
    /// [`SpendableAsset::RevokedCat`], this is the hidden puzzle hash.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::Xch(coin) => coin.puzzle_hash,
            Self::Cat(cat) => cat.info.p2_puzzle_hash,
            Self::RevokedCat(cat) => cat
                .info
                .hidden_puzzle_hash
                .unwrap_or(cat.info.p2_puzzle_hash),
            Self::Did(did) => did.info.p2_puzzle_hash,
            Self::Nft(nft) => nft.info.p2_puzzle_hash,
            Self::Option(option) => option.info.p2_puzzle_hash,
        }
    }

    /// The coin being spent, including its outer puzzle hash.
    pub fn coin(&self) -> Coin {
        match self {
            Self::Xch(coin) => *coin,
            Self::Cat(cat) | Self::RevokedCat(cat) => cat.coin,
            Self::Did(did) => did.coin,
            Self::Nft(nft) => nft.coin,
            Self::Option(option) => option.coin,
        }
    }
}
