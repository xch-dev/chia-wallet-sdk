use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct DelegatedFeederArgs<I> {
    pub inner_puzzle: I,
}

impl<I> DelegatedFeederArgs<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl<I> Mod for DelegatedFeederArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DELEGATED_FEEDER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DELEGATED_FEEDER_PUZZLE_HASH
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct DelegatedFeederSolution<P, S, I> {
    pub delegated_puzzle: P,
    pub delegated_solution: S,
    #[clvm(rest)]
    pub inner_solution: I,
}

impl<P, S, I> DelegatedFeederSolution<P, S, I> {
    pub fn new(delegated_puzzle: P, delegated_solution: S, inner_solution: I) -> Self {
        Self {
            delegated_puzzle,
            delegated_solution,
            inner_solution,
        }
    }
}

pub const DELEGATED_FEEDER_PUZZLE: [u8; 203] = hex!(
    "
    ff02ffff01ff02ff04ffff04ff02ffff04ffff02ff05ffff04ffff02ff06ffff
    04ff02ffff04ff0bff80808080ff1f8080ffff04ffff02ff0bff1780ff808080
    8080ffff04ffff01ffff02ffff03ff05ffff01ff04ff09ffff02ff04ffff04ff
    02ffff04ff0dffff04ff0bff808080808080ffff010b80ff0180ff02ffff03ff
    ff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff8080
    8080ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101
    ff058080ff0180ff018080
    "
);

pub const DELEGATED_FEEDER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "9db33d93853179903d4dd272a00345ee6630dc94907dbcdd96368df6931060fd"
));
