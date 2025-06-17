use chia_protocol::{Bytes, BytesImpl, ClassgroupElement, Program};
use clvm_utils::TreeHash;
use napi::bindgen_prelude::*;

use crate::{Error, FromRust, IntoRust, Result};

impl From<Error> for napi::Error {
    fn from(value: Error) -> Self {
        napi::Error::from_reason(value.to_string())
    }
}

pub struct Napi;

pub struct NapiParamContext;

pub struct NapiStructContext(pub Env);

pub struct NapiReturnContext(pub Env);

pub struct NapiAsyncReturnContext;

impl<T, U> IntoRust<T, NapiParamContext, Napi> for ClassInstance<U>
where
    U: Clone + IntoRust<T, NapiParamContext, Napi>,
{
    fn into_rust(self, context: &NapiParamContext) -> Result<T> {
        std::ops::Deref::deref(&self).clone().into_rust(context)
    }
}

impl<T, U> IntoRust<T, NapiParamContext, Napi> for Reference<U>
where
    U: Clone + IntoRust<T, NapiParamContext, Napi>,
{
    fn into_rust(self, context: &NapiParamContext) -> Result<T> {
        std::ops::Deref::deref(&self).clone().into_rust(context)
    }
}

impl FromRust<(), NapiReturnContext, Napi> for napi::JsUndefined {
    fn from_rust(_value: (), context: &NapiReturnContext) -> Result<Self> {
        Ok(context.0.get_undefined()?)
    }
}

trait ArrayBuffer: From<Vec<u8>> {
    fn generic_to_vec(&self) -> Vec<u8>;
}

impl ArrayBuffer for Uint8Array {
    fn generic_to_vec(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl ArrayBuffer for Buffer {
    fn generic_to_vec(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl<T, A> FromRust<Vec<u8>, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn from_rust(value: Vec<u8>, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T, A> IntoRust<Vec<u8>, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn into_rust(self, _context: &T) -> Result<Vec<u8>> {
        Ok(self.generic_to_vec())
    }
}

impl<T, const N: usize, A> FromRust<BytesImpl<N>, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec().into())
    }
}

impl<T, const N: usize, A> IntoRust<BytesImpl<N>, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn into_rust(self, _context: &T) -> Result<BytesImpl<N>> {
        let bytes = self.generic_to_vec();

        if bytes.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: bytes.len(),
            });
        }

        Ok(bytes.try_into().unwrap())
    }
}

impl<T, A> FromRust<ClassgroupElement, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn from_rust(value: ClassgroupElement, _context: &T) -> Result<Self> {
        Ok(value.data.to_vec().into())
    }
}

impl<T, A> IntoRust<ClassgroupElement, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn into_rust(self, _context: &T) -> Result<ClassgroupElement> {
        let bytes = self.generic_to_vec();

        if bytes.len() != 100 {
            return Err(Error::WrongLength {
                expected: 100,
                found: bytes.len(),
            });
        }

        Ok(ClassgroupElement::new(bytes.try_into().unwrap()))
    }
}

impl<T, A> FromRust<TreeHash, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
        Ok(value.to_vec().into())
    }
}

impl<T, A> IntoRust<TreeHash, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn into_rust(self, _context: &T) -> Result<TreeHash> {
        let bytes = self.generic_to_vec();

        if bytes.len() != 32 {
            return Err(Error::WrongLength {
                expected: 32,
                found: bytes.len(),
            });
        }

        Ok(TreeHash::new(bytes.try_into().unwrap()))
    }
}

impl<T, A> FromRust<Bytes, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn from_rust(value: Bytes, _context: &T) -> Result<Self> {
        Ok(value.to_vec().into())
    }
}

impl<T, A> IntoRust<Bytes, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn into_rust(self, _context: &T) -> Result<Bytes> {
        Ok(self.generic_to_vec().into())
    }
}

impl<T, A> FromRust<Program, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn from_rust(value: Program, _context: &T) -> Result<Self> {
        Ok(value.to_vec().into())
    }
}

impl<T, A> IntoRust<Program, T, Napi> for A
where
    A: ArrayBuffer,
{
    fn into_rust(self, _context: &T) -> Result<Program> {
        Ok(self.generic_to_vec().into())
    }
}

impl<T> IntoRust<num_bigint::BigInt, T, Napi> for BigInt {
    fn into_rust(self, _context: &T) -> Result<num_bigint::BigInt> {
        if self.words.is_empty() {
            return Ok(num_bigint::BigInt::ZERO);
        }

        // Convert u64 words into a big-endian byte array
        let bytes = words_to_bytes(&self.words);

        // Create the BigInt from the bytes
        let bigint = num_bigint::BigInt::from_bytes_be(
            if self.sign_bit {
                num_bigint::Sign::Minus
            } else {
                num_bigint::Sign::Plus
            },
            &bytes,
        );

        Ok(bigint)
    }
}

impl<T> FromRust<num_bigint::BigInt, T, Napi> for BigInt {
    fn from_rust(value: num_bigint::BigInt, _context: &T) -> Result<Self> {
        let (sign, bytes) = value.to_bytes_be();

        // Convert the byte array into u64 words
        let words = bytes_to_words(&bytes);

        Ok(BigInt {
            sign_bit: sign == num_bigint::Sign::Minus,
            words,
        })
    }
}

impl<T> IntoRust<u64, T, Napi> for BigInt {
    fn into_rust(self, _context: &T) -> Result<u64> {
        let bigint: num_bigint::BigInt = self.into_rust(_context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u64, T, Napi> for BigInt {
    fn from_rust(value: u64, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T> IntoRust<u128, T, Napi> for BigInt {
    fn into_rust(self, _context: &T) -> Result<u128> {
        let bigint: num_bigint::BigInt = self.into_rust(_context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u128, T, Napi> for BigInt {
    fn from_rust(value: u128, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

fn words_to_bytes(words: &[u64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * 8);
    for word in words {
        bytes.extend_from_slice(&word.to_be_bytes());
    }

    while let Some(0) = bytes.first() {
        bytes.remove(0);
    }

    bytes
}

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
