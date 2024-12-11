use p256::ecdsa::SigningKey;

use crate::SignerError;

use super::{Secp256r1PublicKey, Secp256r1Signature};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secp256r1SecretKey(SigningKey);

impl Secp256r1SecretKey {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes().into()
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, SignerError> {
        Ok(Self(SigningKey::from_bytes((&bytes).into())?))
    }

    pub fn public_key(&self) -> Secp256r1PublicKey {
        Secp256r1PublicKey(*self.0.verifying_key())
    }

    pub fn sign_prehashed(
        &self,
        message_hash: [u8; 32],
    ) -> Result<Secp256r1Signature, SignerError> {
        Ok(Secp256r1Signature(
            self.0.sign_prehash_recoverable(&message_hash)?.0,
        ))
    }
}
