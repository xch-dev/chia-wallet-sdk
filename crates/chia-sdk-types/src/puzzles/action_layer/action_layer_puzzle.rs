use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const ACTION_LAYER_PUZZLE: [u8; 808] = hex!(
    "
    ff02ffff01ff02ff05ffff04ff0bffff04ff17ffff04ffff02ff16ffff04ff02
    ffff04ff2fffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ffff04ffff
    02ff08ffff04ff02ffff04ff0bffff04ffff02ff14ffff04ff02ffff04ff2fff
    ff04ffff04ff80ff5f80ff8080808080ff8080808080ffff04ff81bfff808080
    8080808080ffff04ff82017fff808080808080ffff04ffff01ffffff02ffff03
    ffff09ff05ff1b80ffff0113ffff01ff088080ff0180ffff02ffff03ff2bffff
    01ff04ffff02ff1affff04ff02ffff04ffff05ffff02ff1cffff04ff02ffff04
    ff05ffff04ffff04ff2bff8080ff808080808080ffff04ffff05ffff02ff1cff
    ff04ff02ffff04ff05ffff04ffff04ff13ff3b80ff808080808080ff80808080
    80ffff0bffff0102ffff06ffff02ff1cffff04ff02ffff04ff05ffff04ffff04
    ff2bff8080ff808080808080ffff06ffff02ff1cffff04ff02ffff04ff05ffff
    04ffff04ff13ff3b80ff8080808080808080ffff01ff04ffff04ff3bff1380ff
    ff0bffff0101ffff02ff1effff04ff02ffff04ffff02ff3bff0580ff80808080
    808080ff0180ff02ffff03ffff07ff1b80ffff01ff02ff14ffff04ff02ffff04
    ff05ffff04ff0bff8080808080ffff010b80ff0180ffffff02ffff03ffff09ff
    05ff1380ffff01ff0101ffff01ff02ff12ffff04ff02ffff04ff05ffff04ff1b
    ff808080808080ff0180ff02ffff03ff05ffff01ff04ff09ffff02ff1affff04
    ff02ffff04ff0dffff04ff0bff808080808080ffff010b80ff0180ffff02ffff
    03ff81dfffff01ff02ffff03ffff02ff12ffff04ff02ffff04ff82011fffff04
    ff2fff8080808080ffff01ff02ff16ffff04ff02ffff04ff05ffff04ffff04ff
    37ff0b80ffff04ffff02ffff02ff82011fff0580ffff04ff27ffff04ff82019f
    ff80808080ffff04ff2fffff04ff81dfff8080808080808080ffff01ff088080
    ff0180ffff01ff04ff27ffff04ff37ff0b808080ff0180ff02ffff03ffff07ff
    0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ff
    ff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff0580
    80ff0180ff018080
    "
);

pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    36c675ccbaf385caeb0d8ee34bffc1ce8b0d4fc9d9dba2b1c2c9bcae75ca7659
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct ActionLayerArgs<F, S> {
    pub finalizer: F,
    pub merkle_root: Bytes32,
    pub state: S,
}

impl<F, S> ActionLayerArgs<F, S> {
    pub fn new(finalizer: F, merkle_root: Bytes32, state: S) -> Self {
        Self {
            finalizer,
            merkle_root,
            state,
        }
    }
}

impl ActionLayerArgs<TreeHash, TreeHash> {
    pub fn curry_tree_hash(
        finalizer: TreeHash,
        merkle_root: Bytes32,
        state_hash: TreeHash,
    ) -> TreeHash {
        CurriedProgram {
            program: ACTION_LAYER_PUZZLE_HASH,
            args: ActionLayerArgs::<TreeHash, TreeHash>::new(finalizer, merkle_root, state_hash),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RawActionLayerSolution<P, S, F> {
    pub puzzles: Vec<P>,
    pub selectors_and_proofs: Vec<(u32, Option<MerkleProof>)>,
    pub solutions: Vec<S>,
    pub finalizer_solution: F,
}

impl<P, S, F> Mod for RawActionLayerSolution<P, S, F> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ACTION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        ACTION_LAYER_PUZZLE_HASH
    }
}

impl<P, S> Mod for ActionLayerArgs<P, S> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ACTION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        ACTION_LAYER_PUZZLE_HASH
    }
}
