use std::str::FromStr;

use bip39::Mnemonic;
use chia::{
    bls::{
        self, master_to_wallet_hardened, master_to_wallet_hardened_intermediate,
        master_to_wallet_unhardened, master_to_wallet_unhardened_intermediate, sign, verify,
        DerivableKey,
    },
    puzzles::DeriveSynthetic,
};
use napi::bindgen_prelude::*;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::traits::{IntoJs, IntoRust};

#[napi]
pub struct SecretKey(pub(crate) bls::SecretKey);

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
pub struct PublicKey(pub(crate) bls::PublicKey);

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
pub struct Signature(pub(crate) bls::Signature);

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

#[napi]
pub fn mnemonic_from_entropy(entropy: Uint8Array) -> Result<String> {
    Ok(Mnemonic::from_entropy(&entropy)
        .map_err(|error| Error::from_reason(error.to_string()))?
        .to_string())
}

#[napi]
pub fn mnemonic_to_entropy(mnemonic: String) -> Result<Uint8Array> {
    Ok(Mnemonic::from_str(&mnemonic)
        .map_err(|error| Error::from_reason(error.to_string()))?
        .to_entropy()
        .into())
}

#[napi]
pub fn verify_mnemonic(mnemonic: String) -> bool {
    Mnemonic::from_str(&mnemonic).is_ok()
}

#[napi]
pub fn random_bytes(bytes: u32) -> Uint8Array {
    let mut rng = ChaCha20Rng::from_entropy();
    let mut buffer = vec![0; bytes as usize];
    rng.fill_bytes(&mut buffer);
    Uint8Array::new(buffer)
}

#[napi]
pub fn generate_mnemonic(use_24: bool) -> Result<String> {
    let mut rng = ChaCha20Rng::from_entropy();

    let mnemonic = if use_24 {
        let entropy: [u8; 32] = rng.gen();
        Mnemonic::from_entropy(&entropy).map_err(|error| Error::from_reason(error.to_string()))?
    } else {
        let entropy: [u8; 16] = rng.gen();
        Mnemonic::from_entropy(&entropy).map_err(|error| Error::from_reason(error.to_string()))?
    };

    Ok(mnemonic.to_string())
}

#[napi]
pub fn mnemonic_to_seed(mnemonic: String, password: String) -> Result<Uint8Array> {
    let mnemonic =
        Mnemonic::from_str(&mnemonic).map_err(|error| Error::from_reason(error.to_string()))?;
    mnemonic.to_seed(password).into_js()
}
