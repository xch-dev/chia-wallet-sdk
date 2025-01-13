use chia_protocol::Bytes32;
use chia_puzzles::singleton::{SingletonStruct, SINGLETON_TOP_LAYER_PUZZLE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2SingletonMessageArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_struct_hash: Bytes32,
}

impl P2SingletonMessageArgs {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
        }
    }
}

impl Mod for P2SingletonMessageArgs {
    const MOD_REVEAL: &[u8] = &P2_SINGLETON_MESSAGE_PUZZLE;
    const MOD_HASH: TreeHash = P2_SINGLETON_MESSAGE_PUZZLE_HASH;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct P2SingletonMessageSolution<P, S> {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl<P, S> P2SingletonMessageSolution<P, S> {
    pub fn new(
        singleton_inner_puzzle_hash: Bytes32,
        delegated_puzzle: P,
        delegated_solution: S,
    ) -> Self {
        Self {
            singleton_inner_puzzle_hash,
            delegated_puzzle,
            delegated_solution,
        }
    }
}

pub const P2_SINGLETON_MESSAGE_PUZZLE: [u8; 381] = hex!(
    "
    ff02ffff01ff04ffff04ff08ffff04ffff0117ffff04ffff02ff0effff04ff02
    ffff04ff2fff80808080ffff04ffff0bff2affff0bff0cffff0bff0cff32ff05
    80ffff0bff0cffff0bff3affff0bff0cffff0bff0cff32ff0b80ffff0bff0cff
    ff0bff3affff0bff0cffff0bff0cff32ff1780ffff0bff0cff32ff22808080ff
    22808080ff22808080ff8080808080ffff02ff2fff5f8080ffff04ffff01ffff
    4302ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cc
    e23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d
    78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b237
    59d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e
    7298a91ce119a63400ade7c5ff02ffff03ffff07ff0580ffff01ff0bffff0102
    ffff02ff0effff04ff02ffff04ff09ff80808080ffff02ff0effff04ff02ffff
    04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_SINGLETON_MESSAGE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "bd31c364428099b3049fd406ce88d2eef3fb877dfcb3495cb1a9e878f25aa669"
));
