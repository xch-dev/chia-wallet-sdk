use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const P2_DELEGATED_BY_SINGLETON_PUZZLE: [u8; 327] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ffff0143ffff04ffff0117ffff04ffff02ff04ffff04
    ff04ff5f8080ffff04ffff0bffff0102ffff0bffff0182010280ffff0bffff01
    02ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff
    06ffff04ff06ffff04ff0bffff04ff2fff8080808080ffff0bffff0101808080
    80ff8080808080ffff02ff5fff7f8080ffff04ffff04ffff01ff02ffff03ffff
    07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff02
    ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff01ff02ff
    ff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff
    0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff
    04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201018080ff
    018080ff018080
    "
);

pub const P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3f2358e5141470a084ab2f68126f6f07269f01a690994f364e67c9624bb6e05e
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2DelegatedBySingletonLayerArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_struct_hash: Bytes32,
    pub nonce: u64,
}

impl P2DelegatedBySingletonLayerArgs {
    pub fn new(singleton_struct_hash: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash,
            nonce,
        }
    }

    pub fn from_launcher_id(launcher_id: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
            nonce,
        }
    }

    pub fn curry_tree_hash(singleton_struct_hash: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
            args: Self::new(singleton_struct_hash, nonce),
        }
        .tree_hash()
    }

    pub fn curry_tree_hash_with_launcher_id(launcher_id: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
            args: Self::from_launcher_id(launcher_id, nonce),
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2DelegatedBySingletonLayerSolution<P, S> {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub delegated_puzzle: P,
    #[clvm(rest)]
    pub delegated_solution: S,
}

impl Mod for P2DelegatedBySingletonLayerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_DELEGATED_BY_SINGLETON_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH
    }
}
