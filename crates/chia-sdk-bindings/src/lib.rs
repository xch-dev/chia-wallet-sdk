#![allow(clippy::needless_pass_by_value)]
#![allow(missing_debug_implementations)]

mod bls;
mod clvm;
mod mnemonic;
mod puzzles;
mod secp;
mod utils;

pub use bls::*;
pub use clvm::*;
pub use mnemonic::*;
pub use puzzles::*;
pub use secp::*;
pub use utils::*;

pub use chia_protocol::Bytes32;

use bindy::Result;

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
