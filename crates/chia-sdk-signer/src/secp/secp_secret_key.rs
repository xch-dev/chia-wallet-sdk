use k256::ecdsa::SigningKey;

use crate::SignerError;

use super::{SecpPublicKey, SecpSignature};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecpSecretKey(SigningKey);

impl SecpSecretKey {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes().into()
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, SignerError> {
        Ok(Self(SigningKey::from_bytes((&bytes).into())?))
    }

    pub fn public_key(&self) -> SecpPublicKey {
        SecpPublicKey(*self.0.verifying_key())
    }

    pub fn sign_prehashed(&self, message_hash: [u8; 32]) -> Result<SecpSignature, SignerError> {
        Ok(SecpSignature(
            self.0.sign_prehash_recoverable(&message_hash)?.0,
        ))
    }
}
