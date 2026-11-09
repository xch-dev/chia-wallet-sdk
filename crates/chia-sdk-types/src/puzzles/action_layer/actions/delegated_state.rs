use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

/// Serialized delegated state action, compiled from Rue.
///
/// Replaces the action layer's state with a new state approved by another singleton,
/// which must send a puzzle-to-puzzle message of `"s"` followed by the tree hash of the
/// new state. The ephemeral state is left unchanged. Registries use it to let a price
/// singleton (such as a [`StateSchedulerLayerArgs`](crate::puzzles::StateSchedulerLayerArgs)
/// singleton) update their registration prices.
pub const DELEGATED_STATE_ACTION_PUZZLE: [u8; 341] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ff27ff2f80ffff04ffff04ffff0143ffff04ffff0112
    ffff04ffff0effff0173ffff02ff04ffff04ff04ff2f808080ffff04ffff0bff
    ff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff
    0182010180ff0580ffff0bffff0102ffff02ff06ffff04ff06ffff04ff0bffff
    04ff3fff8080808080ffff0bffff010180808080ff8080808080ff808080ffff
    04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02
    ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff01
    01ff038080ff0180ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff
    0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff0bffff018201018080ff018080ff018080
    "
);

/// Tree hash of [`DELEGATED_STATE_ACTION_PUZZLE`].
pub const DELEGATED_STATE_ACTION_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    491ee66c8ffd29ef41bf820d3d4d766deb1b6f650932af09f26f807dab1a362f
    "
));

/// Curried arguments of the delegated state action ([`DELEGATED_STATE_ACTION_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct DelegatedStateActionArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Tree hash of the [`SingletonStruct`] of the singleton allowed to set the state.
    pub other_singleton_struct_hash: Bytes32,
}

impl DelegatedStateActionArgs {
    /// Creates the arguments for the singleton with the given launcher id.
    pub fn new(other_launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            other_singleton_struct_hash: SingletonStruct::new(other_launcher_id).tree_hash().into(),
        }
    }
}

impl DelegatedStateActionArgs {
    /// Computes the action's curried puzzle hash for the singleton with the given launcher
    /// id.
    pub fn curry_tree_hash(other_launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: DELEGATED_STATE_ACTION_PUZZLE_HASH,
            args: DelegatedStateActionArgs::new(other_launcher_id),
        }
        .tree_hash()
    }
}

/// Solution to the delegated state action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct DelegatedStateActionSolution<S> {
    /// New action layer state.
    pub new_state: S,
    /// Inner puzzle hash of the approving singleton's coin, used to verify the message's
    /// sender.
    #[clvm(rest)]
    pub other_singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for DelegatedStateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&DELEGATED_STATE_ACTION_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        DELEGATED_STATE_ACTION_PUZZLE_HASH
    }
}
