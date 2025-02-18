use chia_sdk_bindings::{
    AddressInfo, Bytes, BytesImpl, Cat, Coin, CoinSpend, Error, LineageProof, Program, Result,
};

pub trait IntoRust<T> {
    fn rust(self) -> Result<T>;
}

pub trait IntoPy {
    type Py;

    fn py(self) -> Result<Self::Py>;
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

impl<const N: usize> IntoPy for BytesImpl<N> {
    type Py = Vec<u8>;

    fn py(self) -> Result<Self::Py> {
        Ok(self.into())
    }
}

impl IntoRust<Bytes> for Vec<u8> {
    fn rust(self) -> Result<Bytes> {
        Ok(Bytes::new(self))
    }
}

impl IntoPy for Bytes {
    type Py = Vec<u8>;

    fn py(self) -> Result<Self::Py> {
        Ok(self.into())
    }
}

impl IntoRust<Program> for Vec<u8> {
    fn rust(self) -> Result<Program> {
        Ok(Program::from(self))
    }
}

impl IntoPy for Program {
    type Py = Vec<u8>;

    fn py(self) -> Result<Self::Py> {
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

impl IntoPy for AddressInfo {
    type Py = crate::AddressInfo;

    fn py(self) -> Result<Self::Py> {
        Ok(Self::Py {
            puzzle_hash: self.puzzle_hash.py()?,
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

impl IntoPy for CoinSpend {
    type Py = crate::CoinSpend;

    fn py(self) -> Result<Self::Py> {
        Ok(Self::Py {
            coin: self.coin.py()?,
            puzzle_reveal: self.puzzle_reveal.py()?,
            solution: self.solution.py()?,
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

impl IntoPy for Coin {
    type Py = crate::Coin;

    fn py(self) -> Result<Self::Py> {
        Ok(Self::Py {
            parent_coin_info: self.parent_coin_info.py()?,
            puzzle_hash: self.puzzle_hash.py()?,
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

impl IntoPy for Cat {
    type Py = crate::Cat;

    fn py(self) -> Result<Self::Py> {
        Ok(Self::Py {
            coin: self.coin.py()?,
            lineage_proof: self.lineage_proof.py()?,
            asset_id: self.asset_id.py()?,
            p2_puzzle_hash: self.p2_puzzle_hash.py()?,
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

impl IntoPy for LineageProof {
    type Py = crate::LineageProof;

    fn py(self) -> Result<Self::Py> {
        Ok(Self::Py {
            parent_parent_coin_info: self.parent_parent_coin_info.py()?,
            parent_inner_puzzle_hash: Some(self.parent_inner_puzzle_hash.py()?),
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

impl<T, R> IntoPy for Option<R>
where
    R: IntoPy<Py = T>,
{
    type Py = Option<T>;

    fn py(self) -> Result<Self::Py> {
        self.map(IntoPy::py).transpose()
    }
}
