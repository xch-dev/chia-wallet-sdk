use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::NodePtr;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct RestrictionsArgs<MV, DV, I> {
    pub member_validators: Vec<MV>,
    pub delegated_puzzle_validators: Vec<DV>,
    pub inner_puzzle: I,
}

impl<MV, DV, I> RestrictionsArgs<MV, DV, I> {
    pub fn new(
        member_validators: Vec<MV>,
        delegated_puzzle_validators: Vec<DV>,
        inner_puzzle: I,
    ) -> Self {
        Self {
            member_validators,
            delegated_puzzle_validators,
            inner_puzzle,
        }
    }
}

impl<MV, DV, I> Mod for RestrictionsArgs<MV, DV, I> {
    const MOD_REVEAL: &[u8] = &RESTRICTIONS_PUZZLE;
    const MOD_HASH: TreeHash = RESTRICTIONS_PUZZLE_HASH;
    type Solution = RestrictionsSolution<NodePtr, NodePtr, NodePtr>;
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct RestrictionsSolution<MV, DV, I> {
    pub delegated_puzzle_hash: Bytes32,
    pub member_validator_solutions: Vec<MV>,
    pub delegated_puzzle_validator_solutions: Vec<DV>,
    pub inner_solution: I,
}

impl<MV, DV, I> RestrictionsSolution<MV, DV, I> {
    pub fn new(
        delegated_puzzle_hash: Bytes32,
        member_validator_solutions: Vec<MV>,
        delegated_puzzle_validator_solutions: Vec<DV>,
        inner_solution: I,
    ) -> Self {
        Self {
            delegated_puzzle_hash,
            member_validator_solutions,
            delegated_puzzle_validator_solutions,
            inner_solution,
        }
    }
}

pub const RESTRICTIONS_PUZZLE: [u8; 204] = hex!(
    "
    ff02ffff01ff02ff04ffff04ff02ffff04ff05ffff04ff5fffff04ffff02ff17
    ffff04ff2fff82017f8080ffff04ffff02ff06ffff04ff02ffff04ff0bffff04
    ff81bfffff04ff2fff808080808080ff80808080808080ffff04ffff01ffff03
    ff80ffff02ff06ffff04ff02ffff04ff05ffff04ff0bffff04ff17ff80808080
    8080ff1780ff02ffff03ff05ffff01ff03ff80ffff02ff09ffff04ff17ff1380
    80ffff02ff06ffff04ff02ffff04ff0dffff04ff1bffff04ff17ff8080808080
    8080ff8080ff0180ff018080
    "
);

pub const RESTRICTIONS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "a28d59d39f964a93159c986b1914694f6f2f1c9901178f91e8b0ba4045980eef"
));
