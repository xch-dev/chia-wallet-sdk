use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
};

/// Serialized `XCHandles` initiate update action, compiled from Rue.
///
/// Starts changing a handle's owner and resolved launcher ids. The current owner
/// singleton coin must send a coin-to-puzzle message of `"i"` followed by the tree hash
/// of the new [`XchandlesUpdateSlotValue`](crate::puzzles::XchandlesUpdateSlotValue). The
/// action re-creates the handle slot unchanged (with its counter increased) and creates
/// an update slot, hinted to the initiating coin's id, that can be executed at
/// `min_height + relative_block_height`. The handle must not be expired, and
/// `min_height` must be in the future.
pub const XCHANDLES_INITIATE_UPDATE_PUZZLE: [u8; 791] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff01ff04ff8202ffffff04ffff04ffff0155ff
    ff04ff822dffff808080ffff04ffff04ffff0157ffff04ff821fffff808080ff
    ff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff2bffff04
    ff5bffff04ff81bfffff04ffff0bffff0101ffff02ff13ffff04ff13ff8205ff
    808080ff8080808080ff8080808080ffff04ffff02ff7bffff04ffff04ff2bff
    5b80ffff04ff81bfffff02ff13ffff04ff13ffff04ffff10ff8209ffffff0101
    80ff820dff808080808080ffff04ffff04ffff0143ffff04ffff013affff04ff
    ff0effff0169ff0280ffff04ff05ff8080808080ffff04ffff04ffff0133ffff
    04ffff02ff2bffff04ff5bffff04ff82017fffff04ffff0bffff0101ff0280ff
    8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ff80808080
    80808080ffff04ffff02ff09ffff04ff09ffff04ffff04ff02ffff10ff820fff
    ff2f8080ffff04ff8212ffff8205ff80808080ff018080ffff04ffff30ff8209
    ffffff02ff0affff04ff16ffff04ff05ffff04ffff02ff04ffff04ff04ffff04
    ff05ffff04ff82177fff0b80808080ffff04ff8215ffff808080808080ff821d
    ff80ff018080ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bff
    ff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080
    ffff01ff0bffff0101ff038080ff0180ffff04ffff01ff0bffff0102ffff0bff
    ff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff05
    80ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff0101808080
    80ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff01820104
    80ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bff
    ff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff
    0bffff018201018080ff0180ffff01ff04ffff0133ffff04ffff02ff04ffff04
    ff06ffff04ff05ffff04ffff0bffff0101ff0780ff8080808080ffff04ff80ff
    ff04ffff04ff05ff8080ff8080808080808080ff018080
    "
);

/// Tree hash of [`XCHANDLES_INITIATE_UPDATE_PUZZLE`].
pub const XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    23eeec5ecd731c2822adf82f23a2e73fdd4ad08df56558ad15873c68cadf68f8
    "
));

/// Curried arguments of the `XCHandles` initiate update action
/// ([`XCHANDLES_INITIATE_UPDATE_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesInitiateUpdateActionArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Puzzle hash of the singleton launcher, used with
    /// [`singleton_mod_hash`](Self::singleton_mod_hash) to compute singleton puzzle hashes
    /// from launcher ids.
    pub singleton_launcher_mod_hash: Bytes32,
    /// Number of blocks, counted from the solution's `min_height`, before the update can be
    /// executed.
    pub relative_block_height: u32,
    /// Hash of the slot puzzle after its first curry for handle slots
    /// ([`XchandlesSlotNonce::HANDLE`](crate::puzzles::XchandlesSlotNonce::HANDLE)).
    pub handle_slot_1st_curry_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for update slots
    /// ([`XchandlesSlotNonce::UPDATE`](crate::puzzles::XchandlesSlotNonce::UPDATE)).
    pub update_slot_1st_curry_hash: Bytes32,
}

/// Solution to the `XCHandles` initiate update action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesInitiateUpdateActionSolution {
    /// Current value of the handle's slot.
    pub current_slot_value: XchandlesHandleSlotValue,
    /// New owner and resolved launcher ids.
    pub new_data: XchandlesDataValue,
    /// Proof of the current owner singleton coin, which sends the initiating message.
    pub current_owner: CompactCoinProof,
    /// Block height that must not have been reached yet. The update can be executed at
    /// `min_height + relative_block_height`.
    #[clvm(rest)]
    pub min_height: u32,
}

impl Mod for XchandlesInitiateUpdateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_INITIATE_UPDATE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_INITIATE_UPDATE_PUZZLE_HASH
    }
}
