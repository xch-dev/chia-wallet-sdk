use chia_sdk_bindings::{
    AddressInfo, Bytes, BytesImpl, Cat, Coin, CoinSpend, Error, LineageProof, Memos, Program,
    Result,
};
use clvmr::NodePtr;
use napi::{
    bindgen_prelude::{BigInt, JavaScriptClassExt, Reference, Uint8Array},
    Env,
};

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

impl IntoRust<Cat> for crate::Cat {
    fn rust(self) -> Result<Cat> {
        Ok(Cat {
            coin: self.coin.rust()?,
            lineage_proof: self.lineage_proof.rust()?,
            asset_id: self.asset_id.rust()?,
            p2_puzzle_hash: self.p2_puzzle_hash.rust()?,
        })
    }
}

impl IntoJs for Cat {
    type Js = crate::Cat;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            coin: self.coin.js()?,
            lineage_proof: self.lineage_proof.js()?,
            asset_id: self.asset_id.js()?,
            p2_puzzle_hash: self.p2_puzzle_hash.js()?,
        })
    }
}

impl IntoRust<LineageProof> for crate::LineageProof {
    fn rust(self) -> Result<LineageProof> {
        Ok(LineageProof {
            parent_parent_coin_info: self.parent_parent_coin_info.rust()?,
            parent_inner_puzzle_hash: self
                .parent_inner_puzzle_hash
                .ok_or(Error::MissingParentInnerPuzzleHash)?
                .rust()?,
            parent_amount: self.parent_amount.rust()?,
        })
    }
}

impl IntoJs for LineageProof {
    type Js = crate::LineageProof;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            parent_parent_coin_info: self.parent_parent_coin_info.js()?,
            parent_inner_puzzle_hash: Some(self.parent_inner_puzzle_hash.js()?),
            parent_amount: self.parent_amount.js()?,
        })
    }
}

impl<T, R> IntoRust<Option<R>> for Option<T>
where
    T: IntoRust<R>,
{
    fn rust(self) -> Result<Option<R>> {
        self.map(IntoRust::rust).transpose()
    }
}

impl<T, R> IntoJs for Option<R>
where
    R: IntoJs<Js = T>,
{
    type Js = Option<T>;

    fn js(self) -> Result<Self::Js> {
        self.map(IntoJs::js).transpose()
    }
}

impl<T, R> IntoRust<Vec<R>> for Vec<T>
where
    T: IntoRust<R>,
{
    fn rust(self) -> Result<Vec<R>> {
        self.into_iter()
            .map(IntoRust::rust)
            .collect::<Result<Vec<_>>>()
    }
}

impl<T, R> IntoJs for Vec<R>
where
    R: IntoJs<Js = T>,
{
    type Js = Vec<T>;

    fn js(self) -> Result<Self::Js> {
        self.into_iter().map(IntoJs::js).collect::<Result<Vec<_>>>()
    }
}

impl IntoRust<chia_bls::PublicKey> for Reference<crate::PublicKey> {
    fn rust(self) -> Result<chia_bls::PublicKey> {
        Ok(self.0 .0)
    }
}

impl IntoRust<NodePtr> for Reference<crate::Program> {
    fn rust(self) -> Result<NodePtr> {
        Ok(self.node_ptr)
    }
}

impl IntoRust<Memos<NodePtr>> for Reference<crate::Program> {
    fn rust(self) -> Result<Memos<NodePtr>> {
        Ok(Memos::new(self.node_ptr))
    }
}

impl IntoRust<u32> for u32 {
    fn rust(self) -> Result<u32> {
        Ok(self)
    }
}

impl IntoJs for u32 {
    type Js = u32;

    fn js(self) -> Result<Self::Js> {
        Ok(self)
    }
}

impl IntoRust<u8> for u8 {
    fn rust(self) -> Result<u8> {
        Ok(self)
    }
}

impl IntoJs for u8 {
    type Js = u8;

    fn js(self) -> Result<Self::Js> {
        Ok(self)
    }
}

pub trait IntoJsWithClvm {
    type Js;

    fn js_with_clvm(self, env: Env, clvm: &Reference<crate::Clvm>) -> Result<Self::Js>;
}

impl<T, R> IntoJsWithClvm for R
where
    R: IntoJs<Js = T>,
{
    type Js = T;

    fn js_with_clvm(self, _env: Env, _clvm: &Reference<crate::Clvm>) -> Result<Self::Js> {
        self.js()
    }
}

impl IntoJsWithClvm for NodePtr {
    type Js = Reference<crate::Program>;

    fn js_with_clvm(self, env: Env, clvm: &Reference<crate::Clvm>) -> Result<Self::Js> {
        Ok(crate::Program {
            clvm: clvm.clone(env)?,
            node_ptr: self,
        }
        .into_reference(env)?)
    }
}

impl IntoJsWithClvm for Vec<NodePtr> {
    type Js = Vec<Reference<crate::Program>>;

    fn js_with_clvm(self, env: Env, clvm: &Reference<crate::Clvm>) -> Result<Self::Js> {
        self.into_iter()
            .map(|node_ptr| {
                Ok(crate::Program {
                    clvm: clvm.clone(env)?,
                    node_ptr,
                }
                .into_reference(env)?)
            })
            .collect::<Result<Vec<_>>>()
    }
}

impl IntoJsWithClvm for Option<Memos<NodePtr>> {
    type Js = Option<Reference<crate::Program>>;

    fn js_with_clvm(self, env: Env, clvm: &Reference<crate::Clvm>) -> Result<Self::Js> {
        let Some(memos) = self else {
            return Ok(None);
        };

        Ok(Some(
            crate::Program {
                clvm: clvm.clone(env)?,
                node_ptr: memos.value,
            }
            .into_reference(env)?,
        ))
    }
}

impl IntoJsWithClvm for chia_bls::PublicKey {
    type Js = Reference<crate::PublicKey>;

    fn js_with_clvm(self, env: Env, _clvm: &Reference<crate::Clvm>) -> Result<Self::Js> {
        Ok(crate::PublicKey(chia_sdk_bindings::PublicKey(self)).into_reference(env)?)
    }
}
