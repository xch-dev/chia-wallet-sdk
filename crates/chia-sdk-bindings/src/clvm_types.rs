use bindy::Error;
use chia_protocol::Bytes32;
use chia_puzzle_types::{EveProof, Proof};

use crate::Program;

#[derive(Clone)]
pub struct Spend {
    pub puzzle: Program,
    pub solution: Program,
}

impl From<Spend> for chia_sdk_driver::Spend {
    fn from(value: Spend) -> Self {
        Self {
            puzzle: value.puzzle.1,
            solution: value.solution.1,
        }
    }
}

#[derive(Clone)]
pub struct Output {
    pub value: Program,
    pub cost: u64,
}

#[derive(Clone)]
pub struct Pair {
    pub first: Program,
    pub rest: Program,
}

#[derive(Clone)]
pub struct CurriedProgram {
    pub program: Program,
    pub args: Vec<Program>,
}

#[derive(Clone)]
pub struct LineageProof {
    pub parent_parent_coin_info: Bytes32,
    pub parent_inner_puzzle_hash: Option<Bytes32>,
    pub parent_amount: u64,
}

impl TryFrom<LineageProof> for chia_puzzle_types::LineageProof {
    type Error = Error;

    fn try_from(value: LineageProof) -> Result<Self, Self::Error> {
        Ok(Self {
            parent_parent_coin_info: value.parent_parent_coin_info,
            parent_inner_puzzle_hash: value
                .parent_inner_puzzle_hash
                .ok_or(Error::MissingParentInnerPuzzleHash)?,
            parent_amount: value.parent_amount,
        })
    }
}

impl From<chia_puzzle_types::LineageProof> for LineageProof {
    fn from(value: chia_puzzle_types::LineageProof) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info,
            parent_inner_puzzle_hash: Some(value.parent_inner_puzzle_hash),
            parent_amount: value.parent_amount,
        }
    }
}

impl From<LineageProof> for Proof {
    fn from(value: LineageProof) -> Self {
        if let Some(parent_inner_puzzle_hash) = value.parent_inner_puzzle_hash {
            Self::Lineage(chia_puzzle_types::LineageProof {
                parent_parent_coin_info: value.parent_parent_coin_info,
                parent_inner_puzzle_hash,
                parent_amount: value.parent_amount,
            })
        } else {
            Self::Eve(EveProof {
                parent_parent_coin_info: value.parent_parent_coin_info,
                parent_amount: value.parent_amount,
            })
        }
    }
}

impl From<Proof> for LineageProof {
    fn from(value: Proof) -> Self {
        match value {
            Proof::Lineage(proof) => Self {
                parent_parent_coin_info: proof.parent_parent_coin_info,
                parent_inner_puzzle_hash: Some(proof.parent_inner_puzzle_hash),
                parent_amount: proof.parent_amount,
            },
            Proof::Eve(proof) => Self {
                parent_parent_coin_info: proof.parent_parent_coin_info,
                parent_inner_puzzle_hash: None,
                parent_amount: proof.parent_amount,
            },
        }
    }
}
