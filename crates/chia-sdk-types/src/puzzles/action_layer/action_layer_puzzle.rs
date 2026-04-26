use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const ACTION_LAYER_PUZZLE: [u8; 630] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ff5fffff01ff02ff05ffff04ffff04ff0bff1780ffff
    04ffff02ff1effff04ff1effff04ffff04ffff02ff0cffff04ffff04ff04ffff
    04ff0aff168080ffff04ffff04ff80ff5f80ffff04ff0bff2f80808080ff2f80
    ffff04ff80ffff04ffff04ffff04ff80ff1780ff8080ff81bf8080808080ff81
    ff808080ffff01ff088080ff0180ffff04ffff04ffff04ffff01ff02ffff03ff
    ff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff
    02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff01ff02
    ffff03ff0dffff01ff02ffff03ffff02ffff03ff35ffff01ff09ff0bffff02ff
    0affff04ff0affff04ff35ffff0bffff0101ffff02ff08ffff04ff08ffff02ff
    25ff0f8080808080808080ffff01ff02ff0effff04ff0effff04ff25ff098080
    8080ff0180ffff01ff02ff0cffff04ff02ffff04ffff04ffff04ff25ff0980ff
    1d80ff07808080ffff01ff088080ff0180ffff010980ff018080ffff04ffff01
    ff02ffff03ff0dffff01ff02ff02ffff04ff02ffff04ffff04ffff17ff09ffff
    0181ff80ff1d80ffff0bffff0102ffff03ffff18ff09ffff010180ff15ff0780
    ffff03ffff18ff09ffff010180ff07ff158080808080ffff010780ff0180ffff
    04ffff01ff02ffff03ffff09ff0bff0580ffff01ff0101ffff01ff02ff02ffff
    04ff02ffff04ff05ff0f80808080ff0180ffff01ff02ffff03ff09ffff01ff02
    ff02ffff04ff02ffff04ffff04ff19ff0d80ffff04ffff04ff37ff0b80ffff04
    ffff02ffff02ff11ff0d80ffff04ff27ff2f8080ff3f8080808080ffff01ff04
    ff27ffff04ff37ff0b808080ff0180808080ff018080
    "
);

pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b
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
