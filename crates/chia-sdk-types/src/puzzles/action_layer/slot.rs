use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{Mod, puzzles::CompactLineageProof};

/// Serialized slot puzzle, compiled from Rue.
///
/// A slot is a 0-amount coin created by a controller singleton to store a value that the
/// singleton can read in a later spend. Reading the value means spending the slot, so it
/// can only be read once; the singleton re-creates it if the value should persist. The
/// slot can only be spent by asserting that its parent is a coin of the controller
/// singleton and receiving a puzzle-to-puzzle message from the controller singleton.
///
/// Slots don't guarantee uniqueness by themselves: an app can omit a slot to pretend a
/// value doesn't exist. Apps that need uniqueness, such as `CATalog` and `XCHandles`,
/// arrange slots in a sorted doubly linked list.
///
/// The puzzle is curried twice: [`Slot1stCurryArgs`], then [`Slot2ndCurryArgs`]. The
/// solution is [`SlotSolution`].
pub const SLOT_PUZZLE: [u8; 311] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ffff0147ffff04ffff30ff4fffff02ff04ffff04ff06
    ffff04ff05ff81af808080ff81ef80ff808080ffff04ffff04ffff0143ffff04
    ffff0112ffff04ff80ffff04ffff02ff04ffff04ff06ffff04ff05ff3f808080
    ff8080808080ff808080ffff04ffff04ffff01ff0bffff0102ffff0bffff0182
    010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0980ffff
    0bffff0102ffff02ff02ffff04ff02ffff04ff0dffff04ff07ff8080808080ff
    ff0bffff010180808080ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0b
    ffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff
    0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01018080
    8080ffff01ff0bffff018201018080ff018080ff018080
    "
);

/// Tree hash of [`SLOT_PUZZLE`].
pub const SLOT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3
    "
));

/// Information about the controller singleton curried into a slot, encoded as
/// `(singleton_mod_hash . singleton_struct_hash)`.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PackedSingletonStruct {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Tree hash of the controller singleton's [`SingletonStruct`].
    #[clvm(rest)]
    pub singleton_struct_hash: Bytes32,
}

impl PackedSingletonStruct {
    /// Creates the packed struct for the singleton with the given launcher id, using the
    /// singleton top layer v1.1 mod hash.
    pub fn new(launcher_id: Bytes32) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
        }
    }
}
/// First curry of the slot puzzle ([`SLOT_PUZZLE`]).
///
/// Controller puzzles are curried with the hash of this first curry, so they only need
/// the value hash to compute a full slot puzzle hash.
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_sdk_types::{
///     Mod,
///     puzzles::{CatalogSlotValue, Slot1stCurryArgs, Slot2ndCurryArgs},
/// };
/// use clvm_utils::{CurriedProgram, ToTreeHash};
///
/// let launcher_id = Bytes32::new([1; 32]);
///
/// // CATalog uses nonce 0 for its slots.
/// let first_curry_hash = Slot1stCurryArgs::new(launcher_id, 0).curry_tree_hash();
///
/// let value = CatalogSlotValue::initial_left_end();
/// let slot_puzzle_hash = CurriedProgram {
///     program: first_curry_hash,
///     args: Slot2ndCurryArgs {
///         value_hash: value.tree_hash().into(),
///     },
/// }
/// .tree_hash();
/// # let _ = slot_puzzle_hash;
/// ```
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot1stCurryArgs {
    /// Singleton that controls the slot.
    pub controller_singleton_info: PackedSingletonStruct,
    /// Slot type. Different nonces keep slots of the same controller that hold different
    /// kinds of values (for example the [`RewardDistributorSlotNonce`](crate::puzzles::RewardDistributorSlotNonce)
    /// variants) from being confused with each other.
    pub nonce: u64,
}

impl Slot1stCurryArgs {
    /// Creates the arguments for a slot controlled by the singleton with the given launcher
    /// id.
    pub fn new(launcher_id: Bytes32, nonce: u64) -> Self {
        Self {
            controller_singleton_info: PackedSingletonStruct::new(launcher_id),
            nonce,
        }
    }
}

/// Second curry of the slot puzzle, which commits to the stored value.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot2ndCurryArgs {
    /// Tree hash of the value stored in the slot.
    pub value_hash: Bytes32,
}

/// Solution to the slot puzzle.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotSolution {
    /// Proof of the slot's parent, which must be a coin of the controller singleton.
    pub lineage_proof: CompactLineageProof,
    /// Inner puzzle hash of the controller singleton coin that spends the slot and sends it a
    /// message.
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
