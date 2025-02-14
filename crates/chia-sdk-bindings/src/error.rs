use chia_sdk_utils::AddressError;

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum Error {
    #[error("Expected {expected} bytes, but instead found {found}")]
    WrongLength { expected: usize, found: usize },

    #[error("Bech32m encoding error: {0}")]
    Bech32(#[from] bech32::Error),

    #[error("Bip39 error: {0}")]
    Bip39(#[from] bip39::Error),

    #[error("Address error: {0}")]
    Address(#[from] AddressError),

    #[error("Hex error: {0}")]
    Hex(#[from] hex::FromHexError),
}

#[cfg(feature = "napi")]
impl From<Error> for napi::Error {
    fn from(error: Error) -> Self {
        napi::Error::new(napi::Status::GenericFailure, error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
