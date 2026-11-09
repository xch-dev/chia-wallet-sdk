use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{
        PuzzleAndSolution, SlotNeigborsInfo, XchandlesDataValue, XchandlesNewDataPuzzleHashes,
    },
};

/// Serialized `XCHandles` expire action, compiled from Rue.
///
/// Registers an expired handle to a new owner. The handle's current expiration must have
/// passed, and the price comes from the expired handle pricing puzzle in the registry's
/// state (for example [`XchandlesExponentialPremiumRenewPuzzleArgs`](crate::puzzles::XchandlesExponentialPremiumRenewPuzzleArgs),
/// which adds a decaying premium). The action consumes a precommit coin whose amount
/// equals the price, re-creates the handle slot with the new data and an expiration of
/// `buy_time + registered_time`, requires a message of `"e"` followed by the precommit
/// coin's puzzle hash from the new owner singleton (and `"f"` followed by it from the
/// resolved singleton if different), and announces `"x"` followed by that puzzle hash.
pub const XCHANDLES_EXPIRE_PUZZLE: [u8; 1151] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff02ff04ffff04ff04ff82027f8080
    ff82015f80ffff09ffff02ff04ffff04ff04ff82013f8080ff8203df8080ffff
    01ff02ffff01ff02ffff01ff02ffff01ff04ff8202ffffff02ffff03ffff09ff
    8247ffff8267ff80ffff0102ffff01ff04ffff04ffff0143ffff04ffff0112ff
    ff04ffff0effff0166ff0580ffff04ffff02ff57ffff04ff81b7ffff04ff2fff
    ff04ffff02ff27ffff04ff27ffff04ff2fffff04ff8267ffff5f80808080ffff
    04ff8301ffffff808080808080ff8080808080ff028080ff018080ffff04ffff
    04ffff04ffff0151ffff04ff820affff808080ffff04ffff04ffff0151ffff04
    ff8216ffff808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ffff
    04ffff02ff2bffff04ff5bffff04ff81bfffff04ffff0bffff0101ffff02ff13
    ffff04ff13ffff04ff8217ffffff04ffff04ff09ff822fff80ffff04ff8216ff
    ff825fff808080808080ff8080808080ff8080808080ffff04ffff02ff7bffff
    04ffff04ff2bff5b80ffff04ff81bfffff02ff13ffff04ff13ffff04ffff10ff
    8217ffffff010180ffff04ffff04ff09ff822fff80ffff04ffff10ff1dff820a
    ff80ff8213ff8080808080808080ffff04ffff04ffff0142ffff04ffff0113ff
    ff04ffff0101ffff04ff02ffff04ff15ff808080808080ffff04ffff04ffff01
    3effff04ffff0effff0178ff0280ff808080ffff04ffff04ffff0143ffff04ff
    ff0112ffff04ffff0effff0165ff0280ffff04ffff02ff2bffff04ff5bffff04
    ff17ffff04ffff02ff13ffff04ff13ffff04ff17ffff04ff8223ffff2f808080
    80ffff04ff82bfffff808080808080ff8080808080ff8080808080808080ff01
    8080ffff04ffff02ff8204ffffff04ffff02ff15ffff04ff2dffff04ff2fffff
    04ff8215ffffff04ffff0bffff0101ffff02ff09ffff04ff09ffff04ffff04ff
    ff04ff8202bfff8206ff80ffff04ff8207bfff82037f8080ffff04ffff04ff82
    177fff821dff80ff8209ff8080808080ff808080808080ff8206ff8080ff0180
    80ffff04ffff04ffff0bffff0101ff820bbf80ffff02ff82013fff8201bf8080
    ff018080ffff01ff088080ff0180ffff04ffff04ffff01ff02ffff03ffff07ff
    0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff
    04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff04ffff01ff0b
    ffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bff
    ff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff
    0bffff010180808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ff
    ff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff01820101
    80ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff0101
    80808080ffff01ff0bffff018201018080ff0180ffff01ff04ffff0133ffff04
    ffff02ff04ffff04ff06ffff04ff05ffff04ffff0bffff0101ff0780ff808080
    8080ffff04ff80ffff04ffff04ff05ff8080ff8080808080808080ff018080
    "
);

/// Tree hash of [`XCHANDLES_EXPIRE_PUZZLE`].
pub const XCHANDLES_EXPIRE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ee1554255f27b737b73dbba2d3fc2e3d7e8de2954c35c8660341362410d5b4e0
    "
));

/// Curried arguments of the `XCHandles` expire action ([`XCHANDLES_EXPIRE_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExpireActionArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Puzzle hash of the singleton launcher, used with
    /// [`singleton_mod_hash`](Self::singleton_mod_hash) to compute singleton puzzle hashes
    /// from launcher ids.
    pub singleton_launcher_mod_hash: Bytes32,
    /// Hash of the precommit layer after its first curry
    /// ([`PrecommitLayer1stCurryArgs`](crate::puzzles::PrecommitLayer1stCurryArgs)).
    pub precommit_1st_curry_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for handle slots
    /// ([`XchandlesSlotNonce::HANDLE`](crate::puzzles::XchandlesSlotNonce::HANDLE)).
    pub handle_slot_1st_curry_hash: Bytes32,
}

/// Refund information and secret committed to by an `XCHandles` precommit coin, encoded
/// as `(refund_puzzle_hash_hash . secret)`.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRefundAndSecret<S> {
    /// Tree hash of the precommit coin's refund puzzle hash.
    pub refund_puzzle_hash_hash: Bytes32,
    /// Secret included in the precommitted value, so the handle can't be guessed from the
    /// precommit coin's puzzle hash before it is revealed.
    #[clvm(rest)]
    pub secret: S,
}

impl<S> XchandlesRefundAndSecret<S> {
    /// Creates the value from its parts.
    pub fn new(refund_puzzle_hash_hash: Bytes32, secret: S) -> Self {
        Self {
            refund_puzzle_hash_hash,
            secret,
        }
    }
}

/// Data committed to by an `XCHandles` precommit coin, besides the CAT maker, pricing
/// puzzle, and handle. Encoded as `(launcher_ids . refund_and_secret)`.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesOtherPrecommitData<S> {
    /// Owner and resolved launcher ids the handle will have after the registration.
    pub launcher_ids: XchandlesDataValue,
    /// Refund puzzle hash and secret.
    #[clvm(rest)]
    pub refund_and_secret: XchandlesRefundAndSecret<S>,
}

impl<S> XchandlesOtherPrecommitData<S> {
    /// Creates the value from its parts.
    pub fn new(
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
        refund_puzzle_hash_hash: Bytes32,
        secret: S,
    ) -> Self {
        Self {
            launcher_ids: XchandlesDataValue {
                owner_launcher_id,
                resolved_launcher_id,
            },
            refund_and_secret: XchandlesRefundAndSecret::new(refund_puzzle_hash_hash, secret),
        }
    }
}

/// Solution to the `XCHandles` expire action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExpireActionSolution<CMP, CMS, EP, ES, S> {
    /// Expired handle pricing puzzle and its solution (usually an
    /// [`XchandlesPricingSolution`](crate::puzzles::XchandlesPricingSolution) whose current
    /// expiration is the handle's expiration). The puzzle must match the expired handle
    /// pricing puzzle hash in the registry's state.
    pub expired_handle_pricing_puzzle_and_solution: PuzzleAndSolution<EP, ES>,
    /// CAT maker puzzle and solution for the payment asset. The puzzle must match the CAT
    /// maker puzzle hash in the registry's state.
    pub cat_maker_and_solution: PuzzleAndSolution<CMP, CMS>,
    /// Rest of the data the precommit coin committed to, including the new owner and
    /// resolved launcher ids.
    pub other_precommit_data: XchandlesOtherPrecommitData<S>,
    /// Counter of the handle's slot.
    pub counter: u64,
    /// Neighbors of the handle's slot, which stay the same.
    pub neighbors: SlotNeigborsInfo,
    /// Owner and resolved launcher ids currently in the handle's slot.
    pub old_rest: XchandlesDataValue,
    /// Inner puzzle hashes of the new owner and resolved singletons, which send the approval
    /// messages.
    #[clvm(rest)]
    pub new_inner_puzzle_hashes: XchandlesNewDataPuzzleHashes,
}

impl Mod for XchandlesExpireActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXPIRE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXPIRE_PUZZLE_HASH
    }
}
