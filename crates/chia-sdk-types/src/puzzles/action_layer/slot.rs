use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const SLOT_PUZZLE: [u8; 360] = hex!(
    "
    ff02ffff01ff04ffff04ff08ffff04ffff30ff4fffff02ff1effff04ff02ffff
    04ff05ffff04ff81afff8080808080ff82016f80ff808080ffff04ffff04ff0c
    ffff04ffff0112ffff04ff80ffff04ffff02ff1effff04ff02ffff04ff05ffff
    04ff3fff8080808080ff8080808080ff808080ffff04ffff01ffff4743ff02ff
    ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c77
    85459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596
    718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225
    f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a9
    1ce119a63400ade7c5ff0bff56ffff0bff0affff0bff0aff66ff0980ffff0bff
    0affff0bff76ffff0bff0affff0bff0aff66ff0d80ffff0bff0affff0bff76ff
    ff0bff0affff0bff0aff66ff0b80ffff0bff0aff66ff46808080ff46808080ff
    46808080ff018080
    "
);

pub const SLOT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2d55c0904da19dfb06ccdfb9b6ee3e4117e5856b573c9e5f495a5cdeab35ab51
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
    pub lineage_proof: LineageProof,
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
