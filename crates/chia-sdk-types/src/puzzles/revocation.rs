use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{REVOCATION_LAYER, REVOCATION_LAYER_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct RevocationArgs {
    pub mod_hash: Bytes32,
    pub hidden_puzzle_hash: Bytes32,
    pub inner_puzzle_hash: Bytes32,
}

impl RevocationArgs {
    pub fn new(hidden_puzzle_hash: Bytes32, inner_puzzle_hash: Bytes32) -> Self {
        Self {
            mod_hash: REVOCATION_LAYER_HASH.into(),
            hidden_puzzle_hash,
            inner_puzzle_hash,
        }
    }
}

impl Mod for RevocationArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REVOCATION_LAYER)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(REVOCATION_LAYER_HASH)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RevocationSolution<P, S> {
    pub hidden: bool,
    pub puzzle: P,
    pub solution: S,
}

impl<P, S> RevocationSolution<P, S> {
    pub fn new(hidden: bool, puzzle: P, solution: S) -> Self {
        Self {
            hidden,
            puzzle,
            solution,
        }
    }
}
