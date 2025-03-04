use std::str::FromStr;

use chia_protocol::{Bytes, Bytes32, BytesImpl, ClassgroupElement, Program};
use clvm_utils::TreeHash;
use js_sys::wasm_bindgen::{JsCast, UnwrapThrowExt};
use js_sys::Uint8Array;
use num_bigint::BigInt;

use crate::{Error, FromRust, IntoRust, Result};

pub struct Wasm;

pub struct WasmContext;

impl<T> FromRust<(), T, Wasm> for () {
    fn from_rust(value: (), _context: &T) -> Result<Self> {
        Ok(value)
    }
}

impl<T> IntoRust<(), T, Wasm> for () {
    fn into_rust(self, _context: &T) -> Result<Self> {
        Ok(self)
    }
}

impl<T> FromRust<BigInt, T, Wasm> for js_sys::BigInt {
    fn from_rust(value: BigInt, _context: &T) -> Result<Self> {
        js_sys::BigInt::from_str(&value.to_string())
            .map_err(|error| Error::Custom(format!("{error:?}")))
    }
}

impl<T> IntoRust<BigInt, T, Wasm> for js_sys::BigInt {
    fn into_rust(self, _context: &T) -> Result<BigInt> {
        Ok(String::from(
            self.to_string(10)
                .map_err(|error| Error::Custom(format!("{error:?}")))?,
        )
        .parse()?)
    }
}

impl<T, const N: usize> FromRust<BytesImpl<N>, T, Wasm> for Vec<u8> {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T, Wasm> for Vec<u8> {
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

impl<T> FromRust<ClassgroupElement, T, Wasm> for Vec<u8> {
    fn from_rust(value: ClassgroupElement, _context: &T) -> Result<Self> {
        Ok(value.data.to_vec())
    }
}

impl<T> IntoRust<ClassgroupElement, T, Wasm> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<ClassgroupElement> {
        if self.len() != 100 {
            return Err(Error::WrongLength {
                expected: 100,
                found: self.len(),
            });
        }

        Ok(ClassgroupElement::new(self.try_into().unwrap()))
    }
}

impl<T> FromRust<TreeHash, T, Wasm> for Vec<u8> {
    fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<TreeHash, T, Wasm> for Vec<u8> {
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

impl<T> FromRust<Bytes, T, Wasm> for Vec<u8> {
    fn from_rust(value: Bytes, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Bytes, T, Wasm> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Bytes> {
        Ok(self.to_vec().into())
    }
}

impl<T> FromRust<Program, T, Wasm> for Vec<u8> {
    fn from_rust(value: Program, _context: &T) -> Result<Self> {
        Ok(value.to_vec())
    }
}

impl<T> IntoRust<Program, T, Wasm> for Vec<u8> {
    fn into_rust(self, _context: &T) -> Result<Program> {
        Ok(self.to_vec().into())
    }
}

impl<T> IntoRust<u64, T, Wasm> for js_sys::BigInt {
    fn into_rust(self, _context: &T) -> Result<u64> {
        let bigint: BigInt = self.into_rust(_context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u64, T, Wasm> for js_sys::BigInt {
    fn from_rust(value: u64, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T> IntoRust<u128, T, Wasm> for js_sys::BigInt {
    fn into_rust(self, _context: &T) -> Result<u128> {
        let bigint: BigInt = self.into_rust(_context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u128, T, Wasm> for js_sys::BigInt {
    fn from_rust(value: u128, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T> IntoRust<Vec<Bytes32>, T, Wasm> for js_sys::Array {
    fn into_rust(self, context: &T) -> Result<Vec<Bytes32>> {
        let bytes_array: Vec<Vec<u8>> = self
            .values()
            .into_iter()
            .map(|item| item.unwrap_throw().unchecked_ref::<Uint8Array>().to_vec())
            .collect();

        let mut bytes32_array = Vec::with_capacity(bytes_array.len());

        for bytes in bytes_array {
            bytes32_array.push(IntoRust::<Bytes32, T, Wasm>::into_rust(bytes, context)?);
        }

        Ok(bytes32_array)
    }
}

impl<T> IntoRust<Vec<Bytes>, T, Wasm> for js_sys::Array {
    fn into_rust(self, context: &T) -> Result<Vec<Bytes>> {
        let bytes_array: Vec<Vec<u8>> = self
            .values()
            .into_iter()
            .map(|item| item.unwrap_throw().unchecked_ref::<Uint8Array>().to_vec())
            .collect();

        let mut res_bytes_array: Vec<Bytes> = Vec::with_capacity(bytes_array.len());

        for bytes in bytes_array {
            res_bytes_array.push(IntoRust::<Bytes, T, Wasm>::into_rust(bytes, context)?);
        }

        Ok(res_bytes_array)
    }
}

impl<T> IntoRust<Vec<TreeHash>, T, Wasm> for js_sys::Array {
    fn into_rust(self, context: &T) -> Result<Vec<TreeHash>> {
        let bytes_array: Vec<Vec<u8>> = self
            .values()
            .into_iter()
            .map(|item| item.unwrap_throw().unchecked_ref::<Uint8Array>().to_vec())
            .collect();

        let mut bytes32_array = Vec::with_capacity(bytes_array.len());

        for bytes in bytes_array {
            bytes32_array.push(IntoRust::<TreeHash, T, Wasm>::into_rust(bytes, context)?);
        }

        Ok(bytes32_array)
    }
}

impl<T> FromRust<Vec<TreeHash>, T, Wasm> for js_sys::Array {
    fn from_rust(value: Vec<TreeHash>, _context: &T) -> Result<Self> {
        let array = js_sys::Array::new();

        for item in value {
            array
                .push(&<Vec<u8> as FromRust<TreeHash, T, Wasm>>::from_rust(item, _context)?.into());
        }

        Ok(array)
    }
}

impl<T> FromRust<Vec<Bytes32>, T, Wasm> for js_sys::Array {
    fn from_rust(value: Vec<Bytes32>, _context: &T) -> Result<Self> {
        let array = js_sys::Array::new();

        for item in value {
            array.push(&<Vec<u8> as FromRust<Bytes32, T, Wasm>>::from_rust(item, _context)?.into());
        }

        Ok(array)
    }
}

impl<T> FromRust<Vec<Bytes>, T, Wasm> for js_sys::Array {
    fn from_rust(value: Vec<Bytes>, _context: &T) -> Result<Self> {
        let array = js_sys::Array::new();

        for item in value {
            array.push(&<Vec<u8> as FromRust<Bytes, T, Wasm>>::from_rust(item, _context)?.into());
        }

        Ok(array)
    }
}
