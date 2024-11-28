use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError};
use clvmr::Atom;
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use k256::ecdsa::VerifyingKey;

use crate::SignerError;

use super::SecpSignature;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecpPublicKey(pub(crate) VerifyingKey);

impl SecpPublicKey {
    pub const SIZE: usize = 33;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        self.0.to_sec1_bytes().as_ref().try_into().unwrap()
    }

    pub fn from_bytes(bytes: [u8; Self::SIZE]) -> Result<Self, SignerError> {
        Ok(Self(VerifyingKey::from_sec1_bytes(&bytes)?))
    }

    pub fn verify_prehashed(&self, message_hash: [u8; 32], signature: SecpSignature) -> bool {
        self.0.verify_prehash(&message_hash, &signature.0).is_ok()
    }
}

impl<E> ToClvm<E> for SecpPublicKey
where
    E: ClvmEncoder,
{
    fn to_clvm(&self, encoder: &mut E) -> Result<E::Node, ToClvmError> {
        encoder.encode_atom(Atom::Borrowed(&self.to_bytes()))
    }
}

impl<D> FromClvm<D> for SecpPublicKey
where
    D: ClvmDecoder,
{
    fn from_clvm(decoder: &D, node: D::Node) -> Result<Self, FromClvmError> {
        let atom = decoder.decode_atom(&node)?;
        let bytes: [u8; Self::SIZE] =
            atom.as_ref()
                .try_into()
                .map_err(|_| FromClvmError::WrongAtomLength {
                    expected: Self::SIZE,
                    found: atom.len(),
                })?;
        Self::from_bytes(bytes).map_err(|error| FromClvmError::Custom(error.to_string()))
    }
}
