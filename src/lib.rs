#![deny(missing_docs)]

//! This crate is a work in progress.

mod address;
mod condition;
mod spends;
mod ssl;
mod utils;

/// The `sqlite` module contains the SQLite storage backend.
#[cfg(any(test, feature = "sqlite"))]
pub mod sqlite;

/// Contains logic needed to glue a wallet together with all of the data stores.
pub mod wallet;

pub use address::*;
pub use condition::*;
pub use spends::*;
pub use ssl::*;
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
