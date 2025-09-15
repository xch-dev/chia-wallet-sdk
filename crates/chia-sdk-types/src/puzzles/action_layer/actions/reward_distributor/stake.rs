use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE: [u8; 932] = hex!(
    "
    ff02ffff01ff02ff36ffff04ff02ffff04ff05ffff04ff0bffff04ff2fffff04
    ff81dfffff04ffff02ff17ffff04ff4fffff04ff82015fff819f808080ff8080
    808080808080ffff04ffff01ffffff55ff3343ff42ff02ff02ffff03ff05ffff
    01ff0bff72ffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff3cffff04ff
    02ffff04ff0dff80808080ff808080808080ffff016280ff0180ffffffffa04b
    f5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa0
    9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2
    ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923
    f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a6
    3400ade7c5ffff04ff28ffff04ffff02ff3affff04ff02ffff04ff05ffff04ff
    ff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff17ff8080ff
    8080808080ff0bff52ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff3c
    ffff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff2cffff0bff
    2cff62ff0580ffff0bff2cff0bff428080ff04ffff04ffff10ff27ffff010180
    ffff04ff57ffff04ffff10ff81b7ff819f80ffff04ffff04ff820277ffff10ff
    820377ffff12ff81efffff11ff820277ff81af80808080ffff04ff8202f7ff80
    8080808080ffff04ffff02ff2affff04ff02ffff04ff05ffff04ffff02ff2eff
    ff04ff02ffff04ffff04ff4fffff04ff820277ffff10ff81efff819f808080ff
    80808080ffff04ff4fff808080808080ffff04ffff04ff10ffff04ffff10ff82
    04f7ff0b80ff808080ffff04ffff02ffff03ffff15ff81afffff0181ff80ffff
    01ff04ffff04ff38ffff04ffff0112ffff04ffff04ffff0173ffff04ffff12ff
    81efffff11ff820277ff81af8080ff808080ffff04ff4fff8080808080ffff04
    ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff
    2fff80808080ff8080808080ff81df8080ffff01ff02ffff03ffff20ff81ef80
    ffff0181dfffff01ff088080ff018080ff018080808080ffff02ffff03ffff07
    ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080
    ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff05
    8080ff0180ff04ff14ffff04ffff0112ffff04ff80ffff04ffff02ff3affff04
    ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080
    ff018080
    "
);

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    60ce952659fc0d1bffa99e9576e8cf353a9419f1d04cd4ac8bf168a9210c35a7
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorStakeActionArgs<LP> {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
    pub lock_puzzle: LP,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorStakeActionSolution<LPS> {
    pub lock_puzzle_solution: LPS,
    pub entry_custody_puzzle_hash: Bytes32,
    pub existing_slot_cumulative_payout: i128,
    #[clvm(rest)]
    pub existing_slot_shares: u64,
}

impl<LP> Mod for RewardDistributorStakeActionArgs<LP> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_STAKE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH
    }
}

// run '(mod (NONCE INNER_PUZZLE . inner_solution) (a INNER_PUZZLE inner_solution))' -d
pub const NONCE_WRAPPER_PUZZLE: [u8; 7] = hex!("ff02ff05ff0780");
pub const NONCE_WRAPPER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "847d971ef523417d555ea9854b1612837155d34d453298defcd310774305f657"
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct NonceWrapperArgs<N, I> {
    pub nonce: N,
    pub inner_puzzle: I,
}

impl<N, I> Mod for NonceWrapperArgs<N, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NONCE_WRAPPER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        NONCE_WRAPPER_PUZZLE_HASH
    }
}
