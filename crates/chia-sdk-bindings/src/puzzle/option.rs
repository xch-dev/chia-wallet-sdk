use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{
    OptionContract as SdkOptionContract, OptionInfo, OptionMetadata, OptionType, OptionUnderlying,
    SingletonInfo,
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{Clvm, Program, Proof, Spend};

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

pub trait OptionUnderlyingExt: Sized {
    fn exercise_spend(
        &self,
        clvm: Clvm,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
    ) -> Result<Spend>;
    fn clawback_spend(&self, spend: Spend) -> Result<Spend>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
    fn delegated_puzzle_hash(&self) -> Result<TreeHash>;
}

impl OptionUnderlyingExt for OptionUnderlying {
    fn exercise_spend(
        &self,
        clvm: Clvm,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
    ) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let spend = self.exercise_spend(&mut ctx, singleton_inner_puzzle_hash, singleton_amount)?;
        Ok(Spend {
            puzzle: Program(clvm.0.clone(), spend.puzzle),
            solution: Program(clvm.0.clone(), spend.solution),
        })
    }

    fn clawback_spend(&self, spend: Spend) -> Result<Spend> {
        let ctx_clone = spend.puzzle.0.clone();
        let mut ctx = ctx_clone.lock().unwrap();
        let spend = self.clawback_spend(&mut ctx, spend.into())?;
        Ok(Spend {
            puzzle: Program(ctx_clone.clone(), spend.puzzle),
            solution: Program(ctx_clone.clone(), spend.solution),
        })
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.tree_hash())
    }

    fn delegated_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.delegated_puzzle().tree_hash())
    }
}

pub trait OptionTypeExt: Sized {
    fn xch(amount: u64) -> Result<Self>;
    fn cat(asset_id: Bytes32, amount: u64) -> Result<Self>;
    fn revocable_cat(asset_id: Bytes32, hidden_puzzle_hash: Bytes32, amount: u64) -> Result<Self>;
    fn nft(launcher_id: Bytes32, settlement_puzzle_hash: Bytes32, amount: u64) -> Result<Self>;

    fn to_xch(&self) -> Result<Option<OptionTypeXch>>;
    fn to_cat(&self) -> Result<Option<OptionTypeCat>>;
    fn to_revocable_cat(&self) -> Result<Option<OptionTypeRevocableCat>>;
    fn to_nft(&self) -> Result<Option<OptionTypeNft>>;
}

impl OptionTypeExt for OptionType {
    fn xch(amount: u64) -> Result<Self> {
        Ok(Self::Xch { amount })
    }

    fn cat(asset_id: Bytes32, amount: u64) -> Result<Self> {
        Ok(Self::Cat { asset_id, amount })
    }

    fn revocable_cat(asset_id: Bytes32, hidden_puzzle_hash: Bytes32, amount: u64) -> Result<Self> {
        Ok(Self::RevocableCat {
            asset_id,
            hidden_puzzle_hash,
            amount,
        })
    }

    fn nft(launcher_id: Bytes32, settlement_puzzle_hash: Bytes32, amount: u64) -> Result<Self> {
        Ok(Self::Nft {
            launcher_id,
            settlement_puzzle_hash,
            amount,
        })
    }

    fn to_xch(&self) -> Result<Option<OptionTypeXch>> {
        match *self {
            Self::Xch { amount } => Ok(Some(OptionTypeXch { amount })),
            _ => Ok(None),
        }
    }

    fn to_cat(&self) -> Result<Option<OptionTypeCat>> {
        match *self {
            Self::Cat { asset_id, amount } => Ok(Some(OptionTypeCat { asset_id, amount })),
            _ => Ok(None),
        }
    }

    fn to_revocable_cat(&self) -> Result<Option<OptionTypeRevocableCat>> {
        match *self {
            Self::RevocableCat {
                asset_id,
                hidden_puzzle_hash,
                amount,
            } => Ok(Some(OptionTypeRevocableCat {
                asset_id,
                hidden_puzzle_hash,
                amount,
            })),
            _ => Ok(None),
        }
    }

    fn to_nft(&self) -> Result<Option<OptionTypeNft>> {
        match *self {
            Self::Nft {
                launcher_id,
                settlement_puzzle_hash,
                amount,
            } => Ok(Some(OptionTypeNft {
                launcher_id,
                settlement_puzzle_hash,
                amount,
            })),
            _ => Ok(None),
        }
    }
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

pub trait OptionMetadataExt {}

impl OptionMetadataExt for OptionMetadata {}
