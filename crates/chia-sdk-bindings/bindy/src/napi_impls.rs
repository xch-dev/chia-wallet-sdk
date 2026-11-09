use chia_protocol::{Bytes, BytesImpl, ClassgroupElement, Program};
use clvm_utils::TreeHash;
use napi::bindgen_prelude::*;

use crate::{Error, FromRust, IntoRust, Result};

impl From<Error> for napi::Error {
    fn from(value: Error) -> Self {
        napi::Error::from_reason(value.to_string())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Napi;

#[derive(Debug, Clone, Copy)]
pub struct NapiParamContext;

#[derive(Clone, Copy)]
#[allow(missing_debug_implementations)]
pub struct NapiStructContext(pub Env);

#[derive(Clone, Copy)]
#[allow(missing_debug_implementations)]
pub struct NapiReturnContext(pub Env);

#[derive(Debug, Clone, Copy)]
pub struct NapiAsyncReturnContext;

impl<T, U> IntoRust<T, NapiParamContext, Napi> for ClassInstance<'_, U>
where
    U: Clone + IntoRust<T, NapiParamContext, Napi>,
{
    fn into_rust(self, context: &NapiParamContext) -> Result<T> {
        (*self).clone().into_rust(context)
    }
}

impl<T, U> IntoRust<T, NapiParamContext, Napi> for &'_ U
where
    U: Clone + IntoRust<T, NapiParamContext, Napi>,
{
    fn into_rust(self, context: &NapiParamContext) -> Result<T> {
        (*self).clone().into_rust(context)
    }
}

impl FromRust<(), NapiReturnContext, Napi> for () {
    fn from_rust(_value: (), _context: &NapiReturnContext) -> Result<Self> {
        Ok(())
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

macro_rules! impl_array_buffer {
    ($($ty:ty),*) => { $(
        impl<T> FromRust<Vec<u8>, T, Napi> for $ty {
            fn from_rust(value: Vec<u8>, _context: &T) -> Result<Self> {
                Ok(value.into())
            }
        }

        impl<T> IntoRust<Vec<u8>, T, Napi> for $ty {
            fn into_rust(self, _context: &T) -> Result<Vec<u8>> {
                Ok(self.generic_to_vec())
            }
        }

        impl<T, const N: usize> FromRust<BytesImpl<N>, T, Napi> for $ty {
            fn from_rust(value: BytesImpl<N>, _context: &T) -> Result<Self> {
                Ok(value.to_vec().into())
            }
        }

        impl<T, const N: usize> IntoRust<BytesImpl<N>, T, Napi> for $ty {
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

        impl<T> FromRust<ClassgroupElement, T, Napi> for $ty {
            fn from_rust(value: ClassgroupElement, _context: &T) -> Result<Self> {
                Ok(value.data.to_vec().into())
            }
        }

        impl<T> IntoRust<ClassgroupElement, T, Napi> for $ty {
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

        impl<T> FromRust<TreeHash, T, Napi> for $ty {
            fn from_rust(value: TreeHash, _context: &T) -> Result<Self> {
                Ok(value.to_vec().into())
            }
        }

        impl<T> IntoRust<TreeHash, T, Napi> for $ty {
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

        impl<T> FromRust<Bytes, T, Napi> for $ty {
            fn from_rust(value: Bytes, _context: &T) -> Result<Self> {
                Ok(value.to_vec().into())
            }
        }

        impl<T> IntoRust<Bytes, T, Napi> for $ty {
            fn into_rust(self, _context: &T) -> Result<Bytes> {
                Ok(self.generic_to_vec().into())
            }
        }

        impl<T> FromRust<Program, T, Napi> for $ty {
            fn from_rust(value: Program, _context: &T) -> Result<Self> {
                Ok(value.to_vec().into())
            }
        }

        impl<T> IntoRust<Program, T, Napi> for $ty {
            fn into_rust(self, _context: &T) -> Result<Program> {
                Ok(self.generic_to_vec().into())
            }
        }
    )* };
}

impl_array_buffer!(Uint8Array, Buffer);

impl<T> IntoRust<num_bigint::BigInt, T, Napi> for BigInt {
    fn into_rust(self, _context: &T) -> Result<num_bigint::BigInt> {
        // napi BigInt words are little-endian (least significant word first)
        let mut bytes = Vec::with_capacity(self.words.len() * 8);

        for word in &self.words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }

        let sign = if self.sign_bit {
            num_bigint::Sign::Minus
        } else {
            num_bigint::Sign::Plus
        };

        Ok(num_bigint::BigInt::from_bytes_le(sign, &bytes))
    }
}

impl<T> FromRust<num_bigint::BigInt, T, Napi> for BigInt {
    fn from_rust(value: num_bigint::BigInt, _context: &T) -> Result<Self> {
        // `to_u64_digits` returns little-endian words, which matches napi
        let (sign, mut words) = value.to_u64_digits();

        if words.is_empty() {
            words.push(0);
        }

        Ok(BigInt {
            sign_bit: sign == num_bigint::Sign::Minus,
            words,
        })
    }
}

impl<T> IntoRust<u64, T, Napi> for BigInt {
    fn into_rust(self, context: &T) -> Result<u64> {
        let bigint: num_bigint::BigInt = self.into_rust(context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u64, T, Napi> for BigInt {
    fn from_rust(value: u64, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T> IntoRust<i64, T, Napi> for BigInt {
    fn into_rust(self, context: &T) -> Result<i64> {
        let bigint: num_bigint::BigInt = self.into_rust(context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<i64, T, Napi> for BigInt {
    fn from_rust(value: i64, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}

impl<T> IntoRust<usize, T, Napi> for BigInt {
    fn into_rust(self, context: &T) -> Result<usize> {
        let bigint: num_bigint::BigInt = self.into_rust(context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<usize, T, Napi> for BigInt {
    fn from_rust(value: usize, _context: &T) -> Result<Self> {
        Ok((value as u64).into())
    }
}

impl<T> IntoRust<u128, T, Napi> for BigInt {
    fn into_rust(self, context: &T) -> Result<u128> {
        let bigint: num_bigint::BigInt = self.into_rust(context)?;
        Ok(bigint.try_into()?)
    }
}

impl<T> FromRust<u128, T, Napi> for BigInt {
    fn from_rust(value: u128, _context: &T) -> Result<Self> {
        Ok(value.into())
    }
}
