use bindy::{Error, Result};
use chia_protocol::Bytes32;
use chia_puzzle_types::{EveProof as EveProofRs, LineageProof, Proof as ProofRs};

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
pub struct Proof {
    pub parent_parent_coin_info: Bytes32,
    pub parent_inner_puzzle_hash: Option<Bytes32>,
    pub parent_amount: u64,
}

impl Proof {
    pub fn to_lineage_proof(&self) -> Result<Option<LineageProof>> {
        Ok(self.clone().try_into().ok())
    }
}

impl TryFrom<Proof> for LineageProof {
    type Error = Error;

    fn try_from(value: Proof) -> Result<Self> {
        Ok(Self {
            parent_parent_coin_info: value.parent_parent_coin_info,
            parent_inner_puzzle_hash: value
                .parent_inner_puzzle_hash
                .ok_or(Error::MissingParentInnerPuzzleHash)?,
            parent_amount: value.parent_amount,
        })
    }
}

impl From<LineageProof> for Proof {
    fn from(value: LineageProof) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info,
            parent_inner_puzzle_hash: Some(value.parent_inner_puzzle_hash),
            parent_amount: value.parent_amount,
        }
    }
}

impl From<Proof> for ProofRs {
    fn from(value: Proof) -> Self {
        if let Some(parent_inner_puzzle_hash) = value.parent_inner_puzzle_hash {
            Self::Lineage(chia_puzzle_types::LineageProof {
                parent_parent_coin_info: value.parent_parent_coin_info,
                parent_inner_puzzle_hash,
                parent_amount: value.parent_amount,
            })
        } else {
            Self::Eve(EveProofRs {
                parent_parent_coin_info: value.parent_parent_coin_info,
                parent_amount: value.parent_amount,
            })
        }
    }
}

impl From<ProofRs> for Proof {
    fn from(value: ProofRs) -> Self {
        match value {
            ProofRs::Lineage(proof) => Self {
                parent_parent_coin_info: proof.parent_parent_coin_info,
                parent_inner_puzzle_hash: Some(proof.parent_inner_puzzle_hash),
                parent_amount: proof.parent_amount,
            },
            ProofRs::Eve(proof) => Self {
                parent_parent_coin_info: proof.parent_parent_coin_info,
                parent_inner_puzzle_hash: None,
                parent_amount: proof.parent_amount,
            },
        }
    }
}

pub trait LineageProofExt {
    fn to_proof(&self) -> Result<Proof>;
}

impl LineageProofExt for LineageProof {
    fn to_proof(&self) -> Result<Proof> {
        Ok((*self).into())
    }
}
