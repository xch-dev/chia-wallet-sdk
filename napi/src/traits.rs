use std::fmt::Display;

use chia::{
    bls,
    clvm_utils::TreeHash,
    protocol::{Bytes, BytesImpl},
    secp,
};
use chia_wallet_sdk::Memos;
use clvmr::NodePtr;
use napi::bindgen_prelude::*;

use crate::{
    ClvmAllocator, K1PublicKey, K1SecretKey, K1Signature, Program, PublicKey, R1PublicKey,
    R1SecretKey, R1Signature, SecretKey,
};

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

pub(crate) trait FromRust<T> {
    fn from_rust(rust_value: T) -> Result<Self>
    where
        Self: Sized;
}

pub(crate) trait IntoJsContextual<T> {
    fn into_js_contextual(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
        clvm_allocator: &mut ClvmAllocator,
    ) -> Result<T>;
}

impl<T, U> IntoRust<U> for T
where
    U: FromJs<T>,
{
    fn into_rust(self) -> Result<U> {
        U::from_js(self)
    }
}

impl<T, U> FromRust<U> for T
where
    U: IntoJs<T>,
{
    fn from_rust(rust_value: U) -> Result<T> {
        rust_value.into_js()
    }
}

impl<T, U> IntoJsContextual<T> for U
where
    U: IntoJs<T>,
{
    fn into_js_contextual(
        self,
        _env: Env,
        _this: Reference<ClvmAllocator>,
        _clvm_allocator: &mut ClvmAllocator,
    ) -> Result<T> {
        self.into_js()
    }
}

impl IntoJsContextual<ClassInstance<Program>> for NodePtr {
    fn into_js_contextual(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
        _clvm_allocator: &mut ClvmAllocator,
    ) -> Result<ClassInstance<Program>> {
        Program::new(this, self).into_instance(env)
    }
}

impl IntoJsContextual<Vec<ClassInstance<Program>>> for Vec<NodePtr> {
    fn into_js_contextual(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
        _clvm_allocator: &mut ClvmAllocator,
    ) -> Result<Vec<ClassInstance<Program>>> {
        let mut result = Vec::with_capacity(self.len());

        for ptr in self {
            result.push(Program::new(this.clone(env)?, ptr).into_instance(env)?);
        }

        Ok(result)
    }
}

impl IntoJsContextual<Option<ClassInstance<Program>>> for Option<Memos<NodePtr>> {
    fn into_js_contextual(
        self,
        env: Env,
        this: Reference<ClvmAllocator>,
        clvm_allocator: &mut ClvmAllocator,
    ) -> Result<Option<ClassInstance<Program>>> {
        let Some(memos) = self else {
            return Ok(None);
        };

        let ptr = clvm_allocator.0.alloc(&memos.value).map_err(js_err)?;

        Ok(Some(Program::new(this, ptr).into_instance(env)?))
    }
}

impl IntoJsContextual<ClassInstance<PublicKey>> for bls::PublicKey {
    fn into_js_contextual(
        self,
        env: Env,
        _this: Reference<ClvmAllocator>,
        _clvm_allocator: &mut ClvmAllocator,
    ) -> Result<ClassInstance<PublicKey>> {
        PublicKey(self).into_instance(env)
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

impl FromJs<ClassInstance<Program>> for NodePtr {
    fn from_js(program: ClassInstance<Program>) -> Result<Self> {
        Ok(program.ptr)
    }
}

impl FromJs<ClassInstance<PublicKey>> for bls::PublicKey {
    fn from_js(program: ClassInstance<PublicKey>) -> Result<Self> {
        Ok(program.0)
    }
}

impl IntoJs<Uint8Array> for TreeHash {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_vec()))
    }
}

impl FromJs<Uint8Array> for TreeHash {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        Ok(Self::new(js_value.to_vec().try_into().map_err(
            |bytes: Vec<u8>| {
                Error::from_reason(format!("Expected length 32, found {}", bytes.len()))
            },
        )?))
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

impl IntoJs<Uint8Array> for bls::PublicKey {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_bytes().to_vec()))
    }
}

impl FromJs<Uint8Array> for bls::PublicKey {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        bls::PublicKey::from_bytes(&js_value.into_rust()?).map_err(js_err)
    }
}

impl IntoJs<Uint8Array> for secp::K1PublicKey {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_bytes().to_vec()))
    }
}

impl FromJs<Uint8Array> for secp::K1PublicKey {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        secp::K1PublicKey::from_bytes(&js_value.into_rust()?).map_err(js_err)
    }
}

impl IntoJs<Uint8Array> for secp::R1PublicKey {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_bytes().to_vec()))
    }
}

impl FromJs<Uint8Array> for secp::R1PublicKey {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        secp::R1PublicKey::from_bytes(&js_value.into_rust()?).map_err(js_err)
    }
}

impl IntoJs<Uint8Array> for secp::K1Signature {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_bytes().to_vec()))
    }
}

impl FromJs<Uint8Array> for secp::K1Signature {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        secp::K1Signature::from_bytes(&js_value.into_rust()?).map_err(js_err)
    }
}

impl IntoJs<Uint8Array> for secp::R1Signature {
    fn into_js(self) -> Result<Uint8Array> {
        Ok(Uint8Array::new(self.to_bytes().to_vec()))
    }
}

impl FromJs<Uint8Array> for secp::R1Signature {
    fn from_js(js_value: Uint8Array) -> Result<Self> {
        secp::R1Signature::from_bytes(&js_value.into_rust()?).map_err(js_err)
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

impl<T> FromJs<Option<T>> for Option<T>
where
    T: FromJs<T>,
{
    fn from_js(js_value: Option<T>) -> Result<Self> {
        js_value.map(FromJs::from_js).transpose()
    }
}

impl FromJs<Program> for NodePtr {
    fn from_js(program: Program) -> Result<Self> {
        Ok(program.ptr)
    }
}

impl FromJs<Option<ClassInstance<Program>>> for Option<Memos<NodePtr>> {
    fn from_js(js_value: Option<ClassInstance<Program>>) -> Result<Self>
    where
        Self: Sized,
    {
        let Some(program) = js_value else {
            return Ok(None);
        };
        Ok(Some(Memos::new(program.ptr)))
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

impl IntoJs<PublicKey> for bls::PublicKey {
    fn into_js(self) -> Result<PublicKey> {
        Ok(PublicKey(self))
    }
}

impl FromJs<PublicKey> for bls::PublicKey {
    fn from_js(js_value: PublicKey) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<SecretKey> for bls::SecretKey {
    fn into_js(self) -> Result<SecretKey> {
        Ok(SecretKey(self))
    }
}

impl FromJs<SecretKey> for bls::SecretKey {
    fn from_js(js_value: SecretKey) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<K1SecretKey> for secp::K1SecretKey {
    fn into_js(self) -> Result<K1SecretKey> {
        Ok(K1SecretKey(self))
    }
}

impl FromJs<K1SecretKey> for secp::K1SecretKey {
    fn from_js(js_value: K1SecretKey) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<R1SecretKey> for secp::R1SecretKey {
    fn into_js(self) -> Result<R1SecretKey> {
        Ok(R1SecretKey(self))
    }
}

impl FromJs<R1SecretKey> for secp::R1SecretKey {
    fn from_js(js_value: R1SecretKey) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<K1PublicKey> for secp::K1PublicKey {
    fn into_js(self) -> Result<K1PublicKey> {
        Ok(K1PublicKey(self))
    }
}

impl FromJs<K1PublicKey> for secp::K1PublicKey {
    fn from_js(js_value: K1PublicKey) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<R1PublicKey> for secp::R1PublicKey {
    fn into_js(self) -> Result<R1PublicKey> {
        Ok(R1PublicKey(self))
    }
}

impl FromJs<R1PublicKey> for secp::R1PublicKey {
    fn from_js(js_value: R1PublicKey) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<K1Signature> for secp::K1Signature {
    fn into_js(self) -> Result<K1Signature> {
        Ok(K1Signature(self))
    }
}

impl FromJs<K1Signature> for secp::K1Signature {
    fn from_js(js_value: K1Signature) -> Result<Self> {
        Ok(js_value.0)
    }
}

impl IntoJs<R1Signature> for secp::R1Signature {
    fn into_js(self) -> Result<R1Signature> {
        Ok(R1Signature(self))
    }
}

impl FromJs<R1Signature> for secp::R1Signature {
    fn from_js(js_value: R1Signature) -> Result<Self> {
        Ok(js_value.0)
    }
}

pub(crate) fn js_err(err: impl Display) -> Error {
    Error::from_reason(err.to_string())
}
