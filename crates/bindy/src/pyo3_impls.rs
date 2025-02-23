use crate::{impl_self, Error, FromRust, IntoRust, Result};
use chia_protocol::{Bytes, BytesImpl, Program};
use clvm_utils::TreeHash;
use num_bigint::BigInt;

impl From<Error> for pyo3::PyErr {
    fn from(error: Error) -> Self {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}

pub struct Pyo3Context;

impl_self!(u64);
impl_self!(i64);
impl_self!(());
impl_self!(BigInt);

impl<T, const N: usize> FromRust<BytesImpl<N>, T> for Vec<u8> {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<BytesImpl<N>> {
        let bytes = self.to_vec();

        if bytes.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: bytes.len(),
            });
        }

        Ok(bytes.try_into().unwrap())
    }
}

impl<T> FromRust<TreeHash, T> for Vec<u8> {
    fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<TreeHash, T> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<TreeHash> {
        if self.len() != 32 {
            return Err(Error::WrongLength {
                expected: 32,
                found: self.len(),
            });
        }

        Ok(TreeHash::new(self.try_into().unwrap()))
    }
}

impl<T> FromRust<Bytes, T> for Vec<u8> {
    fn from_rust(value: Bytes, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Bytes, T> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Bytes> {
        Ok(self.to_vec().into())
    }
}

impl<T> FromRust<Program, T> for Vec<u8> {
    fn from_rust(value: Program, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Program, T> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Program> {
        Ok(self.to_vec().into())
    }
}
