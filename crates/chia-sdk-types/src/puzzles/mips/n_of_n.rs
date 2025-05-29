use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

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
        Cow::Borrowed(&N_OF_N_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        N_OF_N_PUZZLE_HASH
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

pub const N_OF_N_PUZZLE: [u8; 243] = hex!(
    "
    ff02ffff01ff02ff04ffff04ff02ffff04ff05ffff04ff17ffff04ff0bff8080
    80808080ffff04ffff01ffff02ffff03ff0dffff01ff02ff0affff04ff02ffff
    04ffff02ff0effff04ff02ffff04ff09ffff04ff13ffff04ff17ff8080808080
    80ffff04ffff02ff04ffff04ff02ffff04ff0dffff04ff1bffff04ff17ff8080
    80808080ff8080808080ffff01ff02ff0effff04ff02ffff04ff09ffff04ff13
    ffff04ff17ff80808080808080ff0180ffff02ffff03ff05ffff01ff04ff09ff
    ff02ff0affff04ff02ffff04ff0dffff04ff0bff808080808080ffff010b80ff
    0180ff02ff05ffff04ff17ff0b8080ff018080
    "
);

pub const N_OF_N_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "d4394f50cb1d6ef130788db2e69ab0087ef79b0737179f201c1d1d2a52df1e59"
));
