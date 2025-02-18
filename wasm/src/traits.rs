use chia_sdk_bindings::{
    AddressInfo, Bytes, BytesImpl, Cat, Coin, CoinSpend, Error, LineageProof, Program, Result,
};

pub trait IntoRust<T> {
    fn rust(self) -> Result<T>;
}

pub trait IntoJs {
    type Js;

    fn js(self) -> Result<Self::Js>;
}

impl<const N: usize> IntoRust<BytesImpl<N>> for Vec<u8> {
    fn rust(self) -> Result<BytesImpl<N>> {
        if self.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: self.len(),
            });
        }
        Ok(BytesImpl::new(self.try_into().unwrap()))
    }
}

impl<const N: usize> IntoJs for BytesImpl<N> {
    type Js = Vec<u8>;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<Bytes> for Vec<u8> {
    fn rust(self) -> Result<Bytes> {
        Ok(Bytes::new(self))
    }
}

impl IntoJs for Bytes {
    type Js = Vec<u8>;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<Program> for Vec<u8> {
    fn rust(self) -> Result<Program> {
        Ok(Program::from(self))
    }
}

impl IntoJs for Program {
    type Js = Vec<u8>;

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
            amount: self.amount,
        })
    }
}

impl IntoJs for Coin {
    type Js = crate::Coin;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            parent_coin_info: self.parent_coin_info.js()?,
            puzzle_hash: self.puzzle_hash.js()?,
            amount: self.amount,
        })
    }
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
                .ok_or(chia_sdk_bindings::Error::MissingParentInnerPuzzleHash)?
                .rust()?,
            parent_amount: self.parent_amount,
        })
    }
}

impl IntoJs for LineageProof {
    type Js = crate::LineageProof;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            parent_parent_coin_info: self.parent_parent_coin_info.js()?,
            parent_inner_puzzle_hash: Some(self.parent_inner_puzzle_hash.js()?),
            parent_amount: self.parent_amount,
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
