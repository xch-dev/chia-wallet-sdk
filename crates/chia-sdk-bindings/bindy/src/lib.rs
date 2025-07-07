#[cfg(feature = "napi")]
mod napi_impls;

#[cfg(feature = "wasm")]
mod wasm_impls;

#[cfg(feature = "pyo3")]
mod pyo3_impls;

#[cfg(feature = "napi")]
pub use napi_impls::*;

#[cfg(feature = "wasm")]
pub use wasm_impls::*;

#[cfg(feature = "pyo3")]
pub use pyo3_impls::*;

use std::string::FromUtf8Error;

use chia_sdk_driver::DriverError;
use chia_sdk_test::SimulatorError;
use chia_sdk_utils::AddressError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::reduction::EvalErr;

use num_bigint::{BigInt, ParseBigIntError, TryFromBigIntError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[cfg(feature = "napi")]
    #[error("NAPI error: {0}")]
    Napi(#[from] napi::Error),

    #[error("Wrong length, expected {expected} bytes, found {found}")]
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

    #[error("Missing parent inner puzzle hash")]
    MissingParentInnerPuzzleHash,

    #[error("Simulator error: {0}")]
    Simulator(#[from] SimulatorError),

    #[error("BigInt parse error: {0}")]
    BigIntParse(#[from] ParseBigIntError),

    #[error("BigInt error: {0}")]
    BigInt(#[from] TryFromBigIntError<BigInt>),

    #[error("No spends")]
    NoSpends,

    #[error("{0}")]
    Custom(String),

    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait IntoRust<T, C, L> {
    fn into_rust(self, context: &C) -> Result<T>;
}

pub trait FromRust<T, C, L>: Sized {
    fn from_rust(value: T, context: &C) -> Result<Self>;
}

#[macro_export]
macro_rules! impl_self {
    ( $ty:ty ) => {
        impl<T, L> $crate::FromRust<$ty, T, L> for $ty {
            fn from_rust(value: $ty, _context: &T) -> $crate::Result<Self> {
                Ok(value)
            }
        }

        impl<T, L> $crate::IntoRust<$ty, T, L> for $ty {
            fn into_rust(self, _context: &T) -> $crate::Result<$ty> {
                Ok(self)
            }
        }
    };
}

impl_self!(bool);
impl_self!(u8);
impl_self!(i8);
impl_self!(u16);
impl_self!(i16);
impl_self!(u32);
impl_self!(i32);
impl_self!(String);

impl<R, B, C, L> IntoRust<Vec<R>, C, L> for Vec<B>
where
    B: IntoRust<R, C, L>,
{
    fn into_rust(self, context: &C) -> Result<Vec<R>> {
        self.into_iter().map(|b| b.into_rust(context)).collect()
    }
}

impl<R, B, C, L> FromRust<Vec<R>, C, L> for Vec<B>
where
    B: FromRust<R, C, L>,
{
    fn from_rust(value: Vec<R>, context: &C) -> Result<Self> {
        value
            .into_iter()
            .map(|r| B::from_rust(r, context))
            .collect()
    }
}

impl<R, B, C, L> IntoRust<Option<R>, C, L> for Option<B>
where
    B: IntoRust<R, C, L>,
{
    fn into_rust(self, context: &C) -> Result<Option<R>> {
        self.map(|b| b.into_rust(context)).transpose()
    }
}

impl<R, B, C, L> FromRust<Option<R>, C, L> for Option<B>
where
    B: FromRust<R, C, L>,
{
    fn from_rust(value: Option<R>, context: &C) -> Result<Self> {
        value.map(|r| B::from_rust(r, context)).transpose()
    }
}
