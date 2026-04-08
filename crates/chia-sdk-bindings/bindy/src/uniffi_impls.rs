use crate::{Error, FromRust, IntoRust, Result, impl_self};
use chia_protocol::{Bytes, BytesImpl, ClassgroupElement, Program};
use clvm_utils::TreeHash;
use num_bigint::BigInt;

#[derive(Debug, Clone, Copy)]
pub struct Uniffi;

#[derive(Debug, Clone, Copy)]
pub struct UniffiContext;

// --- Unit type ---

impl<T> FromRust<(), T, Uniffi> for () {
    fn from_rust(_value: (), _context: &T) -> Result<Self> {
        Ok(())
    }
}

impl<T> IntoRust<(), T, Uniffi> for () {
    fn into_rust(self, _context: &T) -> Result<Self> {
        Ok(())
    }
}

// --- BigInt, u64, u128 → String ---
// UniFFI has no native BigInt. We use String; C# parses with BigInteger.Parse().

impl<T> FromRust<BigInt, T, Uniffi> for String {
    fn from_rust(value: BigInt, _context: &T) -> Result<Self> {
        Ok(value.to_string())
    }
}

impl<T> IntoRust<BigInt, T, Uniffi> for String {
    fn into_rust(self, _context: &T) -> Result<BigInt> {
        Ok(self.parse()?)
    }
}

impl<T> FromRust<u64, T, Uniffi> for String {
    fn from_rust(value: u64, _context: &T) -> Result<Self> {
        Ok(value.to_string())
    }
}

impl<T> IntoRust<u64, T, Uniffi> for String {
    fn into_rust(self, _context: &T) -> Result<u64> {
        self.parse().map_err(|_| Error::Custom(format!("cannot parse '{self}' as u64")))
    }
}

impl<T> FromRust<u128, T, Uniffi> for String {
    fn from_rust(value: u128, _context: &T) -> Result<Self> {
        Ok(value.to_string())
    }
}

impl<T> IntoRust<u128, T, Uniffi> for String {
    fn into_rust(self, _context: &T) -> Result<u128> {
        self.parse().map_err(|_| Error::Custom(format!("cannot parse '{self}' as u128")))
    }
}

// i64 and i128 — native UniFFI types, pass through as-is.
impl_self!(i64);
impl_self!(i128);

// usize — keep native (maps to u64 in UniFFI via {usize} group).
// The blanket impl_self!(usize) in lib.rs covers this; no additional impl needed here.

// --- Bytes types → Vec<u8> (same pattern as pyo3_impls.rs) ---

impl<T, const N: usize> FromRust<BytesImpl<N>, T, Uniffi> for Vec<u8> {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T, Uniffi> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<BytesImpl<N>> {
        if self.len() != N {
            return Err(Error::WrongLength { expected: N, found: self.len() });
        }
        Ok(self.try_into().unwrap())
    }
}

impl<T> FromRust<ClassgroupElement, T, Uniffi> for Vec<u8> {
    fn from_rust(value: ClassgroupElement, _context: &T) -> Result<Self> {
        Ok(value.data.to_vec())
    }
}

impl<T> IntoRust<ClassgroupElement, T, Uniffi> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<ClassgroupElement> {
        if self.len() != 100 {
            return Err(Error::WrongLength { expected: 100, found: self.len() });
        }
        Ok(ClassgroupElement::new(self.try_into().unwrap()))
    }
}

impl<T> FromRust<TreeHash, T, Uniffi> for Vec<u8> {
    fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<TreeHash, T, Uniffi> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<TreeHash> {
        if self.len() != 32 {
            return Err(Error::WrongLength { expected: 32, found: self.len() });
        }
        Ok(TreeHash::new(self.try_into().unwrap()))
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
