use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{M_OF_N, M_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct MofNArgs {
    pub required: usize,
    pub merkle_root: Bytes32,
}

impl MofNArgs {
    pub fn new(required: usize, merkle_root: Bytes32) -> Self {
        Self {
            required,
            merkle_root,
        }
    }
}

impl Mod for MofNArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&M_OF_N)
    }

    fn mod_hash() -> TreeHash {
        M_OF_N_HASH.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNSolution<P> {
    pub proofs: P,
}

impl<P> MofNSolution<P> {
    pub fn new(proofs: P) -> Self {
        Self { proofs }
    }
}
