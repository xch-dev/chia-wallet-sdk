use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{ONE_OF_N, ONE_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{MerkleProof, Mod};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct OneOfNArgs {
    pub merkle_root: Bytes32,
}

impl OneOfNArgs {
    pub fn new(merkle_root: Bytes32) -> Self {
        Self { merkle_root }
    }
}

impl Mod for OneOfNArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ONE_OF_N)
    }

    fn mod_hash() -> TreeHash {
        ONE_OF_N_HASH.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct OneOfNSolution<P, S> {
    pub merkle_proof: MerkleProof,
    pub member_puzzle: P,
    pub member_solution: S,
}

impl<P, S> OneOfNSolution<P, S> {
    pub fn new(merkle_proof: MerkleProof, member_puzzle: P, member_solution: S) -> Self {
        Self {
            merkle_proof,
            member_puzzle,
            member_solution,
        }
    }
}
