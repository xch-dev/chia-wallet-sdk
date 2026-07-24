use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{Mod, puzzles::CompactLineageProof};

pub const SLOT_PUZZLE: [u8; 311] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ffff0147ffff04ffff30ff4fffff02ff04ffff04ff06
    ffff04ff05ff81af808080ff81ef80ff808080ffff04ffff04ffff0143ffff04
    ffff0112ffff04ff80ffff04ffff02ff04ffff04ff06ffff04ff05ff3f808080
    ff8080808080ff808080ffff04ffff04ffff01ff0bffff0102ffff0bffff0182
    010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0980ffff
    0bffff0102ffff02ff02ffff04ff02ffff04ff0dffff04ff07ff8080808080ff
    ff0bffff010180808080ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0b
    ffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff
    0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01018080
    8080ffff01ff0bffff018201018080ff018080ff018080
    "
);

pub const SLOT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PackedSingletonStruct {
    pub singleton_mod_hash: Bytes32,
    #[clvm(rest)]
    pub singleton_struct_hash: Bytes32,
}

impl PackedSingletonStruct {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
        }
    }
}
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot1stCurryArgs {
    pub controller_singleton_info: PackedSingletonStruct,
    pub nonce: u64,
}

impl Slot1stCurryArgs {
    pub fn new(launcher_id: Bytes32, nonce: u64) -> Self {
        Self {
            controller_singleton_info: PackedSingletonStruct::new(launcher_id),
            nonce,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot2ndCurryArgs {
    pub value_hash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotSolution {
    pub lineage_proof: CompactLineageProof,
    #[clvm(rest)]
    pub spender_inner_puzzle_hash: Bytes32,
}

impl Mod for Slot1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SLOT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        SLOT_PUZZLE_HASH
    }
}
