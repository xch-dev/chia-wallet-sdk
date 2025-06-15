use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::nft::NftMetadata;
use chia_sdk_driver::{
    HashedPtr, Nft as SdkNft, NftInfo as SdkNftInfo, NftMint as SdkNftMint, SpendContext,
};
use chia_sdk_types::conditions;
use clvm_utils::TreeHash;
use clvmr::Allocator;

use crate::{AsProgram, AsPtr, Program, Proof, TransferNft};

use super::Puzzle;

#[derive(Clone)]
pub struct Nft {
    pub coin: Coin,
    pub proof: Proof,
    pub info: NftInfo,
}

impl Nft {
    pub fn child_proof(&self) -> Result<Proof> {
        let ctx = self.info.metadata.0.lock().unwrap();
        Ok(self.as_ptr(&ctx).child_lineage_proof().into())
    }

    pub fn child(
        &self,
        p2_puzzle_hash: Bytes32,
        current_owner: Option<Bytes32>,
        metadata: Program,
    ) -> Result<Self> {
        let ctx = metadata.0.lock().unwrap();
        Ok(self
            .as_ptr(&ctx)
            .child(
                p2_puzzle_hash,
                current_owner,
                metadata.as_ptr(&ctx),
                self.coin.amount,
            )
            .as_program(&metadata.0))
    }

    pub fn child_with(&self, info: NftInfo) -> Result<Self> {
        let ctx = self.info.metadata.0.lock().unwrap();
        Ok(self
            .as_ptr(&ctx)
            .child_with(info.as_ptr(&ctx), self.coin.amount)
            .as_program(&self.info.metadata.0))
    }
}

impl AsProgram for SdkNft<HashedPtr> {
    type AsProgram = Nft;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Nft {
            coin: self.coin,
            proof: self.proof.into(),
            info: self.info.as_program(clvm),
        }
    }
}

impl AsPtr for Nft {
    type AsPtr = SdkNft<HashedPtr>;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        SdkNft::new(
            self.coin,
            self.proof.clone().into(),
            self.info.as_ptr(allocator),
        )
    }
}

#[derive(Clone)]
pub struct NftInfo {
    pub launcher_id: Bytes32,
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub p2_puzzle_hash: Bytes32,
}

impl NftInfo {
    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        let ctx = self.metadata.0.lock().unwrap();
        Ok(self.as_ptr(&ctx).inner_puzzle_hash())
    }

    pub fn puzzle_hash(&self) -> Result<TreeHash> {
        let ctx = self.metadata.0.lock().unwrap();
        Ok(self.as_ptr(&ctx).puzzle_hash())
    }
}

impl AsProgram for SdkNftInfo<HashedPtr> {
    type AsProgram = NftInfo;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        NftInfo {
            launcher_id: self.launcher_id,
            metadata: self.metadata.as_program(clvm),
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_basis_points: self.royalty_basis_points,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }
}

impl AsPtr for NftInfo {
    type AsPtr = SdkNftInfo<HashedPtr>;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        SdkNftInfo {
            launcher_id: self.launcher_id,
            metadata: self.metadata.as_ptr(allocator),
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_basis_points: self.royalty_basis_points,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }
}

#[derive(Clone)]
pub struct ParsedNft {
    pub info: NftInfo,
    pub p2_puzzle: Puzzle,
}

pub trait NftMetadataExt {}

impl NftMetadataExt for NftMetadata {}

#[derive(Clone)]
pub struct NftMint {
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub p2_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub transfer_condition: Option<TransferNft>,
}

impl AsPtr for NftMint {
    type AsPtr = SdkNftMint<HashedPtr>;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        SdkNftMint {
            metadata: self.metadata.as_ptr(allocator),
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            p2_puzzle_hash: self.p2_puzzle_hash,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_basis_points: self.royalty_basis_points,
            transfer_condition: self.transfer_condition.as_ref().map(|cond| {
                conditions::TransferNft::new(
                    cond.launcher_id,
                    cond.trade_prices.clone(),
                    cond.singleton_inner_puzzle_hash,
                )
            }),
        }
    }
}

#[derive(Clone)]
pub struct MintedNfts {
    pub nfts: Vec<Nft>,
    pub parent_conditions: Vec<Program>,
}
