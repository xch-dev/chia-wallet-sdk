use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

pub const ACTION_LAYER_PUZZLE: [u8; 771] = hex!(
    "
    ff02ffff01ff02ff05ffff04ff0bffff04ff17ffff04ffff02ff16ffff04ff02
    ffff04ff2fffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ffff04ffff
    02ff08ffff04ff02ffff04ff0bffff04ffff02ff14ffff04ff02ffff04ff2fff
    ff04ffff04ff80ff5f80ff8080808080ff8080808080ffff04ff81bfff808080
    8080808080ffff04ff82017fff808080808080ffff04ffff01ffffff03ffff09
    ff05ff1b80ff13ff8080ffff02ffff03ff2bffff01ff04ffff02ff1affff04ff
    02ffff04ffff05ffff02ff1cffff04ff02ffff04ff05ffff04ffff04ff80ff2b
    80ff808080808080ffff04ffff05ffff02ff1cffff04ff02ffff04ff05ffff04
    ffff04ff13ff3b80ff808080808080ff8080808080ffff0bffff0102ffff06ff
    ff02ff1cffff04ff02ffff04ff05ffff04ffff04ff80ff2b80ff808080808080
    ffff06ffff02ff1cffff04ff02ffff04ff05ffff04ffff04ff13ff3b80ff8080
    808080808080ffff01ff04ffff04ff3bff1380ffff0bffff0101ffff02ff1eff
    ff04ff02ffff04ffff02ff3bff0580ff80808080808080ff0180ff02ffff03ff
    ff07ff1b80ffff01ff02ff14ffff04ff02ffff04ff05ffff04ff0bff80808080
    80ffff010b80ff0180ffffff02ffff03ffff09ff05ff1380ffff0105ffff01ff
    02ff12ffff04ff02ffff04ff05ffff04ff1bff808080808080ff0180ff02ffff
    03ff05ffff01ff04ff09ffff02ff1affff04ff02ffff04ff0dffff04ff0bff80
    8080808080ffff010b80ff0180ffff02ffff03ff5fffff01ff02ff16ffff04ff
    02ffff04ff05ffff04ffff04ff37ff0b80ffff04ffff02ffff02ffff02ff12ff
    ff04ff02ffff04ff82011fffff04ff2fff8080808080ff0580ffff04ff27ffff
    04ff82019fff80808080ffff04ff2fffff04ff81dfff8080808080808080ffff
    01ff04ff27ffff04ff37ff0b808080ff0180ff02ffff03ffff07ff0580ffff01
    ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff1eff
    ff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff
    018080
    "
);

pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    45dee8d1a78d7509b7cb46e4593e430a54d598e708b677eba33c36eda29aa707
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
pub struct RawActionLayerSolution<P, R, S, F> {
    pub puzzles: Vec<P>,
    pub partial_tree_reveal: R,
    pub selectors_and_solutions: Vec<(u32, S)>,
    pub finalizer_solution: F,
}

impl<P, S> Mod for ActionLayerArgs<P, S> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&ACTION_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        ACTION_LAYER_PUZZLE_HASH
    }
}
