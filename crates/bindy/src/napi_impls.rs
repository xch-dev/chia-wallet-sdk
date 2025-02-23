use chia_protocol::BytesImpl;
use napi::bindgen_prelude::*;

use crate::{Error, FromRust, IntoRust, Result};

impl From<Error> for napi::Error {
    fn from(_value: Error) -> Self {
        napi::Error::new(napi::Status::GenericFailure, "error")
    }
}

pub struct NapiParamContext;

pub struct NapiStructContext(pub Env);

pub struct NapiReturnContext(pub Env);

impl<T> FromRust<T, NapiStructContext> for Reference<T>
where
    T: JavaScriptClassExt,
{
    fn from_rust(value: T, context: &NapiStructContext) -> Result<Self> {
        let env = context.0;
        Ok(value.into_reference(env)?)
    }
}

impl<T, U> IntoRust<T, NapiParamContext> for ClassInstance<'_, U>
where
    U: Clone + IntoRust<T, NapiParamContext>,
{
    fn into_rust(self, context: &NapiParamContext) -> Result<T> {
        std::ops::Deref::deref(&self).clone().into_rust(context)
    }
}

impl FromRust<(), NapiReturnContext> for napi::JsUndefined {
    fn from_rust(_value: (), context: &NapiReturnContext) -> Result<Self> {
        Ok(context.0.get_undefined()?)
    }
}

impl<T> FromRust<Vec<u8>, T> for Uint8Array {
    fn from_rust(value: Vec<u8>, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T> IntoRust<Vec<u8>, T> for Uint8Array {
    fn into_rust(self, _context: &T) -> Result<Vec<u8>> {
        Ok(self.to_vec())
    }
}

impl<T, const N: usize> FromRust<BytesImpl<N>, T> for Uint8Array {
    fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
        Ok(value.to_vec().into())
    }
}

impl<T, const N: usize> IntoRust<BytesImpl<N>, T> for Uint8Array {
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

impl<T> IntoRust<num_bigint::BigInt, T> for BigInt {
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

impl<T> FromRust<num_bigint::BigInt, T> for BigInt {
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

impl<T> IntoRust<u64, T> for BigInt {
    fn into_rust(self, _context: &T) -> Result<u64> {
        let bigint: num_bigint::BigInt = self.into_rust(_context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u64, T> for BigInt {
    fn from_rust(value: u64, _context: &T) -> Result<Self> {
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
