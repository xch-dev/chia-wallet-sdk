use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const PRECOMMIT_LAYER_PUZZLE: [u8; 469] = hex!(
    "
    ff02ffff01ff04ffff04ff10ffff04ff17ff808080ffff04ffff04ff18ffff04
    ff8202ffff808080ffff04ffff04ff14ffff04ffff03ff82017fff2fff5f80ff
    ff04ff8202ffffff04ffff04ffff03ff82017fff2fff5f80ff8080ff80808080
    80ffff04ffff04ff1cffff04ffff0113ffff04ff82017fffff04ffff02ff2eff
    ff04ff02ffff04ff05ffff04ff0bffff04ff8205ffff808080808080ff808080
    8080ff8080808080ffff04ffff01ffffff5249ff3343ffff02ff02ffff03ff05
    ffff01ff0bff76ffff02ff3effff04ff02ffff04ff09ffff04ffff02ff1affff
    04ff02ffff04ff0dff80808080ff808080808080ffff016680ff0180ffffffa0
    4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a
    a09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7
    b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f68069
    23f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119
    a63400ade7c5ffff0bff56ffff02ff3effff04ff02ffff04ff05ffff04ffff02
    ff1affff04ff02ffff04ff07ff80808080ff808080808080ff0bff12ffff0bff
    12ff66ff0580ffff0bff12ff0bff468080ff018080
    "
);

pub const PRECOMMIT_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    10efe1dab105ef4780345baa2442196a26944040b12c0167375d79aaec89e33f
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
