use chia_sdk_utils::AddressError;

#[derive(Debug, thiserror::Error)]
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

    #[error("Bls error: {0}")]
    Bls(#[from] chia_bls::Error),

    #[error("Secp error: {0}")]
    Secp(#[from] signature::Error),
}

#[cfg(feature = "napi")]
impl From<Error> for napi::Error {
    fn from(error: Error) -> Self {
        napi::Error::new(napi::Status::GenericFailure, error.to_string())
    }
}

#[cfg(feature = "pyo3")]
impl From<Error> for pyo3::PyErr {
    fn from(error: Error) -> Self {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
