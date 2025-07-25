use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const ACTION_LAYER_PUZZLE: [u8; 670] = hex!(
    "
    ff02ffff01ff02ff05ffff04ff0bffff04ff17ffff04ffff02ff0affff04ff02
    ffff04ff2fffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ffff04ffff
    02ff0cffff04ff02ffff04ff0bffff04ff2fffff04ff80ffff04ff5fff808080
    80808080ffff04ff81bfff8080808080808080ffff04ff82017fff8080808080
    80ffff04ffff01ffffff02ffff03ffff09ff05ff1380ffff01ff0101ffff01ff
    02ff08ffff04ff02ffff04ff05ffff04ff1bff808080808080ff0180ff02ffff
    03ff2fffff01ff02ffff03ffff02ffff03ff81cfffff01ff09ff05ffff02ff1e
    ffff04ff02ffff04ffff0bffff0101ffff02ff16ffff04ff02ffff04ffff02ff
    818fff0b80ff8080808080ffff04ff81cfff808080808080ffff01ff02ff08ff
    ff04ff02ffff04ff818fffff04ff17ff808080808080ff0180ffff01ff02ff0c
    ffff04ff02ffff04ff05ffff04ff0bffff04ffff04ff818fff1780ffff04ff6f
    ff80808080808080ffff01ff088080ff0180ffff011780ff0180ffff02ffff03
    ff2fffff01ff02ff0affff04ff02ffff04ff05ffff04ffff04ff37ff0b80ffff
    04ffff02ffff02ff4fff0580ffff04ff27ffff04ff819fff80808080ffff04ff
    6fffff04ff81dfff8080808080808080ffff01ff04ff27ffff04ff37ff0b8080
    80ff0180ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff
    04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ffff04ff0dff808080
    8080ffff01ff0bffff0101ff058080ff0180ff02ffff03ff1bffff01ff02ff1e
    ffff04ff02ffff04ffff02ffff03ffff18ffff0101ff1380ffff01ff0bffff01
    02ff2bff0580ffff01ff0bffff0102ff05ff2b8080ff0180ffff04ffff04ffff
    17ff13ffff0181ff80ff3b80ff8080808080ffff010580ff0180ff018080
    "
);

pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    2ad6e558c952fb62de6428fb8d627bcd21ddf37aa8aabb43a8620d98e922a163
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
