#[cfg(all(feature = "napi", not(feature = "wasm"), not(feature = "pyo3")))]
mod napi_impls;

#[cfg(all(feature = "wasm", not(feature = "napi"), not(feature = "pyo3")))]
mod wasm_impls;

#[cfg(all(feature = "pyo3", not(feature = "napi"), not(feature = "wasm")))]
mod pyo3_impls;

#[cfg(all(feature = "napi", not(feature = "wasm"), not(feature = "pyo3")))]
pub use napi_impls::*;

#[cfg(all(feature = "wasm", not(feature = "napi"), not(feature = "pyo3")))]
pub use wasm_impls::*;

#[cfg(all(feature = "pyo3", not(feature = "napi"), not(feature = "wasm")))]
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

    #[cfg(feature = "wasm")]
    #[error("Js error: {0:?}")]
    Js(js_sys::Error),

    #[cfg(feature = "wasm")]
    #[error("Range error: {0:?}")]
    Range(js_sys::RangeError),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait IntoRust<T, C> {
    fn into_rust(self, context: &C) -> Result<T>;
}

pub trait FromRust<T, C>: Sized {
    fn from_rust(value: T, context: &C) -> Result<Self>;
}

#[macro_export]
macro_rules! impl_self {
    ( $ty:ty ) => {
        impl<T> $crate::FromRust<$ty, T> for $ty {
            fn from_rust(value: $ty, _context: &T) -> $crate::Result<Self> {
                Ok(value)
            }
        }

        impl<T> $crate::IntoRust<$ty, T> for $ty {
            fn into_rust(self, _context: &T) -> $crate::Result<$ty> {
                Ok(self)
            }
        }
    };
}

impl_self!(bool);
impl_self!(u16);
impl_self!(i16);
impl_self!(u32);
impl_self!(i32);
impl_self!(String);

impl<R, B, C> IntoRust<Vec<R>, C> for Vec<B>
where
    B: IntoRust<R, C>,
{
    fn into_rust(self, context: &C) -> Result<Vec<R>> {
        self.into_iter().map(|b| b.into_rust(context)).collect()
    }
}

impl<R, B, C> FromRust<Vec<R>, C> for Vec<B>
where
    B: FromRust<R, C>,
{
    fn from_rust(value: Vec<R>, context: &C) -> Result<Self> {
        value
            .into_iter()
            .map(|r| B::from_rust(r, context))
            .collect()
    }
}

impl<R, B, C> IntoRust<Option<R>, C> for Option<B>
where
    B: IntoRust<R, C>,
{
    fn into_rust(self, context: &C) -> Result<Option<R>> {
        self.map(|b| b.into_rust(context)).transpose()
    }
}

impl<R, B, C> FromRust<Option<R>, C> for Option<B>
where
    B: FromRust<R, C>,
{
    fn from_rust(value: Option<R>, context: &C) -> Result<Self> {
        value.map(|r| B::from_rust(r, context)).transpose()
    }
}
