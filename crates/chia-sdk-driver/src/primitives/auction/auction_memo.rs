use chia_protocol::Bytes32;
use chia_puzzle_types::LineageProof;
use clvm_traits::{FromClvm, ToClvm, apply_constants};

use crate::AuctionSettings;

/// Stored in the key value list of the auction's launcher solution. Along with the royalty info of
/// the NFT at `nft_coin_id`, which wallets fetch from the blockchain, it contains everything needed
/// to reconstruct the auction from its launcher spend.
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionMemo {
    #[clvm(constant = 0)]
    pub version: u8,
    pub settings: AuctionSettings,
    pub nft_coin_id: Bytes32,
    #[clvm(rest)]
    pub reserve: AuctionReserveMemo,
}

impl AuctionMemo {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
pub enum AuctionReserveMemo {
    Xch {
        parent_coin_info: Bytes32,
    } = 0,
    Cat {
        parent_coin_info: Bytes32,
        lineage_proof: Option<LineageProof>,
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
    } = 1,
}
