use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoRust};

#[napi]
pub struct K1SecretKey(pub(crate) chia_sdk_bindings::K1SecretKey);

#[napi]
impl K1SecretKey {
    #[napi]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::K1SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[napi]
    pub fn public_key(&self) -> Result<K1PublicKey> {
        Ok(K1PublicKey(self.0.public_key()?))
    }

    #[napi]
    pub fn sign_prehashed(&self, prehashed: Uint8Array) -> Result<K1Signature> {
        Ok(K1Signature(self.0.sign_prehashed(prehashed.rust()?)?))
    }
}

#[napi]
pub struct K1PublicKey(pub(crate) chia_sdk_bindings::K1PublicKey);

#[napi]
impl K1PublicKey {
    #[napi]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::K1PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes().js()?)
    }

    #[napi]
    pub fn fingerprint(&self) -> u32 {
        self.0.fingerprint()
    }

    #[napi]
    pub fn verify_prehashed(&self, prehashed: Uint8Array, signature: &K1Signature) -> Result<bool> {
        Ok(self.0.verify_prehashed(prehashed.rust()?, signature.0))
    }
}

#[napi]
pub struct K1Signature(pub(crate) chia_sdk_bindings::K1Signature);

#[napi]
impl K1Signature {
    #[napi]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::K1Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }
}

#[napi]
pub struct R1SecretKey(pub(crate) chia_sdk_bindings::R1SecretKey);

#[napi]
impl R1SecretKey {
    #[napi]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::R1SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[napi]
    pub fn public_key(&self) -> Result<R1PublicKey> {
        Ok(R1PublicKey(self.0.public_key()?))
    }

    #[napi]
    pub fn sign_prehashed(&self, prehashed: Uint8Array) -> Result<R1Signature> {
        Ok(R1Signature(self.0.sign_prehashed(prehashed.rust()?)?))
    }
}

#[napi]
pub struct R1PublicKey(pub(crate) chia_sdk_bindings::R1PublicKey);

#[napi]
impl R1PublicKey {
    #[napi]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::R1PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[napi]
    pub fn fingerprint(&self) -> u32 {
        self.0.fingerprint()
    }

    #[napi]
    pub fn verify_prehashed(&self, prehashed: Uint8Array, signature: &R1Signature) -> Result<bool> {
        Ok(self.0.verify_prehashed(prehashed.rust()?, signature.0))
    }
}

#[napi]
pub struct R1Signature(pub(crate) chia_sdk_bindings::R1Signature);

#[napi]
impl R1Signature {
    #[napi]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(chia_sdk_bindings::R1Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[napi]
    pub fn to_bytes(&self) -> Result<Uint8Array> {
        Ok(self.0.to_bytes()?.js()?)
    }
}
