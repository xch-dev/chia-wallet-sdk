#![allow(deprecated)]

use pyo3::prelude::*;

use crate::bls::Signature;

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct Coin {
    pub parent_coin_info: Vec<u8>,
    pub puzzle_hash: Vec<u8>,
    pub amount: u64,
}

#[pymethods]
impl Coin {
    #[new]
    pub fn new(parent_coin_info: Vec<u8>, puzzle_hash: Vec<u8>, amount: u64) -> Self {
        Self {
            parent_coin_info,
            puzzle_hash,
            amount,
        }
    }
}

#[pyclass(frozen, get_all)]
pub struct CoinState {
    pub coin: Coin,
    pub spent_height: Option<u32>,
    pub created_height: Option<u32>,
}

#[pymethods]
impl CoinState {
    #[new]
    pub fn new(coin: Coin, spent_height: Option<u32>, created_height: Option<u32>) -> Self {
        Self {
            coin,
            spent_height,
            created_height,
        }
    }
}

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct CoinSpend {
    pub coin: Coin,
    pub puzzle_reveal: Vec<u8>,
    pub solution: Vec<u8>,
}

#[pymethods]
impl CoinSpend {
    #[new]
    pub fn new(coin: Coin, puzzle_reveal: Vec<u8>, solution: Vec<u8>) -> Self {
        Self {
            coin,
            puzzle_reveal,
            solution,
        }
    }
}

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct SpendBundle {
    pub coin_spends: Vec<CoinSpend>,
    pub aggregated_signature: Signature,
}

#[pymethods]
impl SpendBundle {
    #[new]
    pub fn new(coin_spends: Vec<CoinSpend>, aggregated_signature: Signature) -> Self {
        Self {
            coin_spends,
            aggregated_signature,
        }
    }
}

#[pyclass(frozen, get_all)]
#[derive(Clone)]
pub struct LineageProof {
    pub parent_parent_coin_info: Vec<u8>,
    pub parent_inner_puzzle_hash: Option<Vec<u8>>,
    pub parent_amount: u64,
}

#[pymethods]
impl LineageProof {
    #[new]
    #[pyo3(signature = (parent_parent_coin_info, parent_inner_puzzle_hash, parent_amount))]
    pub fn new(
        parent_parent_coin_info: Vec<u8>,
        parent_inner_puzzle_hash: Option<Vec<u8>>,
        parent_amount: u64,
    ) -> Self {
        Self {
            parent_parent_coin_info,
            parent_inner_puzzle_hash,
            parent_amount,
        }
    }
}
