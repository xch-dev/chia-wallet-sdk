use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const PRECOMMIT_LAYER_PUZZLE: [u8; 343] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ffff0152ffff04ff17ff808080ffff04ffff04ffff01
    49ffff04ff8202ffff808080ffff04ffff04ffff0133ffff04ffff03ff82017f
    ff2fff5f80ffff04ff8202ffffff04ffff04ffff03ff82017fff2fff5f80ff80
    80ff8080808080ffff04ffff04ffff0143ffff04ffff0113ffff04ff82017fff
    ff04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    ff04ff0bffff04ff8203ffff8080808080ffff0bffff010180808080ff808080
    8080ff8080808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff
    0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180
    ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180
    808080ffff01ff0bffff018201018080ff0180ff018080
    "
);

pub const PRECOMMIT_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9b3be8822ce5583da2987336d59cf4bdded39d2747838249fd9afe0238cab6ea
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct PrecommitLayer1stCurryArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_struct_hash: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct PrecommitLayer2ndCurryArgs<V> {
    pub refund_puzzle_hash: Bytes32,
    pub value: V,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(atom)]
pub enum PrecommitSpendMode {
    REFUND = 0,
    REGISTER = 1,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PrecommitLayerSolution {
    pub mode: PrecommitSpendMode,
    pub my_amount: u64,
    #[clvm(rest)]
    pub singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for PrecommitLayer1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PRECOMMIT_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        PRECOMMIT_LAYER_PUZZLE_HASH
    }
}
