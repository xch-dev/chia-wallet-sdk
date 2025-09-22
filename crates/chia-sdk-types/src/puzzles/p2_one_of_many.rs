use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{P2_1_OF_N, P2_1_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::{MerkleProof, Mod};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2OneOfManyArgs {
    pub merkle_root: Bytes32,
}

impl P2OneOfManyArgs {
    pub fn new(merkle_root: Bytes32) -> Self {
        Self { merkle_root }
    }
}

impl Mod for P2OneOfManyArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_1_OF_N)
    }

    fn mod_hash() -> TreeHash {
        P2_1_OF_N_HASH.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2OneOfManySolution<P, S> {
    pub merkle_proof: MerkleProof,
    pub puzzle: P,
    pub solution: S,
}

impl<P, S> P2OneOfManySolution<P, S> {
    pub fn new(merkle_proof: MerkleProof, puzzle: P, solution: S) -> Self {
        Self {
            merkle_proof,
            puzzle,
            solution,
        }
    }
}
