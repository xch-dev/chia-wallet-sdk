use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::Coin;
use chia_sdk_driver::{
    MedievalVault as SdkMedievalVault, MedievalVaultHint, MedievalVaultInfo, SingletonInfo,
};
use clvm_utils::TreeHash;

use crate::Proof;

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

#[derive(Clone)]
pub struct MedievalVault {
    pub coin: Coin,
    pub proof: Proof,
    pub info: MedievalVaultInfo,
}

impl MedievalVault {
    pub fn new(coin: Coin, proof: Proof, info: MedievalVaultInfo) -> Self {
        Self { coin, proof, info }
    }

    pub fn child(&self, new_m: usize, new_public_key_list: Vec<PublicKey>) -> Result<Self> {
        let new_sdk_vault =
            SdkMedievalVault::new(self.coin, self.proof.clone().into(), self.info.clone())
                .child(new_m, new_public_key_list);

        Ok(Self::new(
            new_sdk_vault.coin,
            new_sdk_vault.proof.into(),
            new_sdk_vault.info,
        ))
    }

    pub fn to_sdk(self) -> SdkMedievalVault {
        SdkMedievalVault::new(self.coin, self.proof.into(), self.info)
    }
}
