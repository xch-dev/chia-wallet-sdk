use std::collections::HashMap;

use chia_protocol::Bytes32;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;

use crate::{CatInfo, DriverError, HashedPtr};

/// The information needed to construct the puzzles of the non-XCH assets in an offer.
///
/// Requested payments only contain the asset id (or launcher id) of each asset, which isn't enough
/// to calculate the puzzle hash of its settlement coin. For example, a requested NFT's puzzle
/// depends on its metadata and royalty. This stores the rest of the information for CATs, NFTs,
/// and option contracts, keyed by asset id or launcher id.
#[derive(Debug, Default, Clone)]
pub struct AssetInfo {
    cats: HashMap<Bytes32, CatAssetInfo>,
    nfts: HashMap<Bytes32, NftAssetInfo>,
    options: HashMap<Bytes32, OptionAssetInfo>,
}

impl AssetInfo {
    /// Creates an empty [`AssetInfo`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Iterates over the asset ids of the CATs.
    pub fn cats(&self) -> impl Iterator<Item = &Bytes32> {
        self.cats.keys()
    }

    /// Iterates over the launcher ids of the NFTs.
    pub fn nfts(&self) -> impl Iterator<Item = &Bytes32> {
        self.nfts.keys()
    }

    /// Iterates over the launcher ids of the option contracts.
    pub fn options(&self) -> impl Iterator<Item = &Bytes32> {
        self.options.keys()
    }

    /// Returns the info of the CAT with the asset id, if there is any.
    pub fn cat(&self, asset_id: Bytes32) -> Option<&CatAssetInfo> {
        self.cats.get(&asset_id)
    }

    /// Returns the info of the NFT with the launcher id, if there is any.
    pub fn nft(&self, launcher_id: Bytes32) -> Option<&NftAssetInfo> {
        self.nfts.get(&launcher_id)
    }

    /// Returns the info of the option contract with the launcher id, if there is any.
    pub fn option(&self, launcher_id: Bytes32) -> Option<&OptionAssetInfo> {
        self.options.get(&launcher_id)
    }

    /// The puzzle hash of settlement coins for a CAT, or XCH if `asset_id` is `None`. Fails with
    /// [`DriverError::MissingAssetInfo`] for an unknown CAT.
    pub fn settlement_puzzle_hash(
        &self,
        asset_id: Option<Bytes32>,
    ) -> Result<Bytes32, DriverError> {
        let Some(asset_id) = asset_id else {
            return Ok(SETTLEMENT_PAYMENT_HASH.into());
        };

        let info = self.cat(asset_id).ok_or(DriverError::MissingAssetInfo)?;

        Ok(CatInfo::new(
            asset_id,
            info.hidden_puzzle_hash,
            SETTLEMENT_PAYMENT_HASH.into(),
        )
        .puzzle_hash()
        .into())
    }

    /// The asset id of the known CAT whose settlement coins have the given puzzle hash, or `None`
    /// for XCH. Fails with [`DriverError::UnknownSettlementAsset`] if it's neither.
    pub fn settlement_asset_id(
        &self,
        settlement_puzzle_hash: Bytes32,
    ) -> Result<Option<Bytes32>, DriverError> {
        if settlement_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
            return Ok(None);
        }

        self.cats
            .keys()
            .find(|&&asset_id| {
                self.settlement_puzzle_hash(Some(asset_id)).ok() == Some(settlement_puzzle_hash)
            })
            .map(|&asset_id| Some(asset_id))
            .ok_or(DriverError::UnknownSettlementAsset(settlement_puzzle_hash))
    }

    /// Adds the info of a CAT.
    ///
    /// Returns [`DriverError::IncompatibleAssetInfo`] if the CAT already has different info. The
    /// new info replaces the existing info either way.
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

    /// Adds the info of an NFT.
    ///
    /// Returns [`DriverError::IncompatibleAssetInfo`] if the NFT already has different info. The
    /// new info replaces the existing info either way.
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

    /// Adds the info of an option contract.
    ///
    /// Returns [`DriverError::IncompatibleAssetInfo`] if the option contract already has different
    /// info. The new info replaces the existing info either way.
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

    /// Adds all of the info from another [`AssetInfo`].
    ///
    /// Returns [`DriverError::IncompatibleAssetInfo`] if any asset has different info in each.
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

/// The information needed to construct the puzzle of a CAT, other than its asset id.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CatAssetInfo {
    /// The hidden puzzle hash of the revocation layer, if the CAT is revocable.
    pub hidden_puzzle_hash: Option<Bytes32>,
}

impl CatAssetInfo {
    /// Creates a [`CatAssetInfo`].
    pub fn new(hidden_puzzle_hash: Option<Bytes32>) -> Self {
        Self { hidden_puzzle_hash }
    }
}

/// The information needed to construct the puzzle of an NFT, other than its launcher id and
/// owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftAssetInfo {
    /// The metadata of the NFT.
    pub metadata: HashedPtr,
    /// The tree hash of the NFT's metadata updater puzzle.
    pub metadata_updater_puzzle_hash: Bytes32,
    /// The puzzle hash that receives royalties when the NFT is traded.
    pub royalty_puzzle_hash: Bytes32,
    /// The royalty percentage in basis points, so a value of 300 means 3%.
    pub royalty_basis_points: u16,
}

impl NftAssetInfo {
    /// Creates an [`NftAssetInfo`].
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

/// The information needed to construct the puzzle of an option contract, other than its launcher
/// id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionAssetInfo {
    /// The coin id of the underlying coin, which is claimed when the option is exercised.
    pub underlying_coin_id: Bytes32,
    /// The tree hash of the delegated puzzle that the underlying coin is spent with when the
    /// option is exercised.
    pub underlying_delegated_puzzle_hash: Bytes32,
}

impl OptionAssetInfo {
    /// Creates an [`OptionAssetInfo`].
    pub fn new(underlying_coin_id: Bytes32, underlying_delegated_puzzle_hash: Bytes32) -> Self {
        Self {
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
        }
    }
}
