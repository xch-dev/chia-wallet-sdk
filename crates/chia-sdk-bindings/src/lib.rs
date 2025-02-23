#![allow(clippy::needless_pass_by_value)]
#![allow(missing_debug_implementations)]
#![allow(clippy::inherent_to_string)]

mod bls;
mod clvm;
mod puzzles;
mod secp;
mod utils;

pub use bls::*;
pub use clvm::*;
pub use puzzles::*;
pub use secp::*;
pub use utils::*;

use std::str::FromStr;

use bindy::Result;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub use chia_protocol::Bytes32;

#[derive(Clone)]
pub struct AddressInfo {
    pub puzzle_hash: Bytes32,
    pub prefix: String,
}

impl AddressInfo {
    pub fn encode(&self) -> Result<String> {
        Ok(chia_sdk_utils::AddressInfo::new(self.puzzle_hash, self.prefix.clone()).encode()?)
    }

    pub fn decode(address: String) -> Result<Self> {
        let info = chia_sdk_utils::AddressInfo::decode(&address)?;
        Ok(Self {
            puzzle_hash: info.puzzle_hash,
            prefix: info.prefix,
        })
    }
}

#[derive(Clone)]
pub struct Mnemonic(bip39::Mnemonic);

impl Mnemonic {
    pub fn new(mnemonic: String) -> Result<Self> {
        Ok(Self(bip39::Mnemonic::from_str(&mnemonic)?))
    }

    pub fn from_entropy(entropy: Vec<u8>) -> Result<Self> {
        Ok(Self(bip39::Mnemonic::from_entropy(&entropy)?))
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

    pub fn to_entropy(&self) -> Result<Vec<u8>> {
        Ok(self.0.to_entropy())
    }

    pub fn to_seed(&self, password: String) -> Result<Vec<u8>> {
        Ok(self.0.to_seed(password).to_vec())
    }
}

pub fn generate_bytes(bytes: u32) -> Result<Vec<u8>> {
    let mut rng = ChaCha20Rng::from_entropy();
    let mut buffer = vec![0; bytes as usize];
    rng.fill_bytes(&mut buffer);
    Ok(buffer)
}
