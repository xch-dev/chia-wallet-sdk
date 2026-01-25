use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{puzzles::CompactLineageProof, Mod};

pub const SLOT_PUZZLE: [u8; 359] = hex!(
    "
    ff02ffff01ff04ffff04ff08ffff04ffff30ff4fffff02ff1effff04ff02ffff
    04ff05ffff04ff81afff8080808080ff81ef80ff808080ffff04ffff04ff0cff
    ff04ffff0112ffff04ff80ffff04ffff02ff1effff04ff02ffff04ff05ffff04
    ff3fff8080808080ff8080808080ff808080ffff04ffff01ffff4743ff02ffff
    ffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785
    459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f59671
    8ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6
    806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91c
    e119a63400ade7c5ff0bff56ffff0bff0affff0bff0aff66ff0980ffff0bff0a
    ffff0bff76ffff0bff0affff0bff0aff66ff0d80ffff0bff0affff0bff76ffff
    0bff0affff0bff0aff66ff0b80ffff0bff0aff66ff46808080ff46808080ff46
    808080ff018080
    "
);

pub const SLOT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    27de4bb4bc5c6881c08e7e57288e2b855d934fd6f03ef6d8657e15a35e8f96f8
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
