use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{puzzles::ACTION_LAYER_PUZZLE_HASH, Mod};

pub const DEFAULT_FINALIZER_PUZZLE: [u8; 616] = hex!(
    "
    ff02ffff01ff04ffff04ff10ffff04ffff02ff12ffff04ff02ffff04ff05ffff
    04ffff02ff12ffff04ff02ffff04ff17ffff04ffff0bffff0101ff1780ff8080
    808080ffff04ffff0bffff0101ff4f80ffff04ffff02ff1effff04ff02ffff04
    ff82019fff80808080ff80808080808080ffff04ffff0101ffff04ffff04ff0b
    ff8080ff8080808080ffff02ff1affff04ff02ffff04ff81dfff8080808080ff
    ff04ffff01ffffff3302ffff02ffff03ff05ffff01ff0bff7cffff02ff16ffff
    04ff02ffff04ff09ffff04ffff02ff14ffff04ff02ffff04ff0dff80808080ff
    808080808080ffff016c80ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7
    e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb
    99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291e
    aea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb
    1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffffff0bff5cffff02
    ff16ffff04ff02ffff04ff05ffff04ffff02ff14ffff04ff02ffff04ff07ff80
    808080ff808080808080ff02ffff03ff09ffff01ff04ff11ffff02ff1affff04
    ff02ffff04ffff04ff19ff0d80ff8080808080ffff01ff02ffff03ff0dffff01
    ff02ff1affff04ff02ffff04ff0dff80808080ff8080ff018080ff0180ffff0b
    ff18ffff0bff18ff6cff0580ffff0bff18ff0bff4c8080ff02ffff03ffff07ff
    0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ff
    ff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff0580
    80ff0180ff018080
    "
);

pub const DEFAULT_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    5ec944c72fd72b55f7c753f4ded0f3fe7387958f2ebbb35ee3c40f062498b93f
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
