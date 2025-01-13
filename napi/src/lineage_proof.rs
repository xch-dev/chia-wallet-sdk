use chia::puzzles;
use napi::bindgen_prelude::*;

use crate::traits::{FromJs, IntoJs, IntoRust};

#[napi(object)]
#[derive(Clone)]
pub struct LineageProof {
    pub parent_parent_coin_info: Uint8Array,
    pub parent_inner_puzzle_hash: Option<Uint8Array>,
    pub parent_amount: BigInt,
}

impl FromJs<LineageProof> for puzzles::Proof {
    fn from_js(value: LineageProof) -> Result<Self> {
        if let Some(parent_inner_puzzle_hash) = value.parent_inner_puzzle_hash {
            Ok(Self::Lineage(puzzles::LineageProof {
                parent_parent_coin_info: value.parent_parent_coin_info.into_rust()?,
                parent_inner_puzzle_hash: parent_inner_puzzle_hash.into_rust()?,
                parent_amount: value.parent_amount.into_rust()?,
            }))
        } else {
            Ok(Self::Eve(puzzles::EveProof {
                parent_parent_coin_info: value.parent_parent_coin_info.into_rust()?,
                parent_amount: value.parent_amount.into_rust()?,
            }))
        }
    }
}

impl IntoJs<LineageProof> for puzzles::Proof {
    fn into_js(self) -> Result<LineageProof> {
        match self {
            Self::Lineage(proof) => Ok(LineageProof {
                parent_parent_coin_info: proof.parent_parent_coin_info.into_js()?,
                parent_inner_puzzle_hash: Some(proof.parent_inner_puzzle_hash.into_js()?),
                parent_amount: proof.parent_amount.into_js()?,
            }),
            Self::Eve(proof) => Ok(LineageProof {
                parent_parent_coin_info: proof.parent_parent_coin_info.into_js()?,
                parent_inner_puzzle_hash: None,
                parent_amount: proof.parent_amount.into_js()?,
            }),
        }
    }
}
