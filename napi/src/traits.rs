use chia::{
    bls::PublicKey,
    protocol::{Bytes, BytesImpl},
};
use clvmr::NodePtr;
use napi::bindgen_prelude::*;

use crate::{ClvmAllocator, Program};

pub(crate) trait IntoJs<T> {
    fn into_js(self) -> Result<T>;
}

pub(crate) trait FromJs<T> {
    fn from_js(js_value: T) -> Result<Self>
    where
        Self: Sized;
}

pub(crate) trait IntoRust<T> {
    fn into_rust(self) -> Result<T>;
}

pub(crate) trait IntoProgramOrJs<T> {
    fn into_program_or_js(self, env: Env, this: Reference<ClvmAllocator>) -> Result<T>;
}

impl<T, U> IntoRust<U> for T
where
    U: FromJs<T>,
{
    fn into_rust(self) -> Result<U> {
        U::from_js(self)
    }
}

impl<T, U> IntoProgramOrJs<T> for U
where
    U: IntoJs<T>,
{
    fn into_program_or_js(self, _env: Env, _this: Reference<ClvmAllocator>) -> Result<T> {
        self.into_js()
    }
}

impl IntoProgramOrJs<ClassInstance<Program>> for NodePtr {
    fn into_program_or_js(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
    ) -> Result<ClassInstance<Program>> {
        Program::new(this, self).into_instance(env)
    }
}

impl IntoProgramOrJs<Vec<ClassInstance<Program>>> for Vec<NodePtr> {
    fn into_program_or_js(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
    ) -> Result<Vec<ClassInstance<Program>>> {
        let mut result = Vec::with_capacity(self.len());

        for ptr in self {
            result.push(Program::new(this.clone(env)?, ptr).into_instance(env)?);
        }

        Ok(result)
    }
}

macro_rules! impl_primitive {
    ( $( $ty:ty ),* ) => {
        $( impl FromJs<$ty> for $ty {
            fn from_js(value: $ty) -> Result<Self> {
                Ok(value)
            }
        }

        impl IntoJs<$ty> for $ty {
            fn into_js(self) -> Result<Self> {
                Ok(self)
            }
        } )*
    };
}

impl_primitive!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64);

impl<F, T> FromJs<Vec<F>> for Vec<T>
where
    T: FromJs<F>,
{
    fn from_js(js_value: Vec<F>) -> Result<Self> {
        js_value.into_iter().map(FromJs::from_js).collect()
    }
}

impl<F, T> IntoJs<Vec<T>> for Vec<F>
where
    F: IntoJs<T>,
{
    fn into_js(self) -> Result<Vec<T>> {
        self.into_iter().map(IntoJs::into_js).collect()
    }
}

impl FromJs<ClassInstance<crate::Program>> for NodePtr {
    fn from_js(program: ClassInstance<crate::Program>) -> Result<Self> {
        Ok(program.ptr)
    }
}

impl<const N: usize> IntoJs<Uint8Array> for BytesImpl<N> {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_vec()))
    }
}

impl<const N: usize> FromJs<Uint8Array> for BytesImpl<N> {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        Ok(Self::new(js_value.to_vec().try_into().map_err(
            |bytes: Vec<u8>| {
                Error::from_reason(format!("Expected length {N}, found {}", bytes.len()))
            },
        )?))
    }
}

impl<const N: usize> IntoJs<Uint8Array> for [u8; N] {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_vec()))
    }
}

impl<const N: usize> FromJs<Uint8Array> for [u8; N] {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        js_value.to_vec().try_into().map_err(|bytes: Vec<u8>| {
            Error::from_reason(format!("Expected length {N}, found {}", bytes.len()))
        })
    }
}

impl IntoJs<Uint8Array> for Vec<u8> {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self))
    }
}

impl FromJs<Uint8Array> for Vec<u8> {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        Ok(js_value.to_vec())
    }
}

impl IntoJs<Uint8Array> for Bytes {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_vec()))
    }
}

impl FromJs<Uint8Array> for Bytes {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        Ok(Bytes::from(js_value.to_vec()))
    }
}

impl IntoJs<Uint8Array> for PublicKey {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_bytes().to_vec()))
    }
}

impl FromJs<Uint8Array> for PublicKey {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        PublicKey::from_bytes(&js_value.into_rust()?)
            .map_err(|error| Error::from_reason(error.to_string()))
    }
}

impl IntoJs<Uint8Array> for chia::protocol::Program {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_vec()))
    }
}

impl FromJs<Uint8Array> for chia::protocol::Program {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        Ok(chia::protocol::Program::from(js_value.to_vec()))
    }
}

impl IntoJs<BigInt> for u64 {
    fn into_js(self) -> Result<BigInt> {
        Ok(BigInt::from(self))
    }
}

impl FromJs<BigInt> for u64 {
    fn from_js(js_value: BigInt) -> Result<Self> {
        let (signed, value, lossless) = js_value.get_u64();

        if signed || !lossless {
            return Err(Error::from_reason("Expected u64"));
        }

        Ok(value)
    }
}

impl FromJs<BigInt> for num_bigint::BigInt {
    fn from_js(num: BigInt) -> Result<Self> {
        if num.words.is_empty() {
            return Ok(num_bigint::BigInt::ZERO);
        }

        // Convert u64 words into a big-endian byte array
        let bytes = words_to_bytes(&num.words);

        // Create the BigInt from the bytes
        let bigint = num_bigint::BigInt::from_bytes_be(
            if num.sign_bit {
                num_bigint::Sign::Minus
            } else {
                num_bigint::Sign::Plus
            },
            &bytes,
        );

        Ok(bigint)
    }
}

impl IntoJs<BigInt> for num_bigint::BigInt {
    fn into_js(self) -> Result<BigInt> {
        let (sign, bytes) = self.to_bytes_be();

        // Convert the byte array into u64 words
        let words = bytes_to_words(&bytes);

        Ok(BigInt {
            sign_bit: sign == num_bigint::Sign::Minus,
            words,
        })
    }
}

/// Helper function to convert Vec<u64> (words) into Vec<u8> (byte array)
fn words_to_bytes(words: &[u64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * 8);
    for word in words {
        bytes.extend_from_slice(&word.to_be_bytes());
    }

    // Remove leading zeros from the byte array
    while let Some(0) = bytes.first() {
        bytes.remove(0);
    }

    bytes
}

/// Helper function to convert Vec<u8> (byte array) into Vec<u64> (words)
fn bytes_to_words(bytes: &[u8]) -> Vec<u64> {
    let mut padded_bytes = vec![0u8; (8 - bytes.len() % 8) % 8];
    padded_bytes.extend_from_slice(bytes);

    let mut words = Vec::with_capacity(padded_bytes.len() / 8);

    for chunk in padded_bytes.chunks(8) {
        let word = u64::from_be_bytes(chunk.try_into().unwrap());
        words.push(word);
    }

    words
}
