use pyo3::prelude::*;

use crate::traits::{IntoPy, IntoRust};

#[pyclass]
pub struct SecretKey(pub(crate) chia_sdk_bindings::SecretKey);

#[pymethods]
impl SecretKey {
    #[staticmethod]
    pub fn from_seed(seed: Vec<u8>) -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::SecretKey::from_seed(seed.rust()?)?))
    }

    #[staticmethod]
    pub fn from_bytes(bytes: Vec<u8>) -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::SecretKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    pub fn to_bytes(&self) -> PyResult<Vec<u8>> {
        Ok(self.0.to_bytes()?.py()?)
    }

    pub fn public_key(&self) -> PyResult<PublicKey> {
        Ok(PublicKey(self.0.public_key()?))
    }

    pub fn sign(&self, message: Vec<u8>) -> PyResult<Signature> {
        Ok(Signature(self.0.sign(message.rust()?)?))
    }

    pub fn derive_unhardened(&self, index: u32) -> PyResult<Self> {
        Ok(Self(self.0.derive_unhardened(index)?))
    }

    pub fn derive_hardened(&self, index: u32) -> PyResult<Self> {
        Ok(Self(self.0.derive_hardened(index)?))
    }

    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> PyResult<Self> {
        Ok(Self(self.0.derive_unhardened_path(path)?))
    }

    pub fn derive_hardened_path(&self, path: Vec<u32>) -> PyResult<Self> {
        Ok(Self(self.0.derive_hardened_path(path)?))
    }

    pub fn derive_synthetic(&self) -> PyResult<Self> {
        Ok(Self(self.0.derive_synthetic()?))
    }

    pub fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Vec<u8>) -> PyResult<Self> {
        Ok(Self(
            self.0.derive_synthetic_hidden(hidden_puzzle_hash.rust()?)?,
        ))
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PublicKey(pub(crate) chia_sdk_bindings::PublicKey);

#[pymethods]
impl PublicKey {
    #[staticmethod]
    pub fn infinity() -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::PublicKey::infinity()?))
    }

    #[staticmethod]
    pub fn aggregate(public_keys: Vec<PublicKey>) -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::PublicKey::aggregate(
            public_keys.into_iter().map(|pk| pk.0).collect(),
        )?))
    }

    #[staticmethod]
    pub fn from_bytes(bytes: Vec<u8>) -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::PublicKey::from_bytes(
            bytes.rust()?,
        )?))
    }

    pub fn to_bytes(&self) -> PyResult<Vec<u8>> {
        Ok(self.0.to_bytes()?.py()?)
    }

    pub fn fingerprint(&self) -> PyResult<u32> {
        Ok(self.0.fingerprint()?)
    }

    pub fn is_infinity(&self) -> PyResult<bool> {
        Ok(self.0.is_infinity()?)
    }

    pub fn is_valid(&self) -> PyResult<bool> {
        Ok(self.0.is_valid()?)
    }

    pub fn derive_unhardened(&self, index: u32) -> PyResult<Self> {
        Ok(Self(self.0.derive_unhardened(index)?))
    }

    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> PyResult<Self> {
        Ok(Self(self.0.derive_unhardened_path(path)?))
    }

    pub fn derive_synthetic(&self) -> PyResult<Self> {
        Ok(Self(self.0.derive_synthetic()?))
    }

    pub fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Vec<u8>) -> PyResult<Self> {
        Ok(Self(
            self.0.derive_synthetic_hidden(hidden_puzzle_hash.rust()?)?,
        ))
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Signature(pub(crate) chia_sdk_bindings::Signature);

#[pymethods]
impl Signature {
    #[staticmethod]
    pub fn infinity() -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::Signature::infinity()?))
    }

    #[staticmethod]
    pub fn aggregate(signatures: Vec<Signature>) -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::Signature::aggregate(
            signatures.into_iter().map(|sig| sig.0.clone()).collect(),
        )?))
    }

    #[staticmethod]
    pub fn from_bytes(bytes: Vec<u8>) -> PyResult<Self> {
        Ok(Self(chia_sdk_bindings::Signature::from_bytes(
            bytes.rust()?,
        )?))
    }

    pub fn to_bytes(&self) -> PyResult<Vec<u8>> {
        Ok(self.0.to_bytes()?.py()?)
    }

    pub fn is_infinity(&self) -> PyResult<bool> {
        Ok(self.0.is_infinity()?)
    }

    pub fn is_valid(&self) -> PyResult<bool> {
        Ok(self.0.is_valid()?)
    }
}
