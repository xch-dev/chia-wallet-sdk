use chia_sdk_bindings::{AddressInfo, Bytes, BytesImpl, Coin, CoinSpend, Error, Program, Result};
use napi::bindgen_prelude::{BigInt, Uint8Array};

pub trait IntoRust<T> {
    fn rust(self) -> Result<T>;
}

pub trait IntoJs {
    type Js;

    fn js(self) -> Result<Self::Js>;
}

impl<const N: usize> IntoRust<BytesImpl<N>> for Uint8Array {
    fn rust(self) -> Result<BytesImpl<N>> {
        if self.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: self.len(),
            });
        }
        Ok(BytesImpl::new(self.as_ref().try_into().unwrap()))
    }
}

impl<const N: usize> IntoJs for BytesImpl<N> {
    type Js = Uint8Array;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<Bytes> for Uint8Array {
    fn rust(self) -> Result<Bytes> {
        Ok(Bytes::new(self.to_vec()))
    }
}

impl IntoJs for Bytes {
    type Js = Uint8Array;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<Program> for Uint8Array {
    fn rust(self) -> Result<Program> {
        Ok(Program::new(self.to_vec().into()))
    }
}

impl IntoJs for Program {
    type Js = Uint8Array;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<AddressInfo> for crate::AddressInfo {
    fn rust(self) -> Result<AddressInfo> {
        Ok(AddressInfo {
            puzzle_hash: self.puzzle_hash.rust()?,
            prefix: self.prefix,
        })
    }
}

impl IntoJs for AddressInfo {
    type Js = crate::AddressInfo;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            puzzle_hash: self.puzzle_hash.js()?,
            prefix: self.prefix,
        })
    }
}

impl IntoRust<CoinSpend> for crate::CoinSpend {
    fn rust(self) -> Result<CoinSpend> {
        Ok(CoinSpend {
            coin: self.coin.rust()?,
            puzzle_reveal: self.puzzle_reveal.rust()?,
            solution: self.solution.rust()?,
        })
    }
}

impl IntoJs for CoinSpend {
    type Js = crate::CoinSpend;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            coin: self.coin.js()?,
            puzzle_reveal: self.puzzle_reveal.js()?,
            solution: self.solution.js()?,
        })
    }
}

impl IntoRust<Coin> for crate::Coin {
    fn rust(self) -> Result<Coin> {
        Ok(Coin {
            parent_coin_info: self.parent_coin_info.rust()?,
            puzzle_hash: self.puzzle_hash.rust()?,
            amount: self.amount.rust()?,
        })
    }
}

impl IntoJs for Coin {
    type Js = crate::Coin;

    fn js(self) -> Result<Self::Js> {
        let amount: num_bigint::BigInt = self.amount.into();

        Ok(Self::Js {
            parent_coin_info: self.parent_coin_info.js()?,
            puzzle_hash: self.puzzle_hash.js()?,
            amount: amount.js()?,
        })
    }
}

impl IntoRust<num_bigint::BigInt> for BigInt {
    fn rust(self) -> Result<num_bigint::BigInt> {
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

impl IntoJs for num_bigint::BigInt {
    type Js = BigInt;

    fn js(self) -> Result<BigInt> {
        let (sign, bytes) = self.to_bytes_be();

        // Convert the byte array into u64 words
        let words = bytes_to_words(&bytes);

        Ok(BigInt {
            sign_bit: sign == num_bigint::Sign::Minus,
            words,
        })
    }
}

impl IntoRust<u64> for BigInt {
    fn rust(self) -> Result<u64> {
        let bigint: num_bigint::BigInt = self.rust()?;
        Ok(bigint.try_into()?)
    }
}

impl IntoJs for u64 {
    type Js = BigInt;

    fn js(self) -> Result<BigInt> {
        Ok(self.into())
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
