use std::string::FromUtf8Error;

use chia_sdk_driver::DriverError;
use chia_sdk_test::SimulatorError;
use chia_sdk_utils::AddressError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;
use num_bigint::{BigInt, TryFromBigIntError};

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

    #[error("Driver error: {0}")]
    Driver(#[from] DriverError),

    #[error("Eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("Value is infinite")]
    Infinite,

    #[error("Value is NaN")]
    NaN,

    #[error("Value has a fractional part")]
    Fractional,

    #[error("Value is larger than MAX_SAFE_INTEGER")]
    TooLarge,

    #[error("Value is smaller than MIN_SAFE_INTEGER")]
    TooSmall,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),

    #[error("Atom expected")]
    AtomExpected,

    #[error("Pair expected")]
    PairExpected,

    #[error("To CLVM error: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("From CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[cfg(feature = "wasm")]
    #[error("Range error: {0:?}")]
    Range(js_sys::RangeError),

    #[error("BigInt error: {0}")]
    BigInt(#[from] TryFromBigIntError<BigInt>),

    #[error("Missing parent inner puzzle hash")]
    MissingParentInnerPuzzleHash,

    #[error("Simulator error: {0}")]
    Simulator(#[from] SimulatorError),
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
