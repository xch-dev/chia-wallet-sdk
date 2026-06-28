use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const P2_PARENT_PUZZLE: [u8; 157] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ffff30ff0bffff02ff05ffff04ffff02ff
    06ffff04ff02ffff04ff2fff80808080ff7f8080ff1780ff808080ffff02ff2f
    ff5f8080ffff04ffff01ff47ff02ffff03ffff07ff0580ffff01ff0bffff0102
    ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff
    04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_PARENT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    638181aa0cd6ea9f042a69e578690513385e8531b361ffc26f03cc35f51018c2
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2ParentArgs<CM> {
    pub cat_maker: CM,
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct P2ParentSolution<PIP, PS, CMS> {
    pub parent_parent_id: Bytes32,
    pub parent_amount: u64,
    pub parent_inner_puzzle: PIP,
    pub parent_solution: PS,
    #[clvm(rest)]
    pub cat_maker_solution: CMS,
}

impl<M> Mod for P2ParentArgs<M> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_PARENT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_PARENT_PUZZLE_HASH
    }
}
