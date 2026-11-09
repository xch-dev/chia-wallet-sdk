use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

/// Serialized state scheduler puzzle, compiled from Rue.
///
/// Used as an inner puzzle of a singleton that schedules state changes for another
/// (receiver) singleton. When spent, it sends a fixed message to the receiver singleton
/// and runs its inner puzzle, which typically re-creates the scheduler with the next
/// scheduled state. The receiver accepts the message with the delegated state action
/// ([`DelegatedStateActionArgs`](crate::puzzles::DelegatedStateActionArgs)).
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

/// Tree hash of [`STATE_SCHEDULER_PUZZLE`].
pub const STATE_SCHEDULER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8811d56e9efd2c9f449ea10cb00e00417b372f46d9d3a00ddf632f292de7e2c3
    "
));

/// Curried arguments of the state scheduler ([`STATE_SCHEDULER_PUZZLE`]).
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct StateSchedulerLayerArgs<M, I> {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Tree hash of the receiver singleton's [`SingletonStruct`](chia_puzzle_types::singleton::SingletonStruct).
    pub receiver_singleton_struct_hash: Bytes32,
    /// Message sent to the receiver singleton (puzzle-to-puzzle), for a state update `"s"`
    /// followed by the tree hash of the new state.
    pub prefix_and_message: M,
    /// Inner puzzle, run with [`inner_solution`](StateSchedulerLayerSolution::inner_solution),
    /// whose conditions are added to the output.
    pub inner_puzzle: I,
}

impl<M, I> StateSchedulerLayerArgs<M, I>
where
    I: ToTreeHash,
{
    /// Computes the curried puzzle hash, given the tree hash of the message and the inner
    /// puzzle (or its hash).
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

/// Solution to the state scheduler puzzle.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct StateSchedulerLayerSolution<I> {
    /// Current inner puzzle hash of the receiver singleton, used to address the message.
    pub other_singleton_inner_puzzle_hash: Bytes32,
    /// Solution passed to the inner puzzle.
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
