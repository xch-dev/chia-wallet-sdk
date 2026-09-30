use std::collections::HashMap;

use chia_protocol::Bytes32;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;

use crate::{CatInfo, DriverError, HashedPtr, Id};

#[derive(Debug, Default, Clone)]
pub struct AssetInfo {
    cats: HashMap<Bytes32, CatAssetInfo>,
    nfts: HashMap<Bytes32, NftAssetInfo>,
    options: HashMap<Bytes32, OptionAssetInfo>,
}

impl AssetInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cats(&self) -> impl Iterator<Item = &Bytes32> {
        self.cats.keys()
    }

    pub fn nfts(&self) -> impl Iterator<Item = &Bytes32> {
        self.nfts.keys()
    }

    pub fn options(&self) -> impl Iterator<Item = &Bytes32> {
        self.options.keys()
    }

    pub fn cat(&self, asset_id: Bytes32) -> Option<&CatAssetInfo> {
        self.cats.get(&asset_id)
    }

    pub fn nft(&self, launcher_id: Bytes32) -> Option<&NftAssetInfo> {
        self.nfts.get(&launcher_id)
    }

    pub fn option(&self, launcher_id: Bytes32) -> Option<&OptionAssetInfo> {
        self.options.get(&launcher_id)
    }

    /// The puzzle hash of settlement coins for XCH or a CAT.
    pub fn settlement_puzzle_hash(&self, asset: Id) -> Bytes32 {
        let Id::Existing(asset_id) = asset else {
            return SETTLEMENT_PAYMENT_HASH.into();
        };

        let hidden_puzzle_hash = self.cat(asset_id).and_then(|info| info.hidden_puzzle_hash);

        CatInfo::new(asset_id, hidden_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into())
            .puzzle_hash()
            .into()
    }

    /// The XCH or known CAT whose settlement coins have the given puzzle hash.
    pub fn settlement_asset(&self, settlement_puzzle_hash: Bytes32) -> Option<Id> {
        if settlement_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
            return Some(Id::Xch);
        }

        self.cats
            .keys()
            .map(|&asset_id| Id::Existing(asset_id))
            .find(|&asset| self.settlement_puzzle_hash(asset) == settlement_puzzle_hash)
    }

    pub fn insert_cat(&mut self, asset_id: Bytes32, info: CatAssetInfo) -> Result<(), DriverError> {
        if self
            .cats
            .insert(asset_id, info)
            .is_some_and(|existing| existing != info)
        {
            return Err(DriverError::IncompatibleAssetInfo);
        }

        Ok(())
    }

    pub fn insert_nft(
        &mut self,
        launcher_id: Bytes32,
        info: NftAssetInfo,
    ) -> Result<(), DriverError> {
        if self
            .nfts
            .insert(launcher_id, info)
            .is_some_and(|existing| existing != info)
        {
            return Err(DriverError::IncompatibleAssetInfo);
        }

        Ok(())
    }

    pub fn insert_option(
        &mut self,
        launcher_id: Bytes32,
        info: OptionAssetInfo,
    ) -> Result<(), DriverError> {
        if self
            .options
            .insert(launcher_id, info)
            .is_some_and(|existing| existing != info)
        {
            return Err(DriverError::IncompatibleAssetInfo);
        }

        Ok(())
    }

    pub fn extend(&mut self, other: Self) -> Result<(), DriverError> {
        for (asset_id, asset_info) in other.cats {
            self.insert_cat(asset_id, asset_info)?;
        }

        for (launcher_id, asset_info) in other.nfts {
            self.insert_nft(launcher_id, asset_info)?;
        }

        for (launcher_id, asset_info) in other.options {
            self.insert_option(launcher_id, asset_info)?;
        }

        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CatAssetInfo {
    pub hidden_puzzle_hash: Option<Bytes32>,
}

impl CatAssetInfo {
    pub fn new(hidden_puzzle_hash: Option<Bytes32>) -> Self {
        Self { hidden_puzzle_hash }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftAssetInfo {
    pub metadata: HashedPtr,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
}

impl NftAssetInfo {
    pub fn new(
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
    ) -> Self {
        Self {
            metadata,
            metadata_updater_puzzle_hash,
            royalty_puzzle_hash,
            royalty_basis_points,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionAssetInfo {
    pub underlying_coin_id: Bytes32,
    pub underlying_delegated_puzzle_hash: Bytes32,
}

impl OptionAssetInfo {
    pub fn new(underlying_coin_id: Bytes32, underlying_delegated_puzzle_hash: Bytes32) -> Self {
        Self {
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
        }
    }
}
