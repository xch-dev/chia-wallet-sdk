#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

mod address;
mod condition;
mod spends;
mod ssl;

/// The `prelude` module contains the most commonly used types and traits.
pub mod prelude;

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

fn trim_leading_zeros(mut slice: &[u8]) -> &[u8] {
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    slice
}

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
