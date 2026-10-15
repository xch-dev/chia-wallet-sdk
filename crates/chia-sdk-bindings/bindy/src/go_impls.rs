use crate::{Error, FromRust, IntoRust, Result, impl_self};
use chia_protocol::{Bytes, BytesImpl, ClassgroupElement, Program};
use clvm_utils::TreeHash;
use num_bigint::BigInt;

#[derive(Debug, Clone, Copy)]
pub struct Go;

#[derive(Debug, Clone, Copy)]
pub struct GoContext;

impl_self!(u64);
impl_self!(i64);
impl_self!(u128);
impl_self!(i128);
impl_self!(BigInt);

impl<T> FromRust<(), T, Go> for () {
    fn from_rust(value: (), _context: &T) -> Result<Self> {
        Ok(value)
    }
}

impl<T> IntoRust<(), T, Go> for () {
    fn into_rust(self, _context: &T) -> Result<Self> {
        Ok(self)
    }
}

impl<T, const N: usize> FromRust<BytesImpl<N>, T, Go> for Vec<u8> {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T, Go> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<BytesImpl<N>> {
        if self.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: self.len(),
            });
        }

        Ok(self
            .try_into()
            .expect("length already checked to be N bytes"))
    }
}

impl<T> FromRust<ClassgroupElement, T, Go> for Vec<u8> {
    fn from_rust(value: ClassgroupElement, _context: &T) -> Result<Self> {
        Ok(value.data.to_vec())
    }
}

impl<T> IntoRust<ClassgroupElement, T, Go> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<ClassgroupElement> {
        if self.len() != 100 {
            return Err(Error::WrongLength {
                expected: 100,
                found: self.len(),
            });
        }

        Ok(ClassgroupElement::new(
            self.try_into()
                .expect("length already checked to be 100 bytes"),
        ))
    }
}

impl<T> FromRust<TreeHash, T, Go> for Vec<u8> {
    fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<TreeHash, T, Go> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<TreeHash> {
        if self.len() != 32 {
            return Err(Error::WrongLength {
                expected: 32,
                found: self.len(),
            });
        }

        Ok(TreeHash::new(
            self.try_into()
                .expect("length already checked to be 32 bytes"),
        ))
    }
}

impl<T> FromRust<Bytes, T, Go> for Vec<u8> {
    fn from_rust(value: Bytes, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Bytes, T, Go> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Bytes> {
        Ok(self.into())
    }
}

impl<T> FromRust<Program, T, Go> for Vec<u8> {
    fn from_rust(value: Program, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Program, T, Go> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Program> {
        Ok(self.into())
    }
}

// BigInt ↔ Vec<u8> (signed big-endian bytes, used at the FFI boundary)
impl<T> FromRust<BigInt, T, Go> for Vec<u8> {
    fn from_rust(value: BigInt, _context: &T) -> Result<Self> {
        Ok(value.to_signed_bytes_be())
    }
}

impl<T> IntoRust<BigInt, T, Go> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<BigInt> {
        Ok(BigInt::from_signed_bytes_be(&self))
    }
}

// u128 ↔ BigInt conversions for FFI output
impl<T> FromRust<u128, T, Go> for BigInt {
    fn from_rust(value: u128, _context: &T) -> Result<Self> {
        Ok(BigInt::from(value))
    }
}

impl<T> IntoRust<u128, T, Go> for BigInt {
    fn into_rust(self, _context: &T) -> Result<u128> {
        Ok(self.try_into().map_err(|e: num_bigint::TryFromBigIntError<BigInt>| {
            Error::Custom(e.to_string())
        })?)
    }
}
