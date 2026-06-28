use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{Mod, puzzles::ACTION_LAYER_PUZZLE_HASH};

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

pub const RESERVE_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    5b406eb66d10998027924c9bdea4254966e27ca765a8992227f826cb0e868095
    "
));

// run '(mod state (f state))' -d
pub const RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM: [u8; 1] = hex!("02");
pub const RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH: TreeHash =
    TreeHash::new(hex!(
        "a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222"
    ));

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

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ReserveFinalizer1stCurryArgs<P> {
    pub action_layer_mod_hash: Bytes32,
    pub reserve_full_puzzle_hash: Bytes32,
    pub reserve_inner_puzzle_hash: Bytes32,
    pub reserve_amount_from_state_program: P,
    pub hint: Bytes32,
}

impl<P> ReserveFinalizer1stCurryArgs<P> {
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

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ReserveFinalizer2ndCurryArgs {
    pub finalizer_self_hash: Bytes32,
}

impl ReserveFinalizer2ndCurryArgs {
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

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(transparent)]
pub struct ReserveFinalizerSolution {
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
