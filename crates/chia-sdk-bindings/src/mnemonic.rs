use std::str::FromStr;

use bindy::Result;
use chia_protocol::Bytes;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

#[derive(Clone)]
pub struct Mnemonic(bip39::Mnemonic);

impl Mnemonic {
    pub fn new(mnemonic: String) -> Result<Self> {
        Ok(Self(bip39::Mnemonic::from_str(&mnemonic)?))
    }

    pub fn from_entropy(entropy: Bytes) -> Result<Self> {
        Ok(Self(bip39::Mnemonic::from_entropy(&entropy.to_vec())?))
    }

    pub fn generate(use_24: bool) -> Result<Self> {
        let mut rng = ChaCha20Rng::from_entropy();

        let mnemonic = if use_24 {
            let entropy: [u8; 32] = rng.gen();
            bip39::Mnemonic::from_entropy(&entropy)?
        } else {
            let entropy: [u8; 16] = rng.gen();
            bip39::Mnemonic::from_entropy(&entropy)?
        };

        Ok(Self(mnemonic))
    }

    pub fn verify(mnemonic: String) -> Result<bool> {
        Ok(bip39::Mnemonic::from_str(&mnemonic).is_ok())
    }

    pub fn to_string(&self) -> Result<String> {
        Ok(self.0.to_string())
    }

    pub fn to_entropy(&self) -> Result<Bytes> {
        Ok(Bytes::new(self.0.to_entropy()))
    }

    pub fn to_seed(&self, password: String) -> Result<Bytes> {
        Ok(Bytes::new(self.0.to_seed(password).to_vec()))
    }
}
