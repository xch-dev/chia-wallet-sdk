use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const STATE_SCHEDULER_PUZZLE: [u8; 285] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ffff0112ffff04ff17ffff04ffff0bff2e
    ffff0bff0affff0bff0aff36ff0580ffff0bff0affff0bff3effff0bff0affff
    0bff0aff36ff0b80ffff0bff0affff0bff3effff0bff0affff0bff0aff36ff5f
    80ffff0bff0aff36ff26808080ff26808080ff26808080ff8080808080ffff02
    ff2fff7f8080ffff04ffff01ff42ff02ffffa04bf5122f344554c53bde2ebb8c
    d2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a7312
    4ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619
    291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471
    ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff018080
    "
);

pub const STATE_SCHEDULER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    13fe7833751a6fe582caa09d48978d8d1b016d224cb0c10e538184ab22df9c13
    "
));

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct StateSchedulerLayerArgs<M, I> {
    pub singleton_mod_hash: Bytes32,
    pub receiver_singleton_struct_hash: Bytes32,
    pub message: M,
    pub inner_puzzle: I,
}

impl<M, I> StateSchedulerLayerArgs<M, I>
where
    M: ToTreeHash,
    I: ToTreeHash,
{
    pub fn curry_tree_hash(
        receiver_singleton_struct_hash: Bytes32,
        message: &M,
        inner_puzzle: &I,
    ) -> TreeHash {
        CurriedProgram::<TreeHash, _> {
            program: STATE_SCHEDULER_PUZZLE_HASH,
            args: StateSchedulerLayerArgs {
                singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
                receiver_singleton_struct_hash,
                message: message.tree_hash(),
                inner_puzzle: inner_puzzle.tree_hash(),
            },
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct StateSchedulerLayerSolution<I> {
    pub other_singleton_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub inner_solution: I,
}

impl<M, I> Mod for StateSchedulerLayerArgs<M, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&STATE_SCHEDULER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        STATE_SCHEDULER_PUZZLE_HASH
    }
}
