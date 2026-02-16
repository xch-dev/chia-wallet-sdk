use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::{MerkleProof, Mod};

pub const ACTION_LAYER_PUZZLE: [u8; 618] = hex!(
    // Rue
    "
    ff02ffff01ff02ff05ffff04ffff04ff0bff1780ffff04ffff02ff1effff04ff
    1effff04ffff04ff80ffff02ff0cffff04ffff04ff04ffff04ff0aff168080ff
    ff04ffff04ff80ff5f80ffff04ff0bff2f8080808080ffff04ff2fffff04ffff
    04ffff04ff80ff1780ff8080ff81bf8080808080ff81ff808080ffff04ffff04
    ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ff
    ff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff0101
    ff038080ff0180ffff01ff02ffff03ff0dffff01ff02ffff03ffff02ffff03ff
    35ffff01ff09ff0bffff02ff0affff04ff0affff04ff35ffff0bffff0101ffff
    02ff08ffff04ff08ffff02ff25ff0f8080808080808080ffff01ff02ff0effff
    04ff0effff04ff25ff0980808080ff0180ffff01ff02ff0cffff04ff02ffff04
    ffff04ffff04ff25ff0980ff1d80ff07808080ffff01ff088080ff0180ffff01
    0980ff018080ffff04ffff01ff02ffff03ff0dffff01ff02ff02ffff04ff02ff
    ff04ffff04ffff17ff09ffff0181ff80ff1d80ffff0bffff0102ffff03ffff18
    ff09ffff010180ff15ff0780ffff03ffff18ff09ffff010180ff07ff15808080
    8080ffff010780ff0180ffff04ffff01ff02ffff03ffff09ff0bff0580ffff01
    ff0101ffff01ff02ff02ffff04ff02ffff04ff05ff0f80808080ff0180ffff01
    ff02ffff03ff0dffff01ff02ff02ffff04ff02ffff04ffff04ffff03ff37ffff
    04ff37ff0980ff0980ff1d80ffff04ff0bffff04ffff02ffff02ff15ff0b80ff
    ff04ff27ff2f8080ff3f8080808080ffff01ff04ff27ffff04ff37ff09808080
    ff0180808080ff018080
    "
);

pub const ACTION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    e1312fc3c4075301cc8dfca2d8800d7a21b7a68cab7cfb893b2852d77fb42c67
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
