use chia_protocol::BytesImpl;

use crate::{impl_self, Error, FromRust, IntoRust, Result};

pub struct WasmContext;

impl_self!(Vec<u8>);

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
