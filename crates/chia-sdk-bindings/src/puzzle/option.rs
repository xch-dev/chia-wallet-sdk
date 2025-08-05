use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{OptionContract as SdkOptionContract, OptionInfo, SingletonInfo};
use clvm_utils::TreeHash;

use crate::{Program, Proof};

use super::Puzzle;

#[derive(Clone)]
pub struct OptionContract {
    pub coin: Coin,
    pub proof: Proof,
    pub info: OptionInfo,
}

pub trait OptionContractExt: Sized {
    fn child_proof(&self) -> Result<Proof>;
    fn child(&self, p2_puzzle_hash: Bytes32) -> Result<Self>;
    fn child_with(&self, info: OptionInfo) -> Result<Self>;
}

impl OptionContractExt for OptionContract {
    fn child_proof(&self) -> Result<Proof> {
        Ok(SdkOptionContract::from(self.clone())
            .child_lineage_proof()
            .into())
    }

    fn child(&self, p2_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(SdkOptionContract::from(self.clone())
            .child(p2_puzzle_hash, self.coin.amount)
            .into())
    }

    fn child_with(&self, info: OptionInfo) -> Result<Self> {
        Ok(SdkOptionContract::from(self.clone())
            .child_with(info, self.coin.amount)
            .into())
    }
}

impl From<SdkOptionContract> for OptionContract {
    fn from(value: SdkOptionContract) -> Self {
        OptionContract {
            coin: value.coin,
            proof: value.proof.into(),
            info: value.info,
        }
    }
}

impl From<OptionContract> for SdkOptionContract {
    fn from(value: OptionContract) -> Self {
        SdkOptionContract {
            coin: value.coin,
            proof: value.proof.into(),
            info: value.info,
        }
    }
}

pub trait OptionInfoExt {
    fn inner_puzzle_hash(&self) -> Result<TreeHash>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
}

impl OptionInfoExt for OptionInfo {
    fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::inner_puzzle_hash(self))
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::puzzle_hash(self))
    }
}

#[derive(Clone)]
pub struct ParsedOptionInfo {
    pub info: OptionInfo,
    pub p2_puzzle: Puzzle,
}

#[derive(Clone)]
pub struct ParsedOption {
    pub option: OptionContract,
    pub p2_puzzle: Puzzle,
    pub p2_solution: Program,
}

#[derive(Clone)]
pub enum OptionType {
    Xch(OptionTypeXch),
    Cat(OptionTypeCat),
    RevocableCat(OptionTypeRevocableCat),
    Nft(OptionTypeNft),
}

#[derive(Clone)]
pub struct OptionTypeXch {
    pub amount: u64,
}

#[derive(Clone)]
pub struct OptionTypeCat {
    pub asset_id: Bytes32,
    pub amount: u64,
}

#[derive(Clone)]
pub struct OptionTypeRevocableCat {
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Bytes32,
    pub amount: u64,
}

#[derive(Clone)]
pub struct OptionTypeNft {
    pub launcher_id: Bytes32,
    pub settlement_puzzle_hash: Bytes32,
    pub amount: u64,
}
