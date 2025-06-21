use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{Did as SdkDid, DidInfo as SdkDidInfo, HashedPtr, SpendContext};
use clvm_utils::TreeHash;
use clvmr::Allocator;

use crate::{AsProgram, AsPtr, Program, Proof};

use super::Puzzle;

#[derive(Clone)]
pub struct Did {
    pub coin: Coin,
    pub proof: Proof,
    pub info: DidInfo,
}

impl Did {
    pub fn child_proof(&self) -> Result<Proof> {
        let ctx = self.info.metadata.0.lock().unwrap();
        Ok(self.as_ptr(&ctx).child_lineage_proof().into())
    }

    pub fn child(&self, p2_puzzle_hash: Bytes32, metadata: Program) -> Result<Self> {
        let ctx = metadata.0.lock().unwrap();
        Ok(self
            .as_ptr(&ctx)
            .child(p2_puzzle_hash, metadata.as_ptr(&ctx), self.coin.amount)
            .as_program(&metadata.0))
    }

    pub fn child_with(&self, info: DidInfo) -> Result<Self> {
        let ctx = self.info.metadata.0.lock().unwrap();
        Ok(self
            .as_ptr(&ctx)
            .child_with(info.as_ptr(&ctx), self.coin.amount)
            .as_program(&self.info.metadata.0))
    }
}

impl AsProgram for SdkDid<HashedPtr> {
    type AsProgram = Did;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Did {
            coin: self.coin,
            proof: self.proof.into(),
            info: self.info.as_program(clvm),
        }
    }
}

impl AsPtr for Did {
    type AsPtr = SdkDid<HashedPtr>;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        SdkDid::new(
            self.coin,
            self.proof.clone().into(),
            self.info.as_ptr(allocator),
        )
    }
}

#[derive(Clone)]
pub struct DidInfo {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: Program,
    pub p2_puzzle_hash: Bytes32,
}

impl DidInfo {
    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        let ctx = self.metadata.0.lock().unwrap();
        Ok(self.as_ptr(&ctx).inner_puzzle_hash())
    }

    pub fn puzzle_hash(&self) -> Result<TreeHash> {
        let ctx = self.metadata.0.lock().unwrap();
        Ok(self.as_ptr(&ctx).puzzle_hash())
    }
}

impl AsProgram for SdkDidInfo<HashedPtr> {
    type AsProgram = DidInfo;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        DidInfo {
            launcher_id: self.launcher_id,
            recovery_list_hash: self.recovery_list_hash,
            num_verifications_required: self.num_verifications_required,
            metadata: self.metadata.as_program(clvm),
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }
}

impl AsPtr for DidInfo {
    type AsPtr = SdkDidInfo<HashedPtr>;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        SdkDidInfo::new(
            self.launcher_id,
            self.recovery_list_hash,
            self.num_verifications_required,
            self.metadata.as_ptr(allocator),
            self.p2_puzzle_hash,
        )
    }
}

#[derive(Clone)]
pub struct ParsedDid {
    pub info: DidInfo,
    pub p2_puzzle: Puzzle,
}

#[derive(Clone)]
pub struct CreatedDid {
    pub did: Did,
    pub parent_conditions: Vec<Program>,
}
