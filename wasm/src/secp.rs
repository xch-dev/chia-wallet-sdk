use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen]
#[derive(Clone)]
pub struct K1SecretKey(pub(crate) chia_sdk_bindings::K1SecretKey);

#[wasm_bindgen]
impl K1SecretKey {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::K1SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[wasm_bindgen(js_name = "publicKey")]
    pub fn public_key(&self) -> Result<K1PublicKey, JsError> {
        Ok(K1PublicKey(self.0.public_key()?))
    }

    #[wasm_bindgen(js_name = "signPrehashed")]
    pub fn sign_prehashed(&self, prehashed: Vec<u8>) -> Result<K1Signature, JsError> {
        Ok(K1Signature(self.0.sign_prehashed(prehashed.rust()?)?))
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct K1PublicKey(pub(crate) chia_sdk_bindings::K1PublicKey);

#[wasm_bindgen]
impl K1PublicKey {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::K1PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes().js()?)
    }
}

#[wasm_bindgen]
pub struct K1Signature(pub(crate) chia_sdk_bindings::K1Signature);

#[wasm_bindgen]
impl K1Signature {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::K1Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct R1SecretKey(pub(crate) chia_sdk_bindings::R1SecretKey);

#[wasm_bindgen]
impl R1SecretKey {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::R1SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[wasm_bindgen(js_name = "publicKey")]
    pub fn public_key(&self) -> Result<R1PublicKey, JsError> {
        Ok(R1PublicKey(self.0.public_key()?))
    }

    #[wasm_bindgen(js_name = "signPrehashed")]
    pub fn sign_prehashed(&self, prehashed: Vec<u8>) -> Result<R1Signature, JsError> {
        Ok(R1Signature(self.0.sign_prehashed(prehashed.rust()?)?))
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct R1PublicKey(pub(crate) chia_sdk_bindings::R1PublicKey);

#[wasm_bindgen]
impl R1PublicKey {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::R1PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes().js()?)
    }
}

#[wasm_bindgen]
pub struct R1Signature(pub(crate) chia_sdk_bindings::R1Signature);

#[wasm_bindgen]
impl R1Signature {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::R1Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }
}
