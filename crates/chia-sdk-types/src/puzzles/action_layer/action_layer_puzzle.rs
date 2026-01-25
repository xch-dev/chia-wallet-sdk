use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const ACTION_LAYER_PUZZLE: [u8; 657] = hex!(
    "
    ff02ffff01ff02ff05ffff04ffff04ff0bff1780ffff04ffff02ff0affff04ff
    02ffff04ff2fffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ffff04ff
    ff02ff0cffff04ff02ffff04ff0bffff04ff2fffff04ff80ffff04ff5fff8080
    8080808080ffff04ff81bfff8080808080808080ff81ff808080ffff04ffff01
    ffffff02ffff03ffff09ff05ff1380ffff01ff0101ffff01ff02ff08ffff04ff
    02ffff04ff05ffff04ff1bff808080808080ff0180ff02ffff03ff2fffff01ff
    02ffff03ffff02ffff03ff81cfffff01ff09ff05ffff02ff1effff04ff02ffff
    04ffff0bffff0101ffff02ff16ffff04ff02ffff04ffff02ff818fff0b80ff80
    80808080ffff04ff81cfff808080808080ffff01ff02ff08ffff04ff02ffff04
    ff818fffff04ff17ff808080808080ff0180ffff01ff02ff0cffff04ff02ffff
    04ff05ffff04ff0bffff04ffff04ff818fff1780ffff04ff6fff808080808080
    80ffff01ff088080ff0180ffff011780ff0180ffff02ffff03ff2fffff01ff02
    ff0affff04ff02ffff04ff05ffff04ffff04ff37ff0b80ffff04ffff02ffff02
    ff4fff0580ffff04ff27ff819f8080ffff04ff6fffff04ff81dfff8080808080
    808080ffff01ff04ff27ffff04ff37ff0b808080ff0180ffff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff80808080
    ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff02ffff03ff1bffff01ff02ff1effff04ff02ffff04ffff02ffff
    03ffff18ffff0101ff1380ffff01ff0bffff0102ff2bff0580ffff01ff0bffff
    0102ff05ff2b8080ff0180ffff04ffff04ffff17ff13ffff0181ff80ff3b80ff
    8080808080ffff010580ff0180ff018080
    "
);

pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d07619284300be4855f57b76789932021f60e217e000edfee07f9ca7e4b3f49a
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
    #[clvm(rest)]
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
