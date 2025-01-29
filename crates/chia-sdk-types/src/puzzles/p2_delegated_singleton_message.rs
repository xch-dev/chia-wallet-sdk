use chia_protocol::Bytes32;
use chia_puzzles::singleton::{SingletonStruct, SINGLETON_TOP_LAYER_PUZZLE_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2DelegatedSingletonMessageArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_struct_hash: Bytes32,
    pub nonce: usize,
}

impl Mod for P2DelegatedSingletonMessageArgs {
    const MOD_REVEAL: &[u8] = &P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE;
    const MOD_HASH: TreeHash = P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE_HASH;
}

impl P2DelegatedSingletonMessageArgs {
    pub fn new(launcher_id: Bytes32, nonce: usize) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
            nonce,
        }
    }

    pub fn curry_tree_hash(launcher_id: Bytes32, nonce: usize) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE_HASH,
            args: Self::new(launcher_id, nonce),
        }
        .tree_hash()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2DelegatedSingletonMessageSolution<P, S> {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl<P, S> P2DelegatedSingletonMessageSolution<P, S> {
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

pub const P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE: [u8; 382] = hex!(
    "
    ff02ffff01ff04ffff04ff08ffff04ffff0117ffff04ffff02ff0effff04ff02
    ffff04ff5fff80808080ffff04ffff0bff2affff0bff0cffff0bff0cff32ff05
    80ffff0bff0cffff0bff3affff0bff0cffff0bff0cff32ff0b80ffff0bff0cff
    ff0bff3affff0bff0cffff0bff0cff32ff2f80ffff0bff0cff32ff22808080ff
    22808080ff22808080ff8080808080ffff02ff5fff81bf8080ffff04ffff01ff
    ff4302ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7
    cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a1
    6d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b2
    3759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a
    6e7298a91ce119a63400ade7c5ff02ffff03ffff07ff0580ffff01ff0bffff01
    02ffff02ff0effff04ff02ffff04ff09ff80808080ffff02ff0effff04ff02ff
    ff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "25fbd0d4586ff8266eb8b0fc4768b7714394d87f87824b0124fc10806ba87bb5"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE => P2_DELEGATED_SINGLETON_MESSAGE_PUZZLE_HASH);
        Ok(())
    }
}
