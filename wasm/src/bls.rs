use wasm_bindgen::{prelude::wasm_bindgen, JsError};

use crate::{IntoJs, IntoRust};

#[wasm_bindgen]
pub struct SecretKey(pub(crate) chia_sdk_bindings::SecretKey);

#[wasm_bindgen]
impl SecretKey {
    #[wasm_bindgen(js_name = "fromSeed")]
    pub fn from_seed(seed: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::SecretKey::from_seed(seed.rust()?)?))
    }

    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[wasm_bindgen(js_name = "publicKey")]
    pub fn public_key(&self) -> Result<PublicKey, JsError> {
        Ok(PublicKey(self.0.public_key()?))
    }

    #[wasm_bindgen]
    pub fn sign(&self, message: Vec<u8>) -> Result<Signature, JsError> {
        Ok(Signature(self.0.sign(message.rust()?)?))
    }

    #[wasm_bindgen(js_name = "deriveUnhardened")]
    pub fn derive_unhardened(&self, index: u32) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_unhardened(index)?))
    }

    #[wasm_bindgen(js_name = "deriveHardened")]
    pub fn derive_hardened(&self, index: u32) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_hardened(index)?))
    }

    #[wasm_bindgen(js_name = "deriveUnhardenedPath")]
    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_unhardened_path(path)?))
    }

    #[wasm_bindgen(js_name = "deriveHardenedPath")]
    pub fn derive_hardened_path(&self, path: Vec<u32>) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_hardened_path(path)?))
    }

    #[wasm_bindgen(js_name = "deriveSynthetic")]
    pub fn derive_synthetic(&self) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_synthetic()?))
    }

    #[wasm_bindgen(js_name = "deriveSyntheticHidden")]
    pub fn derive_synthetic_hidden(
        &self,
        #[wasm_bindgen(js_name = "hiddenPuzzleHash")] hidden_puzzle_hash: Vec<u8>,
    ) -> Result<Self, JsError> {
        Ok(Self(
            self.0.derive_synthetic_hidden(hidden_puzzle_hash.rust()?)?,
        ))
    }
}

#[wasm_bindgen]
pub struct PublicKey(pub(crate) chia_sdk_bindings::PublicKey);

#[wasm_bindgen]
impl PublicKey {
    #[wasm_bindgen]
    pub fn infinity() -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::PublicKey::infinity()?))
    }

    #[wasm_bindgen]
    pub fn aggregate(
        #[wasm_bindgen(js_name = "publicKeys")] public_keys: Vec<PublicKey>,
    ) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::PublicKey::aggregate(
            public_keys.into_iter().map(|pk| pk.0).collect(),
        )?))
    }

    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[wasm_bindgen]
    pub fn fingerprint(&self) -> Result<u32, JsError> {
        Ok(self.0.fingerprint()?)
    }

    #[wasm_bindgen(js_name = "isInfinity")]
    pub fn is_infinity(&self) -> Result<bool, JsError> {
        Ok(self.0.is_infinity()?)
    }

    #[wasm_bindgen(js_name = "isValid")]
    pub fn is_valid(&self) -> Result<bool, JsError> {
        Ok(self.0.is_valid()?)
    }

    #[wasm_bindgen(js_name = "deriveUnhardened")]
    pub fn derive_unhardened(&self, index: u32) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_unhardened(index)?))
    }

    #[wasm_bindgen(js_name = "deriveUnhardenedPath")]
    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_unhardened_path(path)?))
    }

    #[wasm_bindgen(js_name = "deriveSynthetic")]
    pub fn derive_synthetic(&self) -> Result<Self, JsError> {
        Ok(Self(self.0.derive_synthetic()?))
    }

    #[wasm_bindgen(js_name = "deriveSyntheticHidden")]
    pub fn derive_synthetic_hidden(
        &self,
        #[wasm_bindgen(js_name = "hiddenPuzzleHash")] hidden_puzzle_hash: Vec<u8>,
    ) -> Result<Self, JsError> {
        Ok(Self(
            self.0.derive_synthetic_hidden(hidden_puzzle_hash.rust()?)?,
        ))
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Signature(pub(crate) chia_sdk_bindings::Signature);

#[wasm_bindgen]
impl Signature {
    #[wasm_bindgen]
    pub fn infinity() -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::Signature::infinity()?))
    }

    #[wasm_bindgen]
    pub fn aggregate(signatures: Vec<Signature>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::Signature::aggregate(
            signatures.into_iter().map(|sig| sig.0.clone()).collect(),
        )?))
    }

    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, JsError> {
        Ok(Self(chia_sdk_bindings::Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.0.to_bytes()?.js()?)
    }

    #[wasm_bindgen(js_name = "isInfinity")]
    pub fn is_infinity(&self) -> Result<bool, JsError> {
        Ok(self.0.is_infinity()?)
    }

    #[wasm_bindgen(js_name = "isValid")]
    pub fn is_valid(&self) -> Result<bool, JsError> {
        Ok(self.0.is_valid()?)
    }
}
