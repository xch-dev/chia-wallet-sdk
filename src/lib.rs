#![deny(missing_docs)]

//! This crate is a work in progress.

mod address;
mod condition;
mod signer;
mod spends;
mod ssl;
mod stores;
mod utils;
mod wallet;

/// The `sqlite` module contains the SQLite storage backend.
#[cfg(any(test, feature = "sqlite"))]
pub mod sqlite;

pub use address::*;
pub use condition::*;
pub use signer::*;
pub use spends::*;
pub use ssl::*;
pub use stores::*;
pub use wallet::*;

#[cfg(test)]
mod testing {
    use std::str::FromStr;

    use bip39::Mnemonic;
    use chia_bls::SecretKey;
    use once_cell::sync::Lazy;

    const MNEMONIC: &str = "setup update spoil lazy square course ring tell hard eager industry ticket guess amused build reunion woman system cause afraid first material machine morning";
    pub(crate) static SECRET_KEY: Lazy<SecretKey> =
        Lazy::new(|| SecretKey::from_seed(&Mnemonic::from_str(MNEMONIC).unwrap().to_seed("")));
}
