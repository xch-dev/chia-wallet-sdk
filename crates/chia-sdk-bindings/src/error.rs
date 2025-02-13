#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum Error {
    #[error("Expected {expected} bytes, but instead found {found}")]
    WrongLength { expected: usize, found: usize },

    #[error("Address error: {0}")]
    Bech32(#[from] bech32::Error),

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
