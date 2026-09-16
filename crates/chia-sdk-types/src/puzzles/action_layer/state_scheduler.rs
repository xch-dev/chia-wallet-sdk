use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const STATE_SCHEDULER_PUZZLE: [u8; 243] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ffff0142ffff04ffff0112ffff04ff17ffff04ffff0b
    ffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bff
    ff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ffff04ff0bff
    ff04ff5fff8080808080ffff0bffff010180808080ff8080808080ffff02ff2f
    ff7f8080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff01
    82010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ff
    ff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ff
    ff01ff0bffff018201018080ff0180ff018080
    "
);

pub const STATE_SCHEDULER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8811d56e9efd2c9f449ea10cb00e00417b372f46d9d3a00ddf632f292de7e2c3
    "
));

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct StateSchedulerLayerArgs<M, I> {
    pub singleton_mod_hash: Bytes32,
    pub receiver_singleton_struct_hash: Bytes32,
    pub prefix_and_message: M,
    pub inner_puzzle: I,
}

impl<M, I> StateSchedulerLayerArgs<M, I>
where
    I: ToTreeHash,
{
    pub fn curry_tree_hash(
        receiver_singleton_struct_hash: Bytes32,
        prefix_and_message_hash: TreeHash,
        inner_puzzle: &I,
    ) -> TreeHash {
        CurriedProgram::<TreeHash, _> {
            program: STATE_SCHEDULER_PUZZLE_HASH,
            args: StateSchedulerLayerArgs {
                singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
                receiver_singleton_struct_hash,
                prefix_and_message: prefix_and_message_hash,
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
