use chia_bls::SecretKey as SecretKeyRs;
use chia_protocol::{Bytes, Bytes32};

use crate::Result;

#[derive(Debug)]
pub struct SecretKey(SecretKeyRs);

impl SecretKey {
    pub fn from_seed(seed: Bytes) -> Result<Self> {
        Ok(Self(SecretKeyRs::from_seed(&seed)))
    }

    pub fn from_bytes(bytes: Bytes32) -> Result<Self> {
        Ok(Self(SecretKeyRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<Bytes32> {
        Ok(Bytes32::new(self.0.to_bytes()))
    }
}
