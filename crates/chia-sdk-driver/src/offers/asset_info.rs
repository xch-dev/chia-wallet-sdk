use std::collections::HashMap;

use chia_protocol::Bytes32;

use crate::{DriverError, HashedPtr, TransferFeePolicy};

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

    pub fn cat(&self, asset_id: Bytes32) -> Option<&CatAssetInfo> {
        self.cats.get(&asset_id)
    }

    pub fn nft(&self, launcher_id: Bytes32) -> Option<&NftAssetInfo> {
        self.nfts.get(&launcher_id)
    }

    pub fn option(&self, launcher_id: Bytes32) -> Option<&OptionAssetInfo> {
        self.options.get(&launcher_id)
    }

    pub fn insert_cat(&mut self, asset_id: Bytes32, info: CatAssetInfo) -> Result<(), DriverError> {
        if let Some(existing) = self.cats.get(&asset_id).copied() {
            let Some(merged) = existing.merge(info) else {
                return Err(DriverError::IncompatibleAssetInfo);
            };
            self.cats.insert(asset_id, merged);
            return Ok(());
        }

        self.cats.insert(asset_id, info);
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
    pub transfer_fee_policy: Option<TransferFeePolicy>,
    pub settlement_puzzle_hash: Option<Bytes32>,
}

impl CatAssetInfo {
    pub fn new(hidden_puzzle_hash: Option<Bytes32>, transfer_fee_policy: Option<TransferFeePolicy>) -> Self {
        Self {
            hidden_puzzle_hash,
            transfer_fee_policy,
            settlement_puzzle_hash: None,
        }
    }

    pub fn with_settlement_puzzle_hash(mut self, settlement_puzzle_hash: Option<Bytes32>) -> Self {
        self.settlement_puzzle_hash = settlement_puzzle_hash;
        self
    }

    fn merge(self, other: Self) -> Option<Self> {
        if self.hidden_puzzle_hash != other.hidden_puzzle_hash
            || self.transfer_fee_policy != other.transfer_fee_policy
        {
            return None;
        }

        let settlement_puzzle_hash =
            match (self.settlement_puzzle_hash, other.settlement_puzzle_hash) {
                (Some(a), Some(b)) if a != b => return None,
                (Some(a), _) => Some(a),
                (_, Some(b)) => Some(b),
                (None, None) => None,
            };

        Some(Self {
            hidden_puzzle_hash: self.hidden_puzzle_hash,
            transfer_fee_policy: self.transfer_fee_policy,
            settlement_puzzle_hash,
        })
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
