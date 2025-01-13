use chia::{protocol::Bytes32, secp};
use napi::bindgen_prelude::*;

use crate::traits::{js_err, IntoRust};

#[napi]
pub struct K1SecretKey(pub(crate) secp::K1SecretKey);

#[napi]
impl K1SecretKey {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            secp::K1SecretKey::from_bytes(&bytes.into_rust()?).map_err(js_err)?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }

    #[napi]
    pub fn public_key(&self) -> K1PublicKey {
        K1PublicKey(self.0.public_key())
    }

    #[napi]
    pub fn sign_prehashed(&self, prehashed: Uint8Array) -> Result<K1Signature> {
        let value: Bytes32 = prehashed.into_rust()?;
        Ok(K1Signature(
            self.0.sign_prehashed(&value.to_bytes()).map_err(js_err)?,
        ))
    }
}

#[napi]
pub struct K1PublicKey(pub(crate) secp::K1PublicKey);

#[napi]
impl K1PublicKey {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            secp::K1PublicKey::from_bytes(&bytes.into_rust()?).map_err(js_err)?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }
}

#[napi]
pub struct K1Signature(pub(crate) secp::K1Signature);

#[napi]
impl K1Signature {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            secp::K1Signature::from_bytes(&bytes.into_rust()?).map_err(js_err)?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }
}

#[napi]
pub struct R1SecretKey(pub(crate) secp::R1SecretKey);

#[napi]
impl R1SecretKey {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            secp::R1SecretKey::from_bytes(&bytes.into_rust()?).map_err(js_err)?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }

    #[napi]
    pub fn public_key(&self) -> R1PublicKey {
        R1PublicKey(self.0.public_key())
    }

    #[napi]
    pub fn sign_prehashed(&self, prehashed: Uint8Array) -> Result<R1Signature> {
        let value: Bytes32 = prehashed.into_rust()?;
        Ok(R1Signature(
            self.0.sign_prehashed(&value.to_bytes()).map_err(js_err)?,
        ))
    }
}

#[napi]
pub struct R1PublicKey(pub(crate) secp::R1PublicKey);

#[napi]
impl R1PublicKey {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            secp::R1PublicKey::from_bytes(&bytes.into_rust()?).map_err(js_err)?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }
}

#[napi]
pub struct R1Signature(pub(crate) secp::R1Signature);

#[napi]
impl R1Signature {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            secp::R1Signature::from_bytes(&bytes.into_rust()?).map_err(js_err)?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }
}
