use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const WRITER_LAYER_PUZZLE: [u8; 110] = hex!(
    "
    ff02ffff01ff02ff02ffff04ff02ffff04ffff02ff05ff0b80ff80808080ffff04ffff01ff02ffff
    03ff05ffff01ff02ffff03ffff09ff11ffff0181f380ffff01ff0880ffff01ff04ff09ffff02ff02
    ffff04ff02ffff04ff0dff808080808080ff0180ff8080ff0180ff018080
    "
);

pub const WRITER_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    407f70ea751c25052708219ae148b45db2f61af2287da53d600b2486f12b3ca6
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct WriterLayerArgs<I> {
    pub inner_puzzle: I,
}

impl<I> WriterLayerArgs<I> {
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl WriterLayerArgs<TreeHash> {
    pub fn curry_tree_hash(inner_puzzle: TreeHash) -> TreeHash {
        CurriedProgram {
            program: WRITER_LAYER_PUZZLE_HASH,
            args: WriterLayerArgs { inner_puzzle },
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct WriterLayerSolution<I> {
    pub inner_solution: I,
}

impl<I> Mod for WriterLayerArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&WRITER_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        WRITER_LAYER_PUZZLE_HASH
    }
}
