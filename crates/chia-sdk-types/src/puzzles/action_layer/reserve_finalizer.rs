use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{puzzles::ACTION_LAYER_PUZZLE_HASH, Mod};

pub const RESERVE_FINALIZER_PUZZLE: [u8; 884] = hex!(
    "
    ff02ffff01ff04ffff04ff10ffff04ffff02ff1affff04ff02ffff04ff05ffff
    04ffff02ff1affff04ff02ffff04ff81bfffff04ffff0bffff0101ff81bf80ff
    8080808080ffff04ffff0bffff0101ff82017f80ffff04ffff02ff3effff04ff
    02ffff04ff8219ffff80808080ff80808080808080ffff04ffff0101ffff04ff
    ff04ff5fff8080ff8080808080ffff04ffff04ff18ffff04ffff0117ffff04ff
    ff02ff3effff04ff02ffff04ffff04ffff0101ffff04ffff04ff10ffff04ff17
    ffff04ffff02ff2fff8219ff80ffff04ffff04ff17ff8080ff8080808080ffff
    06ffff02ff2effff04ff02ffff04ff820dffffff01ff80ff8080808080808080
    ff80808080ffff04ffff30ff8213ffff0bffff02ff2fff8202ff8080ff808080
    8080ffff05ffff02ff2effff04ff02ffff04ff820dffffff01ff80ff80808080
    80808080ffff04ffff01ffffff3342ff02ff02ffff03ff05ffff01ff0bff72ff
    ff02ff16ffff04ff02ffff04ff09ffff04ffff02ff1cffff04ff02ffff04ff0d
    ff80808080ff808080808080ffff016280ff0180ffffffffa04bf5122f344554
    c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f3
    2623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871
    fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8
    d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff
    0bff52ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff1cffff04ff02ff
    ff04ff07ff80808080ff808080808080ffff0bff14ffff0bff14ff62ff0580ff
    ff0bff14ff0bff428080ffff02ffff03ff09ffff01ff02ffff03ffff09ff21ff
    ff0181d680ffff01ff02ff2effff04ff02ffff04ffff04ff19ff0d80ffff04ff
    0bffff04ffff04ff31ff1780ff808080808080ffff01ff02ff2effff04ff02ff
    ff04ffff04ff19ff0d80ffff04ffff04ff11ff0b80ffff04ff17ff8080808080
    8080ff0180ffff01ff02ffff03ff0dffff01ff02ff2effff04ff02ffff04ff0d
    ffff04ff0bffff04ff17ff808080808080ffff01ff04ff0bff178080ff018080
    ff0180ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff
    02ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080
    ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const RESERVE_FINALIZER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d277207ecea05d2b6a3874ef3bf5831cd224527eedab8c000a03b5511fb511de
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
#[clvm(list)]
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
