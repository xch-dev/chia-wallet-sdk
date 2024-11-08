use chia::{
    bls::{
        self, master_to_wallet_hardened, master_to_wallet_hardened_intermediate,
        master_to_wallet_unhardened, master_to_wallet_unhardened_intermediate, sign, verify,
        DerivableKey,
    },
    puzzles::DeriveSynthetic,
};
use napi::bindgen_prelude::*;

use crate::traits::IntoRust;

#[napi]
pub struct SecretKey(bls::SecretKey);

#[napi]
impl SecretKey {
    #[napi(factory)]
    pub fn from_seed(seed: Uint8Array) -> Result<Self> {
        let seed: Vec<u8> = seed.into_rust()?;
        Ok(Self(bls::SecretKey::from_seed(&seed)))
    }

    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            bls::SecretKey::from_bytes(&bytes.into_rust()?)
                .map_err(|error| Error::from_reason(error.to_string()))?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }

    #[napi]
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.public_key())
    }

    #[napi]
    pub fn sign(&self, message: Uint8Array) -> Signature {
        Signature(sign(&self.0, message))
    }

    #[napi]
    pub fn derive_unhardened(&self, index: u32) -> Self {
        Self(self.0.derive_unhardened(index))
    }

    #[napi]
    pub fn derive_hardened(&self, index: u32) -> Self {
        Self(self.0.derive_hardened(index))
    }

    #[napi]
    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Self {
        let mut result = self.0.clone();
        for index in path {
            result = result.derive_unhardened(index);
        }
        Self(result)
    }

    #[napi]
    pub fn derive_hardened_path(&self, path: Vec<u32>) -> Self {
        let mut result = self.0.clone();
        for index in path {
            result = result.derive_hardened(index);
        }
        Self(result)
    }

    #[napi]
    pub fn derive_unhardened_wallet_intermediate(&self) -> Self {
        Self(master_to_wallet_unhardened_intermediate(&self.0))
    }

    #[napi]
    pub fn derive_hardened_wallet_intermediate(&self) -> Self {
        Self(master_to_wallet_hardened_intermediate(&self.0))
    }

    #[napi]
    pub fn derive_unhardened_wallet(&self, index: u32) -> Self {
        Self(master_to_wallet_unhardened(&self.0, index))
    }

    #[napi]
    pub fn derive_hardened_wallet(&self, index: u32) -> Self {
        Self(master_to_wallet_hardened(&self.0, index))
    }

    #[napi]
    pub fn derive_synthetic(&self) -> Self {
        Self(self.0.derive_synthetic())
    }

    #[napi]
    pub fn derive_synthetic_with_hidden_puzzle(
        &self,
        hidden_puzzle_hash: Uint8Array,
    ) -> Result<Self> {
        Ok(Self(
            self.0
                .derive_synthetic_hidden(&hidden_puzzle_hash.into_rust()?),
        ))
    }
}

#[napi]
pub struct PublicKey(bls::PublicKey);

#[napi]
impl PublicKey {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            bls::PublicKey::from_bytes(&bytes.into_rust()?)
                .map_err(|error| Error::from_reason(error.to_string()))?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }

    #[napi(factory)]
    pub fn empty() -> Self {
        Self(bls::PublicKey::default())
    }

    #[napi(factory)]
    pub fn aggregate(public_keys: Vec<Reference<PublicKey>>) -> Self {
        let mut result = bls::PublicKey::default();
        for pk in public_keys {
            result += &pk.0;
        }
        Self(result)
    }

    #[napi]
    pub fn fingerprint(&self) -> u32 {
        self.0.get_fingerprint()
    }

    #[napi]
    pub fn is_infinity(&self) -> bool {
        self.0.is_inf()
    }

    #[napi]
    pub fn is_valid(&self) -> bool {
        self.0.is_valid()
    }

    #[napi]
    pub fn verify(&self, message: Uint8Array, signature: Reference<Signature>) -> bool {
        verify(&signature.0, &self.0, message)
    }

    #[napi]
    pub fn derive_unhardened(&self, index: u32) -> Self {
        Self(self.0.derive_unhardened(index))
    }

    #[napi]
    pub fn derive_unhardened_path(&self, path: Vec<u32>) -> Self {
        let mut result = self.0;
        for index in path {
            result = result.derive_unhardened(index);
        }
        Self(result)
    }

    #[napi]
    pub fn derive_unhardened_wallet_intermediate(&self) -> Self {
        Self(master_to_wallet_unhardened_intermediate(&self.0))
    }

    #[napi]
    pub fn derive_unhardened_wallet(&self, index: u32) -> Self {
        Self(master_to_wallet_unhardened(&self.0, index))
    }

    #[napi]
    pub fn derive_synthetic(&self) -> Self {
        Self(self.0.derive_synthetic())
    }

    #[napi]
    pub fn derive_synthetic_with_hidden_puzzle(
        &self,
        hidden_puzzle_hash: Uint8Array,
    ) -> Result<Self> {
        Ok(Self(
            self.0
                .derive_synthetic_hidden(&hidden_puzzle_hash.into_rust()?),
        ))
    }
}

#[napi]
pub struct Signature(bls::Signature);

#[napi]
impl Signature {
    #[napi(factory)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Self> {
        Ok(Self(
            bls::Signature::from_bytes(&bytes.into_rust()?)
                .map_err(|error| Error::from_reason(error.to_string()))?,
        ))
    }

    #[napi]
    pub fn to_bytes(&self) -> Uint8Array {
        Uint8Array::new(self.0.to_bytes().to_vec())
    }

    #[napi(factory)]
    pub fn empty() -> Self {
        Self(bls::Signature::default())
    }

    #[napi(factory)]
    pub fn aggregate(signatures: Vec<Reference<Signature>>) -> Self {
        let mut result = bls::Signature::default();
        for sig in signatures {
            result += &sig.0;
        }
        Self(result)
    }

    #[napi]
    pub fn is_valid(&self) -> bool {
        self.0.is_valid()
    }
}
