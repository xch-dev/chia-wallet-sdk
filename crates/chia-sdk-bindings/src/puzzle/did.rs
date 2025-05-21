use chia_protocol::{Bytes32, Coin};

use crate::{Program, Proof};

use super::Puzzle;

#[derive(Clone)]
pub struct Did {
    pub coin: Coin,
    pub lineage_proof: Proof,
    pub info: DidInfo,
}

#[derive(Clone)]
pub struct DidInfo {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: Program,
    pub p2_puzzle_hash: Bytes32,
}

impl From<chia_sdk_driver::Did<Program>> for Did {
    fn from(value: chia_sdk_driver::Did<Program>) -> Self {
        Self {
            coin: value.coin,
            lineage_proof: value.proof.into(),
            info: value.info.into(),
        }
    }
}

impl From<Did> for chia_sdk_driver::Did<Program> {
    fn from(value: Did) -> Self {
        Self {
            coin: value.coin,
            proof: value.lineage_proof.into(),
            info: value.info.into(),
        }
    }
}

impl From<chia_sdk_driver::DidInfo<Program>> for DidInfo {
    fn from(value: chia_sdk_driver::DidInfo<Program>) -> Self {
        Self {
            launcher_id: value.launcher_id,
            recovery_list_hash: value.recovery_list_hash,
            num_verifications_required: value.num_verifications_required,
            metadata: value.metadata,
            p2_puzzle_hash: value.p2_puzzle_hash,
        }
    }
}

impl From<DidInfo> for chia_sdk_driver::DidInfo<Program> {
    fn from(value: DidInfo) -> Self {
        Self {
            launcher_id: value.launcher_id,
            recovery_list_hash: value.recovery_list_hash,
            num_verifications_required: value.num_verifications_required,
            metadata: value.metadata,
            p2_puzzle_hash: value.p2_puzzle_hash,
        }
    }
}

#[derive(Clone)]
pub struct ParsedDid {
    pub info: DidInfo,
    pub p2_puzzle: Puzzle,
}
