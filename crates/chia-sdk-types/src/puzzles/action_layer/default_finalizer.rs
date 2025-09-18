use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{puzzles::ACTION_LAYER_PUZZLE_HASH, Mod};

pub const DEFAULT_FINALIZER_PUZZLE: [u8; 617] = hex!(
    "
    ff02ffff01ff04ffff04ff10ffff04ffff02ff12ffff04ff02ffff04ff05ffff
    04ffff02ff12ffff04ff02ffff04ff17ffff04ffff0bffff0101ff1780ff8080
    808080ffff04ffff0bffff0101ff2f80ffff04ffff02ff1effff04ff02ffff04
    ff82033fff80808080ff80808080808080ffff04ffff0101ffff04ffff04ff0b
    ff8080ff8080808080ffff02ff1affff04ff02ffff04ff8201bfff8080808080
    ffff04ffff01ffffff3302ffff02ffff03ff05ffff01ff0bff7cffff02ff16ff
    ff04ff02ffff04ff09ffff04ffff02ff14ffff04ff02ffff04ff0dff80808080
    ff808080808080ffff016c80ff0180ffffa04bf5122f344554c53bde2ebb8cd2
    b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124c
    eb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb861929
    1eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471eb
    cb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffffff0bff5cffff
    02ff16ffff04ff02ffff04ff05ffff04ffff02ff14ffff04ff02ffff04ff07ff
    80808080ff808080808080ff02ffff03ff09ffff01ff04ff11ffff02ff1affff
    04ff02ffff04ffff04ff19ff0d80ff8080808080ffff01ff02ffff03ff0dffff
    01ff02ff1affff04ff02ffff04ff0dff80808080ff8080ff018080ff0180ffff
    0bff18ffff0bff18ff6cff0580ffff0bff18ff0bff4c8080ff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080
    ffff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff018080
    "
);

pub const DEFAULT_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    34b1f957ca3ba935921c32625cd432316ae71344977d96b4ffc5243c7d08d781
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
