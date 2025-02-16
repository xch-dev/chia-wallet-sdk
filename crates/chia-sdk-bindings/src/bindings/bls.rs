use chia_bls::{
    sign, DerivableKey, PublicKey as PublicKeyRs, SecretKey as SecretKeyRs,
    Signature as SignatureRs,
};
use chia_protocol::{Bytes, Bytes32, Bytes48, Bytes96};
use chia_puzzles::DeriveSynthetic;

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

    pub fn public_key(&self) -> Result<PublicKey> {
        Ok(PublicKey(self.0.public_key()))
    }

    pub fn sign(&self, message: Bytes) -> Result<Signature> {
        Ok(Signature(sign(&self.0, message)))
    }

    pub fn derive_unhardened(&self, index: u32) -> Result<Self> {
        Ok(Self(self.0.derive_unhardened(index)))
    }

    pub fn derive_hardened(&self, index: u32) -> Result<Self> {
        Ok(Self(self.0.derive_hardened(index)))
    }

    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self> {
        let mut result = self.0.clone();

        for index in path {
            result = result.derive_unhardened(index);
        }

        Ok(Self(result))
    }

    pub fn derive_hardened_path(&self, path: Vec<u32>) -> Result<Self> {
        let mut result = self.0.clone();

        for index in path {
            result = result.derive_hardened(index);
        }

        Ok(Self(result))
    }

    pub fn derive_synthetic(&self) -> Result<Self> {
        Ok(Self(self.0.derive_synthetic()))
    }

    pub fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self(
            self.0
                .derive_synthetic_hidden(&hidden_puzzle_hash.to_bytes()),
        ))
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct PublicKey(PublicKeyRs);

impl PublicKey {
    pub fn infinity() -> Result<Self> {
        Ok(Self(PublicKeyRs::default()))
    }

    pub fn aggregate(mut public_keys: Vec<Self>) -> Result<Self> {
        if public_keys.is_empty() {
            return Self::infinity();
        }

        let mut result = public_keys.remove(0).0;

        for pk in public_keys {
            result += &pk.0;
        }

        Ok(Self(result))
    }

    pub fn from_bytes(bytes: Bytes48) -> Result<Self> {
        Ok(Self(PublicKeyRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<Bytes48> {
        Ok(Bytes48::new(self.0.to_bytes()))
    }

    pub fn fingerprint(&self) -> Result<u32> {
        Ok(self.0.get_fingerprint())
    }

    pub fn is_infinity(&self) -> Result<bool> {
        Ok(self.0.is_inf())
    }

    pub fn is_valid(&self) -> Result<bool> {
        Ok(self.0.is_valid())
    }

    pub fn derive_unhardened(&self, index: u32) -> Result<Self> {
        Ok(Self(self.0.derive_unhardened(index)))
    }

    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self> {
        let mut result = self.0;

        for index in path {
            result = result.derive_unhardened(index);
        }

        Ok(Self(result))
    }

    pub fn derive_synthetic(&self) -> Result<Self> {
        Ok(Self(self.0.derive_synthetic()))
    }

    pub fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self(
            self.0
                .derive_synthetic_hidden(&hidden_puzzle_hash.to_bytes()),
        ))
    }
}

#[derive(Clone)]
pub struct Signature(SignatureRs);

impl Signature {
    pub fn infinity() -> Result<Self> {
        Ok(Self(SignatureRs::default()))
    }

    pub fn aggregate(mut signatures: Vec<Self>) -> Result<Self> {
        if signatures.is_empty() {
            return Self::infinity();
        }

        let mut result = signatures.remove(0).0;

        for sig in signatures {
            result += &sig.0;
        }

        Ok(Self(result))
    }

    pub fn from_bytes(bytes: Bytes96) -> Result<Self> {
        Ok(Self(SignatureRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<Bytes96> {
        Ok(Bytes96::new(self.0.to_bytes()))
    }

    pub fn is_infinity(&self) -> Result<bool> {
        Ok(self.0 == SignatureRs::default())
    }

    pub fn is_valid(&self) -> Result<bool> {
        Ok(self.0.is_valid())
    }
}
