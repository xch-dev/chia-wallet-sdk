use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{Mod, puzzles::ACTION_LAYER_PUZZLE_HASH};

/// Serialized reserve finalizer puzzle, compiled from Rue.
///
/// It does everything [`DEFAULT_FINALIZER_PUZZLE`](crate::puzzles::DEFAULT_FINALIZER_PUZZLE)
/// does, and also spends and re-creates a reserve coin controlled by the singleton
/// through [`P2DelegatedBySingletonLayerArgs`](crate::puzzles::P2DelegatedBySingletonLayerArgs).
/// Actions route conditions to the reserve by prefixing them with `-42`; the finalizer
/// removes those conditions from the singleton's output and has the reserve output them
/// instead, along with a `CREATE_COIN` that re-creates the reserve with the amount
/// computed from the new state.
///
/// Use [`ReserveFinalizer1stCurryArgs`] and [`ReserveFinalizer2ndCurryArgs`] to curry it.
pub const RESERVE_FINALIZER_PUZZLE: [u8; 739] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ffff04ffff0133ffff04ffff02ff2dffff04ff3d
    ffff04ff0bffff04ffff02ff2dffff04ff3dffff04ff82017fffff04ffff0bff
    ff0101ff82017f80ff8080808080ffff04ffff0bffff0101ff8204ff80ffff04
    ffff02ff15ffff04ff15ff8219ff8080ff80808080808080ffff04ffff0101ff
    ff04ffff04ff81bfff8080ff8080808080ffff04ffff04ffff0142ffff04ffff
    0117ffff04ffff02ff15ffff04ff15ffff04ffff0101ffff04ffff04ffff0133
    ffff04ff2fffff04ffff02ff5fff8219ff80ffff04ffff04ff2fff8080ff8080
    808080ff0680808080ffff04ffff30ff8207ffff17ffff02ff5fff8206ff8080
    ff8080808080ff048080ffff04ffff02ff04ffff04ff04ffff04ff80ffff04ff
    80ff8206ff80808080ff018080ffff04ffff04ffff01ff02ffff03ff17ffff01
    ff02ffff03ffff09ff47ffff0181d680ffff01ff02ff02ffff04ff02ffff04ff
    05ffff04ffff04ff67ff0b80ffff04ff37ff1f8080808080ffff01ff02ff02ff
    ff04ff02ffff04ffff04ff27ff0580ffff04ff0bffff04ff37ff1f8080808080
    80ff0180ffff01ff02ffff03ff1fffff01ff02ff02ffff04ff02ffff04ff05ff
    ff04ff0bff1f80808080ffff01ff04ff05ff0b8080ff018080ff0180ffff04ff
    ff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02
    ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080
    ff0180ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102
    ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02
    ffff04ff02ff078080ffff0bffff010180808080ffff01ff02ffff03ff03ffff
    01ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ff
    ff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff0780
    80ffff0bffff010180808080ffff01ff0bffff018201018080ff0180808080ff
    018080
    "
);

/// Tree hash of [`RESERVE_FINALIZER_PUZZLE`].
pub const RESERVE_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    5b406eb66d10998027924c9bdea4254966e27ca765a8992227f826cb0e868095
    "
));

/// Default program that computes the reserve amount from the state: `(f state)`, the
/// first item of the state.
// run '(mod state (f state))' -d
pub const RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM: [u8; 1] = hex!("02");
/// Tree hash of [`RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM`].
pub const RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH: TreeHash =
    TreeHash::new(hex!(
        "a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222"
    ));

/// Marker for [`RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM`], which takes
/// no curried arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultReserveAmountFromStateProgramArgs {}

impl Mod for DefaultReserveAmountFromStateProgramArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM)
    }

    fn mod_hash() -> TreeHash {
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH
    }
}

/// First curry of the reserve finalizer ([`RESERVE_FINALIZER_PUZZLE`]).
///
/// The result is curried again with [`ReserveFinalizer2ndCurryArgs`] so the finalizer
/// can refer to its own hash.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ReserveFinalizer1stCurryArgs<P> {
    /// Mod hash of the action layer ([`ACTION_LAYER_PUZZLE_HASH`]), used to compute the
    /// re-created singleton's inner puzzle hash.
    pub action_layer_mod_hash: Bytes32,
    /// Full puzzle hash of the reserve coin (for example a CAT whose inner puzzle is
    /// [`reserve_inner_puzzle_hash`](Self::reserve_inner_puzzle_hash)), used to compute the
    /// current reserve's coin id.
    pub reserve_full_puzzle_hash: Bytes32,
    /// Inner puzzle hash of the reserve, used for the re-created reserve coin and as its hint.
    pub reserve_inner_puzzle_hash: Bytes32,
    /// Program run with the state as its environment that returns the reserve amount. The
    /// default is [`RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM`].
    pub reserve_amount_from_state_program: P,
    /// Hint (memo) added to the coin that re-creates the singleton, usually the launcher id.
    pub hint: Bytes32,
}

impl<P> ReserveFinalizer1stCurryArgs<P> {
    /// Creates the arguments with the standard action layer mod hash.
    pub fn new(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: P,
        hint: Bytes32,
    ) -> Self {
        Self {
            action_layer_mod_hash: ACTION_LAYER_PUZZLE_HASH.into(),
            reserve_full_puzzle_hash,
            reserve_inner_puzzle_hash,
            reserve_amount_from_state_program,
            hint,
        }
    }

    /// Computes the hash of the finalizer after the first curry, given the tree hash of the
    /// reserve amount program.
    pub fn curry_tree_hash(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: TreeHash,
        hint: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: RESERVE_FINALIZER_PUZZLE_HASH,
            args: ReserveFinalizer1stCurryArgs::new(
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program,
                hint,
            ),
        }
        .tree_hash()
    }
}

/// Second curry of the reserve finalizer. The resulting hash is what goes into
/// [`ActionLayerArgs::finalizer`](crate::puzzles::ActionLayerArgs::finalizer).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ReserveFinalizer2ndCurryArgs {
    /// Hash of the finalizer after the first curry (see
    /// [`ReserveFinalizer1stCurryArgs::curry_tree_hash`]).
    pub finalizer_self_hash: Bytes32,
}

impl ReserveFinalizer2ndCurryArgs {
    /// Creates the arguments by computing the first curry's hash from its parts.
    pub fn new<P>(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: &P,
        hint: Bytes32,
    ) -> Self
    where
        P: ToTreeHash,
    {
        Self {
            finalizer_self_hash: ReserveFinalizer1stCurryArgs::<TreeHash>::curry_tree_hash(
                reserve_full_puzzle_hash,
                reserve_inner_puzzle_hash,
                reserve_amount_from_state_program.tree_hash(),
                hint,
            )
            .into(),
        }
    }

    /// Computes the full finalizer puzzle hash (after both curries).
    pub fn curry_tree_hash(
        reserve_full_puzzle_hash: Bytes32,
        reserve_inner_puzzle_hash: Bytes32,
        reserve_amount_from_state_program: TreeHash,
        hint: Bytes32,
    ) -> TreeHash {
        let self_hash = ReserveFinalizer1stCurryArgs::<TreeHash>::curry_tree_hash(
            reserve_full_puzzle_hash,
            reserve_inner_puzzle_hash,
            reserve_amount_from_state_program,
            hint,
        );

        CurriedProgram {
            program: self_hash,
            args: ReserveFinalizer2ndCurryArgs {
                finalizer_self_hash: self_hash.into(),
            },
        }
        .tree_hash()
    }
}

/// Finalizer solution for [`RESERVE_FINALIZER_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(transparent)]
pub struct ReserveFinalizerSolution {
    /// Parent coin id of the reserve coin being spent alongside the singleton. The
    /// finalizer combines it with the reserve's full puzzle hash and the amount computed from
    /// the initial state to address its message to that coin.
    pub reserve_parent_id: Bytes32,
}

impl<P> Mod for ReserveFinalizer1stCurryArgs<P> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&RESERVE_FINALIZER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        RESERVE_FINALIZER_PUZZLE_HASH
    }
}
