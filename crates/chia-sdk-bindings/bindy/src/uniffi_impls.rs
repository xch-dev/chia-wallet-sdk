use std::{
    future::Future,
    str::FromStr,
    sync::{Arc, LazyLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use chia_protocol::{Bytes, BytesImpl, ClassgroupElement, Program};
use clvm_utils::TreeHash;
use num_bigint::BigInt;

use crate::{Error, FromRust, IntoRust, Result};

#[derive(Debug, Clone, Copy)]
pub struct Uniffi;

#[derive(Debug, Clone, Copy)]
pub struct UniffiContext;

/// A `UniFFI` object that wraps a binding type.
///
/// `UniFFI` only hands out shared references (`Arc<Self>`) to objects, so the inner value is
/// kept behind a lock to allow field setters to mutate it in place, the same as in the other
/// bindings.
pub trait UniffiObject: Send + Sync + 'static {
    type Inner: Clone;

    fn wrap(inner: Self::Inner) -> Self;

    fn lock(&self) -> &RwLock<Self::Inner>;

    fn read(&self) -> RwLockReadGuard<'_, Self::Inner> {
        self.lock().read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Self::Inner> {
        self.lock().write().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<W, C> FromRust<W::Inner, C, Uniffi> for Arc<W>
where
    W: UniffiObject,
{
    fn from_rust(value: W::Inner, _context: &C) -> Result<Self> {
        Ok(Arc::new(W::wrap(value)))
    }
}

impl<W, C> IntoRust<W::Inner, C, Uniffi> for Arc<W>
where
    W: UniffiObject,
{
    fn into_rust(self, _context: &C) -> Result<W::Inner> {
        Ok(self.read().clone())
    }
}

static RUNTIME: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("failed to create the Tokio runtime"));

/// Drives a future to completion on a shared Tokio runtime.
///
/// This is used to expose async methods as blocking calls for languages whose `UniFFI`
/// generator doesn't support async functions (currently C++).
pub fn block_on<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    RUNTIME.block_on(future)
}

impl<T> FromRust<(), T, Uniffi> for () {
    fn from_rust(value: (), _context: &T) -> Result<Self> {
        Ok(value)
    }
}

impl<T> IntoRust<(), T, Uniffi> for () {
    fn into_rust(self, _context: &T) -> Result<Self> {
        Ok(self)
    }
}

// UniFFI has no arbitrary precision or 128-bit integer type, so these are passed as decimal
// strings. Every target language can parse them into its native big integer type.

impl<T> FromRust<BigInt, T, Uniffi> for String {
    fn from_rust(value: BigInt, _context: &T) -> Result<Self> {
        Ok(value.to_string())
    }
}

impl<T> IntoRust<BigInt, T, Uniffi> for String {
    fn into_rust(self, _context: &T) -> Result<BigInt> {
        Ok(BigInt::from_str(&self)?)
    }
}

impl<T> FromRust<u128, T, Uniffi> for String {
    fn from_rust(value: u128, _context: &T) -> Result<Self> {
        Ok(value.to_string())
    }
}

impl<T> IntoRust<u128, T, Uniffi> for String {
    fn into_rust(self, _context: &T) -> Result<u128> {
        self.parse()
            .map_err(|_| Error::Custom(format!("Invalid u128 value: {self}")))
    }
}

impl<T> FromRust<usize, T, Uniffi> for u64 {
    fn from_rust(value: usize, _context: &T) -> Result<Self> {
        Ok(value as u64)
    }
}

impl<T> IntoRust<usize, T, Uniffi> for u64 {
    fn into_rust(self, _context: &T) -> Result<usize> {
        usize::try_from(self).map_err(|_| Error::TooLarge)
    }
}

impl<T, const N: usize> FromRust<BytesImpl<N>, T, Uniffi> for Vec<u8> {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T, Uniffi> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<BytesImpl<N>> {
        let found = self.len();

        Ok(BytesImpl::new(
            self.try_into()
                .map_err(|_| Error::WrongLength { expected: N, found })?,
        ))
    }
}

impl<T> FromRust<ClassgroupElement, T, Uniffi> for Vec<u8> {
    fn from_rust(value: ClassgroupElement, _context: &T) -> Result<Self> {
        Ok(value.data.to_vec())
    }
}

impl<T> IntoRust<ClassgroupElement, T, Uniffi> for Vec<u8> {
    fn into_rust(self, context: &T) -> Result<ClassgroupElement> {
        let bytes = IntoRust::<BytesImpl<100>, T, Uniffi>::into_rust(self, context)?;
        Ok(ClassgroupElement::new(bytes))
    }
}

impl<T> FromRust<TreeHash, T, Uniffi> for Vec<u8> {
    fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<TreeHash, T, Uniffi> for Vec<u8> {
    fn into_rust(self, context: &T) -> Result<TreeHash> {
        let bytes = IntoRust::<BytesImpl<32>, T, Uniffi>::into_rust(self, context)?;
        Ok(TreeHash::new(bytes.to_bytes()))
    }
}

impl<T> FromRust<Bytes, T, Uniffi> for Vec<u8> {
    fn from_rust(value: Bytes, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Bytes, T, Uniffi> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Bytes> {
        Ok(self.into())
    }
}

impl<T> FromRust<Program, T, Uniffi> for Vec<u8> {
    fn from_rust(value: Program, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Program, T, Uniffi> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Program> {
        Ok(self.into())
    }
}
