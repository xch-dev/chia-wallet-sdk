use std::borrow::Cow;

use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

/// The serialized CLVM bytecode of the `DataLayer` writer layer (CHIP-0035). See
/// [`WriterLayerArgs`].
pub const WRITER_LAYER_PUZZLE: [u8; 110] = hex!(
    "
    ff02ffff01ff02ff02ffff04ff02ffff04ffff02ff05ff0b80ff80808080ffff04ffff01ff02ffff
    03ff05ffff01ff02ffff03ffff09ff11ffff0181f380ffff01ff0880ffff01ff04ff09ffff02ff02
    ffff04ff02ffff04ff0dff808080808080ff0180ff8080ff0180ff018080
    "
);

/// The tree hash of [`WRITER_LAYER_PUZZLE`].
pub const WRITER_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    407f70ea751c25052708219ae148b45db2f61af2287da53d600b2486f12b3ca6
    "
));

/// Curried arguments for the `DataLayer` writer layer (CHIP-0035), which removes a delegated
/// puzzle's ability to change the list of delegated puzzles.
///
/// It runs the inner puzzle and fails if any of the conditions is an
/// [`UpdateDatastoreMerkleRoot`](crate::conditions::UpdateDatastoreMerkleRoot) (opcode `-13`).
/// It's used for writers in the [`DelegationLayerArgs`](crate::puzzles::DelegationLayerArgs)
/// merkle tree, which can update the store's metadata but not its delegated puzzles.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct WriterLayerArgs<I> {
    /// The inner puzzle, which decides who the writer is.
    pub inner_puzzle: I,
}

impl<I> WriterLayerArgs<I> {
    /// Creates new [`WriterLayerArgs`] from the inner puzzle.
    pub fn new(inner_puzzle: I) -> Self {
        Self { inner_puzzle }
    }
}

impl WriterLayerArgs<TreeHash> {
    /// Calculates the puzzle hash of the writer layer with the given inner puzzle hash.
    pub fn curry_tree_hash(inner_puzzle: TreeHash) -> TreeHash {
        CurriedProgram {
            program: WRITER_LAYER_PUZZLE_HASH,
            args: WriterLayerArgs { inner_puzzle },
        }
        .tree_hash()
    }
}

/// The solution for the `DataLayer` writer layer. See [`WriterLayerArgs`].
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct WriterLayerSolution<I> {
    /// The solution for the inner puzzle.
    pub inner_solution: I,
}

impl<I> Mod for WriterLayerArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&WRITER_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        WRITER_LAYER_PUZZLE_HASH
    }
}
