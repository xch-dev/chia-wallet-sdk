use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE: [u8; 997] = hex!(
    "
    ff02ffff01ff02ff36ffff04ff02ffff04ff05ffff04ff0bffff04ff2fffff04
    ff81dfffff04ffff02ff17ffff04ff4fffff04ff82015fff819f808080ff8080
    808080808080ffff04ffff01ffffff55ff333effff4342ff02ff02ffff03ff05
    ffff01ff0bff81e2ffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff3cff
    ff04ff02ffff04ff0dff80808080ff808080808080ffff0181c280ff0180ffff
    ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c
    7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f5
    96718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d2
    25f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298
    a91ce119a63400ade7c5ff04ffff02ff2affff04ff02ffff04ff05ffff04ff0b
    ffff04ff17ff808080808080ffff04ffff04ff38ffff04ffff0effff0174ff0b
    80ff808080ff2f8080ffff04ff28ffff04ffff02ff3affff04ff02ffff04ff05
    ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff17
    ff8080ff8080808080ff0bff81a2ffff02ff26ffff04ff02ffff04ff05ffff04
    ffff02ff3cffff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff
    2cffff0bff2cff81c2ff0580ffff0bff2cff0bff81828080ff04ffff04ffff10
    ff27ffff010180ffff04ff57ffff04ffff10ff81b7ff819f80ffff04ffff04ff
    820277ffff10ff820377ffff12ff81efffff11ff820277ff81af80808080ffff
    04ff8202f7ff808080808080ffff02ff32ffff04ff02ffff04ff05ffff04ffff
    02ff2effff04ff02ffff04ffff04ff4fffff04ff820277ffff10ff81efff819f
    808080ff80808080ffff04ff4fffff04ffff04ffff04ff10ffff04ffff10ff82
    04f7ff0b80ff808080ffff02ffff03ffff15ff81afffff0181ff80ffff01ff04
    ffff04ff24ffff04ffff0112ffff04ffff0effff0173ffff0bffff0101ffff12
    ff81efffff11ff820277ff81af80808080ffff04ff4fff8080808080ffff04ff
    ff02ff3effff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff2f
    ff80808080ff8080808080ff81df8080ffff01ff02ffff03ff81efffff01ff08
    80ffff0181df80ff018080ff018080ff8080808080808080ffff02ffff03ffff
    07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff808080
    80ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff
    058080ff0180ff04ff34ffff04ffff0112ffff04ff80ffff04ffff02ff3affff
    04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff80808080
    80ff018080
    "
);

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    54922214fe3163a5dbfa986bd857850b4addddd213b66e69f29debf2cea6706a
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
