mod coin_selection;
mod condition;
mod key_store;
mod spends;
mod utils;
mod wallet;

pub use coin_selection::*;
pub use condition::*;
pub use key_store::*;
pub use spends::*;
pub use utils::*;
pub use wallet::*;

#[cfg(test)]
mod testing {
    use std::str::FromStr;

    use bip39::Mnemonic;
    use once_cell::sync::Lazy;

    const MNEMONIC: &str = "setup update spoil lazy square course ring tell hard eager industry ticket guess amused build reunion woman system cause afraid first material machine morning";
    pub(crate) static SEED: Lazy<[u8; 64]> =
        Lazy::new(|| Mnemonic::from_str(MNEMONIC).unwrap().to_seed(""));
}
