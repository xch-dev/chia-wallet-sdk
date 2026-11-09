use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{CompactCoinProof, XchandlesDataValue, XchandlesHandleSlotValue},
};

/// Serialized `XCHandles` execute update action, compiled from Rue.
///
/// Completes an update started with the initiate update action. The update slot's
/// minimum execution height must have been reached and the handle must not be expired.
/// The owner coin that is the child of the initiating coin must send a coin-to-puzzle
/// message of `"u"` followed by the update slot value's hash, and the new owner and new
/// resolved singletons must send `"o"` and `"r"` followed by it. The action spends the
/// update slot and re-creates the handle slot with the new data.
pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE: [u8; 979] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ff81bfffff04ffff04ffff0155ffff04ff8216ff
    ff808080ffff04ffff04ffff0153ffff04ff82017fff808080ffff04ffff04ff
    ff0142ffff04ffff0112ffff04ff80ffff04ffff02ff15ffff04ff2dffff04ff
    2fffff04ffff0bffff0101ffff02ff09ffff04ff09ff8202ff808080ff808080
    8080ff8080808080ffff04ffff02ff3dffff04ffff04ff15ff2d80ffff04ff2f
    ffff02ff09ffff04ff09ffff04ffff10ff8204ffffff010180ffff04ff820aff
    ffff04ff8216ffff8205ff8080808080808080ffff04ffff04ffff0143ffff04
    ffff013affff04ffff0effff0175ff0280ffff04ffff30ff8213ffffff02ff15
    ffff04ff2dffff04ff0bffff04ffff02ff09ffff04ff09ffff04ff0bffff04ff
    822effff1780808080ffff04ff822bffff808080808080ff823bff80ff808080
    8080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff016fff0280
    ffff04ffff02ff15ffff04ff2dffff04ff0bffff04ffff02ff09ffff04ff09ff
    ff04ff0bffff04ff8209ffff1780808080ffff04ff8217ffff808080808080ff
    8080808080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0172
    ff0280ffff04ffff02ff15ffff04ff2dffff04ff0bffff04ffff02ff09ffff04
    ff09ffff04ff0bffff04ff820dffff1780808080ffff04ff821fffff80808080
    8080ff8080808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff
    04ffff02ff15ffff04ff2dffff04ff5fffff04ffff0bffff0101ff0280ff8080
    808080ff8080808080ff80808080808080808080ffff04ffff02ff04ffff04ff
    04ffff04ffff04ff8209ffff81bf80ffff04ff82097fff8202ff80808080ff01
    8080ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ff
    ff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff
    0bffff0101ff038080ff0180ffff04ffff01ff0bffff0102ffff0bffff018201
    0280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0b
    ffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff04
    ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0b
    ffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ff
    ff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff01
    8201018080ff0180ffff01ff04ffff0133ffff04ffff02ff04ffff04ff06ffff
    04ff05ffff04ffff0bffff0101ff0780ff8080808080ffff04ff80ffff04ffff
    04ff05ff8080ff8080808080808080ff018080
    "
);

/// Tree hash of [`XCHANDLES_EXECUTE_UPDATE_PUZZLE`].
pub const XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    5ab577e34135251486330c53c8c16ed2b50d1b3da58096019c745b7faa94a631
    "
));

/// Curried arguments of the `XCHandles` execute update action
/// ([`XCHANDLES_EXECUTE_UPDATE_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExecuteUpdateActionArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Puzzle hash of the singleton launcher, used with
    /// [`singleton_mod_hash`](Self::singleton_mod_hash) to compute singleton puzzle hashes
    /// from launcher ids.
    pub singleton_launcher_mod_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for handle slots
    /// ([`XchandlesSlotNonce::HANDLE`](crate::puzzles::XchandlesSlotNonce::HANDLE)).
    pub handle_slot_1st_curry_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for update slots
    /// ([`XchandlesSlotNonce::UPDATE`](crate::puzzles::XchandlesSlotNonce::UPDATE)).
    pub update_slot_1st_curry_hash: Bytes32,
}

/// Inner puzzle hashes of the owner and resolved singletons that approve a change,
/// encoded as `(new_owner_inner_puzzle_hash . new_resolved_inner_puzzle_hash)`.
#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesNewDataPuzzleHashes {
    /// Inner puzzle hash of the new owner singleton's coin.
    pub new_owner_inner_puzzle_hash: Bytes32,
    /// Inner puzzle hash of the new resolved singleton's coin.
    #[clvm(rest)]
    pub new_resolved_inner_puzzle_hash: Bytes32,
}

impl XchandlesNewDataPuzzleHashes {
    /// Creates the value from its parts.
    pub fn new(
        new_owner_inner_puzzle_hash: Bytes32,
        new_resolved_inner_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            new_owner_inner_puzzle_hash,
            new_resolved_inner_puzzle_hash,
        }
    }
}

/// Solution to the `XCHandles` execute update action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExecuteUpdateActionSolution {
    /// Minimum execution height stored in the update slot.
    pub min_execution_height: u32,
    /// Current value of the handle's slot.
    pub current_slot_value: XchandlesHandleSlotValue,
    /// New owner and resolved launcher ids, as stored in the update slot.
    pub new_data: XchandlesDataValue,
    /// Proof of the current owner singleton coin, whose parent must be the coin that
    /// initiated the update.
    pub current_owner: CompactCoinProof,
    /// Inner puzzle hashes of the new owner and resolved singletons, which send the approval
    /// messages.
    #[clvm(rest)]
    pub new_data_puzzle_hashes: XchandlesNewDataPuzzleHashes,
}

impl Mod for XchandlesExecuteUpdateActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXECUTE_UPDATE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXECUTE_UPDATE_PUZZLE_HASH
    }
}
