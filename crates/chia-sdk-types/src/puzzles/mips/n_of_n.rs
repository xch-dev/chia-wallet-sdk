use std::borrow::Cow;

use chia_puzzles::{N_OF_N, N_OF_N_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct NofNArgs<T> {
    pub members: Vec<T>,
}

impl<T> NofNArgs<T> {
    pub fn new(members: Vec<T>) -> Self {
        Self { members }
    }
}

impl<T> Mod for NofNArgs<T> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&N_OF_N)
    }

    fn mod_hash() -> TreeHash {
        N_OF_N_HASH.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NofNSolution<T> {
    pub member_solutions: Vec<T>,
}

impl<T> NofNSolution<T> {
    pub fn new(member_solutions: Vec<T>) -> Self {
        Self { member_solutions }
    }
}
