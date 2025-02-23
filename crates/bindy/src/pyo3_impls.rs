use chia_protocol::{Bytes, BytesImpl, Program};

use crate::{impl_self, Error, FromRust, IntoRust, Result};

impl From<Error> for pyo3::PyErr {
    fn from(error: Error) -> Self {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}

pub struct Pyo3Context;

impl_self!(u64);
impl_self!(i64);

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
