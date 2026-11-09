use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm, apply_constants};

use crate::AuctionSettings;

/// Stored in the key value list of the auction's launcher solution. Along with the royalty info of
/// the NFT at `nft_coin_id`, which wallets fetch from the blockchain, it contains everything needed
/// to reconstruct the auction from its launcher spend.
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionMemo {
    /// The version of the memo format, which is always `0`.
    #[clvm(constant = 0)]
    pub version: u8,
    /// The settings of the auction.
    pub settings: AuctionSettings,
    /// The coin id of the locked NFT.
    pub nft_coin_id: Bytes32,
    /// The information needed to find the reserve coin.
    #[clvm(rest)]
    pub reserve: AuctionReserveMemo,
}

impl AuctionMemo {
    /// Creates a new [`AuctionMemo`] with the current version.
    pub fn new(
        settings: AuctionSettings,
        nft_coin_id: Bytes32,
        reserve: AuctionReserveMemo,
    ) -> Self {
        Self {
            settings,
            nft_coin_id,
            reserve,
        }
    }
}

/// The reserve is always an empty coin locked by the auction when it's launched, so only its parent
/// (and asset, for CATs) is needed to find it.
///
/// The lineage proof of a CAT reserve isn't stored, since it can't be trusted. Wallets get it from
/// the spend of the reserve's parent instead, which also checks that the reserve is a real coin of
/// the CAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
pub enum AuctionReserveMemo {
    /// An XCH reserve, which is encoded with the kind `0`.
    Xch {
        /// The parent coin id of the reserve.
        parent_coin_info: Bytes32,
    } = 0,
    /// A CAT reserve, which is encoded with the kind `1`.
    Cat {
        /// The parent coin id of the reserve.
        parent_coin_info: Bytes32,
        /// The asset id of the CAT.
        asset_id: Bytes32,
        /// The hidden puzzle hash of the CAT, if it's revocable.
        hidden_puzzle_hash: Option<Bytes32>,
    } = 1,
}

impl AuctionReserveMemo {
    /// The parent coin id of the reserve.
    pub fn parent_coin_info(&self) -> Bytes32 {
        match *self {
            Self::Xch { parent_coin_info }
            | Self::Cat {
                parent_coin_info, ..
            } => parent_coin_info,
        }
    }

    /// The asset id of the reserve, if it's a CAT.
    pub fn asset_id(&self) -> Option<Bytes32> {
        match *self {
            Self::Xch { .. } => None,
            Self::Cat { asset_id, .. } => Some(asset_id),
        }
    }
}
