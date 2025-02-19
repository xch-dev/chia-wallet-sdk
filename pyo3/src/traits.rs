use std::sync::Arc;

use chia_sdk_bindings::{
    AddressInfo, Bytes, BytesImpl, Cat, Clvm, Coin, CoinSpend, Error, LineageProof, Memos, Program,
    Result,
};
use clvmr::NodePtr;
use parking_lot::RwLock;

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

impl<T, R> IntoPy for Vec<R>
where
    R: IntoPy<Py = T>,
{
    type Py = Vec<T>;

    fn py(self) -> Result<Self::Py> {
        self.into_iter().map(IntoPy::py).collect::<Result<Vec<_>>>()
    }
}

impl IntoRust<chia_bls::PublicKey> for crate::PublicKey {
    fn rust(self) -> Result<chia_bls::PublicKey> {
        Ok(self.0 .0)
    }
}

impl IntoRust<NodePtr> for crate::Program {
    fn rust(self) -> Result<NodePtr> {
        Ok(self.node_ptr)
    }
}

impl IntoRust<u64> for u64 {
    fn rust(self) -> Result<u64> {
        Ok(self)
    }
}

impl IntoPy for u64 {
    type Py = u64;

    fn py(self) -> Result<Self::Py> {
        Ok(self)
    }
}

impl IntoRust<u32> for u32 {
    fn rust(self) -> Result<u32> {
        Ok(self)
    }
}

impl IntoPy for u32 {
    type Py = u32;

    fn py(self) -> Result<Self::Py> {
        Ok(self)
    }
}

impl IntoRust<u8> for u8 {
    fn rust(self) -> Result<u8> {
        Ok(self)
    }
}

impl IntoPy for u8 {
    type Py = u8;

    fn py(self) -> Result<Self::Py> {
        Ok(self)
    }
}

pub trait IntoPyWithClvm {
    type Py;

    fn py_with_clvm(self, clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py>;
}

impl<T, R> IntoPyWithClvm for R
where
    R: IntoPy<Py = T>,
{
    type Py = T;

    fn py_with_clvm(self, _clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py> {
        self.py()
    }
}

impl IntoPyWithClvm for NodePtr {
    type Py = crate::Program;

    fn py_with_clvm(self, clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py> {
        Ok(crate::Program {
            clvm: clvm.clone(),
            node_ptr: self,
        })
    }
}

impl IntoPyWithClvm for chia_bls::PublicKey {
    type Py = crate::PublicKey;

    fn py_with_clvm(self, _clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py> {
        Ok(crate::PublicKey(chia_sdk_bindings::PublicKey(self)))
    }
}

impl IntoPyWithClvm for Vec<NodePtr> {
    type Py = Vec<crate::Program>;

    fn py_with_clvm(self, clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py> {
        self.into_iter()
            .map(|node_ptr| {
                Ok(crate::Program {
                    clvm: clvm.clone(),
                    node_ptr,
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

impl IntoPyWithClvm for Option<NodePtr> {
    type Py = Option<crate::Program>;

    fn py_with_clvm(self, clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py> {
        let Some(node_ptr) = self else {
            return Ok(None);
        };

        Ok(Some(crate::Program {
            clvm: clvm.clone(),
            node_ptr,
        }))
    }
}

impl IntoPyWithClvm for Option<Memos<NodePtr>> {
    type Py = Option<crate::Program>;

    fn py_with_clvm(self, clvm: &Arc<RwLock<Clvm>>) -> Result<Self::Py> {
        let Some(memos) = self else {
            return Ok(None);
        };

        Ok(Some(crate::Program {
            clvm: clvm.clone(),
            node_ptr: memos.value,
        }))
    }
}

impl IntoRust<Memos<NodePtr>> for crate::Program {
    fn rust(self) -> Result<Memos<NodePtr>> {
        Ok(Memos::new(self.node_ptr))
    }
}
