use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{Mod, puzzles::ACTION_LAYER_PUZZLE_HASH};

pub const DEFAULT_FINALIZER_PUZZLE: [u8; 494] = hex!(
    "
    ff02ffff01ff04ffff04ffff0133ffff04ffff02ff0affff04ff0effff04ff05
    ffff04ffff02ff0affff04ff0effff04ff17ffff04ffff0bffff0101ff1780ff
    8080808080ffff04ffff0bffff0101ff4f80ffff04ffff02ff0cffff04ff0cff
    82019f8080ff80808080808080ffff04ffff0101ffff04ffff04ff0bff8080ff
    8080808080ffff02ff08ffff04ff08ff81df808080ffff04ffff04ffff04ffff
    01ff02ffff03ff05ffff01ff04ff09ffff02ff02ffff04ff02ffff04ff0dff07
    80808080ffff01ff02ffff03ff07ffff01ff02ff02ffff04ff02ff078080ffff
    018080ff018080ff0180ffff01ff02ffff03ffff07ff0380ffff01ff0bffff01
    02ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff
    01ff0bffff0101ff038080ff018080ffff04ffff01ff0bffff0102ffff0bffff
    0182010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0b
    ffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ff
    ff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff01
    8201018080ff01808080ff018080
    "
);

pub const DEFAULT_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultFinalizer1stCurryArgs {
    pub action_layer_mod_hash: Bytes32,
    pub hint: Bytes32,
}

impl DefaultFinalizer1stCurryArgs {
    pub fn new(hint: Bytes32) -> Self {
        Self {
            action_layer_mod_hash: ACTION_LAYER_PUZZLE_HASH.into(),
            hint,
        }
    }

    pub fn curry_tree_hash(hint: Bytes32) -> TreeHash {
        CurriedProgram {
            program: DEFAULT_FINALIZER_PUZZLE_HASH,
            args: DefaultFinalizer1stCurryArgs::new(hint),
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DefaultFinalizer2ndCurryArgs {
    pub finalizer_self_hash: Bytes32,
}

impl DefaultFinalizer2ndCurryArgs {
    pub fn new(hint: Bytes32) -> Self {
        Self {
            finalizer_self_hash: DefaultFinalizer1stCurryArgs::curry_tree_hash(hint).into(),
        }
    }

    pub fn curry_tree_hash(hint: Bytes32) -> TreeHash {
        let self_hash = DefaultFinalizer1stCurryArgs::curry_tree_hash(hint);

        CurriedProgram {
            program: self_hash,
            args: DefaultFinalizer2ndCurryArgs {
                finalizer_self_hash: self_hash.into(),
            },
        }
        .tree_hash()
    }
}

impl Mod for DefaultFinalizer1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DEFAULT_FINALIZER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DEFAULT_FINALIZER_PUZZLE_HASH
    }
}
