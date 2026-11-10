use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

/// Serialized `p2_delegated_by_singleton` puzzle, compiled from Rue.
///
/// Locks a coin so that it can only be spent with a delegated puzzle that a singleton
/// approves. The singleton sends a message (sender puzzle, receiver coin) whose content
/// is the tree hash of the delegated puzzle, and the coin outputs the delegated puzzle's
/// conditions. Action layer apps use it for reserves and for locked assets.
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

/// Tree hash of [`P2_DELEGATED_BY_SINGLETON_PUZZLE`].
pub const P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    3f2358e5141470a084ab2f68126f6f07269f01a690994f364e67c9624bb6e05e
    "
));

/// Curried arguments of [`P2_DELEGATED_BY_SINGLETON_PUZZLE`].
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::singleton::SingletonStruct;
/// use chia_sdk_types::puzzles::P2DelegatedBySingletonLayerArgs;
/// use clvm_utils::ToTreeHash;
///
/// let launcher_id = Bytes32::new([1; 32]);
/// let singleton_struct_hash: Bytes32 = SingletonStruct::new(launcher_id).tree_hash().into();
///
/// // Both helpers compute the same puzzle hash.
/// assert_eq!(
///     P2DelegatedBySingletonLayerArgs::curry_tree_hash_with_launcher_id(launcher_id, 0),
///     P2DelegatedBySingletonLayerArgs::curry_tree_hash(singleton_struct_hash, 0),
/// );
///
/// // A different nonce gives a different puzzle hash for the same singleton.
/// assert_ne!(
///     P2DelegatedBySingletonLayerArgs::curry_tree_hash(singleton_struct_hash, 0),
///     P2DelegatedBySingletonLayerArgs::curry_tree_hash(singleton_struct_hash, 1),
/// );
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2DelegatedBySingletonLayerArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Tree hash of the controlling singleton's [`SingletonStruct`].
    pub singleton_struct_hash: Bytes32,
    /// Nonce that lets one singleton control several distinct puzzle hashes. The puzzle
    /// ignores it. For example, the reward distributor uses nonce 0 for its reserve and
    /// nonce 1 for staked assets.
    pub nonce: u64,
}

impl P2DelegatedBySingletonLayerArgs {
    /// Creates the arguments from a singleton struct hash, using the singleton top layer v1.1
    /// mod hash.
    pub fn new(singleton_struct_hash: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash,
            nonce,
        }
    }

    /// Creates the arguments for the singleton with the given launcher id.
    pub fn from_launcher_id(launcher_id: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
            nonce,
        }
    }

    /// Computes the curried puzzle hash from a singleton struct hash.
    pub fn curry_tree_hash(singleton_struct_hash: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
            args: Self::new(singleton_struct_hash, nonce),
        }
        .tree_hash()
    }

    /// Computes the curried puzzle hash for the singleton with the given launcher id.
    pub fn curry_tree_hash_with_launcher_id(launcher_id: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
            args: Self::from_launcher_id(launcher_id, nonce),
        }
        .tree_hash()
    }
}

/// Solution to [`P2_DELEGATED_BY_SINGLETON_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2DelegatedBySingletonLayerSolution<P, S> {
    /// Inner puzzle hash of the controlling singleton coin that sends the approval message.
    pub singleton_inner_puzzle_hash: Bytes32,
    /// Puzzle that produces the conditions. Its tree hash is the message the singleton must
    /// send.
    pub delegated_puzzle: P,
    /// Solution passed to the delegated puzzle.
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
