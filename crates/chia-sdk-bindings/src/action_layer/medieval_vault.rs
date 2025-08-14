use bindy::Result;
use chia_sdk_driver::{MedievalVaultHint, MedievalVaultInfo, SingletonInfo};
use clvm_utils::TreeHash;

pub trait MedievalVaultHintExt {}

impl MedievalVaultHintExt for MedievalVaultHint {}

pub trait MedievalVaultInfoExt
where
    Self: Sized,
{
    fn inner_puzzle_hash(&self) -> Result<TreeHash>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
    fn from_hint(hint: MedievalVaultHint) -> Result<Self>;
    fn to_hint(&self) -> Result<MedievalVaultHint>;
}

impl MedievalVaultInfoExt for MedievalVaultInfo {
    fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::inner_puzzle_hash(self))
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::puzzle_hash(self))
    }

    fn from_hint(hint: MedievalVaultHint) -> Result<Self> {
        Ok(Self::from_hint(hint))
    }

    fn to_hint(&self) -> Result<MedievalVaultHint> {
        Ok(self.to_hint())
    }
}
