use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoRust};

#[napi]
pub struct SecretKey(chia_sdk_bindings::SecretKey);

#[napi]
impl SecretKey {
    #[napi(factory)]
    pub fn from_seed(seed: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::SecretKey::from_seed(seed.rust()?)?))
    }

    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[napi]
    pub fn public_key(&self) -> Result<PublicKey> {
        Ok(PublicKey(self.0.public_key()?))
    }

    #[napi]
    pub fn sign(&self, message: Uint8Array) -> Result<Signature> {
        Ok(Signature(self.0.sign(message.rust()?)?))
    }

    #[napi]
    pub fn derive_unhardened(&self, index: u32) -> Result<Self> {
        Ok(Self(self.0.derive_unhardened(index)?))
    }

    #[napi]
    pub fn derive_hardened(&self, index: u32) -> Result<Self> {
        Ok(Self(self.0.derive_hardened(index)?))
    }

    #[napi]
    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self> {
        Ok(Self(self.0.derive_unhardened_path(path)?))
    }

    #[napi]
    pub fn derive_hardened_path(&self, path: Vec<u32>) -> Result<Self> {
        Ok(Self(self.0.derive_hardened_path(path)?))
    }

    #[napi]
    pub fn derive_synthetic(&self) -> Result<Self> {
        Ok(Self(self.0.derive_synthetic()?))
    }

    #[napi]
    pub fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Uint8Array) -> Result<Self> {
        Ok(Self(
            self.0.derive_synthetic_hidden(hidden_puzzle_hash.rust()?)?,
        ))
    }
}

#[napi]
pub struct PublicKey(chia_sdk_bindings::PublicKey);

#[napi]
impl PublicKey {
    #[napi(factory)]
    pub fn infinity() -> Result<Self> {
        Ok(Self(chia_sdk_bindings::PublicKey::infinity()?))
    }

    #[napi]
    pub fn aggregate(public_keys: Vec<Reference<PublicKey>>) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::PublicKey::aggregate(
            public_keys.into_iter().map(|pk| pk.0).collect(),
        )?))
    }

    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[napi]
    pub fn fingerprint(&self) -> Result<u32> {
        Ok(self.0.fingerprint()?)
    }

    #[napi]
    pub fn is_infinity(&self) -> Result<bool> {
        Ok(self.0.is_infinity()?)
    }

    #[napi]
    pub fn is_valid(&self) -> Result<bool> {
        Ok(self.0.is_valid()?)
    }

    #[napi]
    pub fn derive_unhardened(&self, index: u32) -> Result<Self> {
        Ok(Self(self.0.derive_unhardened(index)?))
    }

    #[napi]
    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self> {
        Ok(Self(self.0.derive_unhardened_path(path)?))
    }

    #[napi]
    pub fn derive_synthetic(&self) -> Result<Self> {
        Ok(Self(self.0.derive_synthetic()?))
    }

    #[napi]
    pub fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Uint8Array) -> Result<Self> {
        Ok(Self(
            self.0.derive_synthetic_hidden(hidden_puzzle_hash.rust()?)?,
        ))
    }
}

#[napi]
pub struct Signature(chia_sdk_bindings::Signature);

#[napi]
impl Signature {
    #[napi(factory)]
    pub fn infinity() -> Result<Self> {
        Ok(Self(chia_sdk_bindings::Signature::infinity()?))
    }

    #[napi]
    pub fn aggregate(signatures: Vec<Reference<Signature>>) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::Signature::aggregate(
            signatures.into_iter().map(|sig| sig.0.clone()).collect(),
        )?))
    }

    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[napi]
    pub fn is_infinity(&self) -> Result<bool> {
        Ok(self.0.is_infinity()?)
    }

    #[napi]
    pub fn is_valid(&self) -> Result<bool> {
        Ok(self.0.is_valid()?)
    }
}
