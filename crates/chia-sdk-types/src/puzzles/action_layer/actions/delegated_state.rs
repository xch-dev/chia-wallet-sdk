use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const DELEGATED_STATE_ACTION_PUZZLE: [u8; 387] = hex!(
    "
    ff02ffff01ff04ffff04ff27ff4f80ffff04ffff04ff08ffff04ffff0112ffff
    04ffff02ff0effff04ff02ffff04ff4fff80808080ffff04ffff0bff2affff0b
    ff0cffff0bff0cff32ff0580ffff0bff0cffff0bff3affff0bff0cffff0bff0c
    ff32ff0b80ffff0bff0cffff0bff3affff0bff0cffff0bff0cff32ff6f80ffff
    0bff0cff32ff22808080ff22808080ff22808080ff8080808080ff808080ffff
    04ffff01ffff4302ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad6
    31c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b08
    3721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581c
    bd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e
    1879b7152a6e7298a91ce119a63400ade7c5ff02ffff03ffff07ff0580ffff01
    ff0bffff0102ffff02ff0effff04ff02ffff04ff09ff80808080ffff02ff0eff
    ff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff
    018080
    "
);

pub const DELEGATED_STATE_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    145e54a297466100f202690d58bded6074834e2ae8cd4dfbcf66e33bb8b77c05
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DelegatedStateActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub other_singleton_struct_hash: Bytes32,
}

impl DelegatedStateActionArgs {
    pub fn new(other_launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            other_singleton_struct_hash: SingletonStruct::new(other_launcher_id).tree_hash().into(),
        }
    }
}

impl DelegatedStateActionArgs {
    pub fn curry_tree_hash(other_launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: DELEGATED_STATE_ACTION_PUZZLE_HASH,
            args: DelegatedStateActionArgs::new(other_launcher_id),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct DelegatedStateActionSolution<S> {
    pub new_state: S,
    #[clvm(rest)]
    pub other_singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for DelegatedStateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DELEGATED_STATE_ACTION_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DELEGATED_STATE_ACTION_PUZZLE_HASH
    }
}
