use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{PuzzleHashPuzzleAndSolution, XchandlesHandleSlotValue, XchandlesOtherPrecommitData},
};

/// Serialized `XCHandles` refund action, compiled from Rue.
///
/// Refunds a precommit coin that can't be used. If the precommit's CAT maker, handle,
/// pricing puzzle (normal or expired handle pricing), and amount all match the current
/// state, the coin could otherwise be used, so the action requires the handle's slot to
/// be spent and re-created before its expiration to prove the handle is taken. In all
/// cases it tells the precommit coin to refund and announces `"$"` followed by the
/// precommit coin's puzzle hash.
pub const XCHANDLES_REFUND_PUZZLE: [u8; 920] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff09ff819fffff02ff04ffff04ff04
    ff82015f808080ffff09ff4fffff02ff04ffff04ff04ff81af80808080ffff02
    ffff03ff8202ffffff01ff09ffff0bffff0101ff81bf80ff8212ff80ffff01ff
    010180ff018080ffff01ff02ffff01ff04ff2fffff04ffff04ffff0142ffff04
    ffff0113ffff04ff80ffff04ff02ffff04ff8202ffff808080808080ffff04ff
    ff04ffff013effff04ffff0effff0124ff0280ff808080ffff02ffff03ffff22
    ffff22ffff22ffff09ff81afff82013f80ffff09ff82017fff820bdf8080ffff
    21ffff09ff819fff82016f80ffff09ff819fff8201ef808080ffff09ff8202ff
    ffff05ffff02ff82015fff8201df80808080ffff01ff04ffff04ffff0155ffff
    04ff822dffff808080ffff04ffff04ffff0142ffff04ffff0112ffff04ff80ff
    ff04ffff02ff15ffff04ff2dffff04ff17ffff04ffff0bffff0101ffff02ff09
    ffff04ff09ff8205ff808080ff8080808080ff8080808080ffff04ffff02ff3d
    ffff04ffff04ff15ff2d80ffff04ff17ffff02ff09ffff04ff09ffff04ffff10
    ff8209ffffff010180ff820dff808080808080ff80808080ffff018080ff0180
    808080ffff04ffff02ff82015fffff04ffff02ff0affff04ff16ffff04ff05ff
    ff04ff820bffffff04ffff0bffff0101ffff02ff04ffff04ff04ffff04ffff04
    ffff04ff819fff8201df80ffff04ff4fff81ef8080ffff04ffff04ff81bfff82
    0fff80ff8205ff8080808080ff808080808080ff8201df8080ff018080ffff01
    ff088080ff0180ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0b
    ffff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff078080
    80ffff01ff0bffff0101ff038080ff0180ffff04ffff01ff0bffff0102ffff0b
    ffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff
    0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff01018080
    8080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff018201
    0480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0b
    ffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff01
    ff0bffff018201018080ff0180ffff01ff04ffff0133ffff04ffff02ff04ffff
    04ff06ffff04ff05ffff04ffff0bffff0101ff0780ff8080808080ffff04ff80
    ffff04ffff04ff05ff8080ff8080808080808080ff018080
    "
);

/// Tree hash of [`XCHANDLES_REFUND_PUZZLE`].
pub const XCHANDLES_REFUND_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    a11cc9b151475ef6cc4b6e1846089dcf9c042413b7a5a943e6b11c4e0f59fcc7
    "
));

/// Curried arguments of the `XCHandles` refund action ([`XCHANDLES_REFUND_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRefundActionArgs {
    /// Hash of the precommit layer after its first curry
    /// ([`PrecommitLayer1stCurryArgs`](crate::puzzles::PrecommitLayer1stCurryArgs)).
    pub precommit_1st_curry_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for handle slots
    /// ([`XchandlesSlotNonce::HANDLE`](crate::puzzles::XchandlesSlotNonce::HANDLE)).
    pub handle_slot_1st_curry_hash: Bytes32,
}

/// Solution to the `XCHandles` refund action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRefundActionSolution<CMP, CMS, PP, PS, S> {
    /// Pricing puzzle hash, puzzle, and solution that the precommit coin was created with.
    pub precommited_pricing_puzzle_and_solution: PuzzleHashPuzzleAndSolution<PP, PS>,
    /// CAT maker puzzle hash, puzzle, and solution that the precommit coin was created with.
    pub precommited_cat_maker_and_solution: PuzzleHashPuzzleAndSolution<CMP, CMS>,
    /// Handle the precommit coin committed to.
    pub handle: String,
    /// Amount of the precommit coin.
    pub precommit_amount: u64,
    /// Current value of the handle's slot. Required when the precommit could otherwise be
    /// used; `None` otherwise.
    pub slot_value: Option<XchandlesHandleSlotValue>,
    /// Rest of the data the precommit coin committed to.
    #[clvm(rest)]
    pub other_precommit_data: XchandlesOtherPrecommitData<S>,
}

impl Mod for XchandlesRefundActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_REFUND_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_REFUND_PUZZLE_HASH
    }
}
