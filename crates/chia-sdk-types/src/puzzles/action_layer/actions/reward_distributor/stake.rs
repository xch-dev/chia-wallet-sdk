use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE: [u8; 988] = hex!(
    "
    ff02ffff01ff02ff36ffff04ff02ffff04ff05ffff04ff0bffff04ff2fffff04
    ff7fffff04ffff02ff17ffff04ff4fffff04ff81bfff5f808080ff8080808080
    808080ffff04ffff01ffffff55ff333effff4342ff02ff02ffff03ff05ffff01
    ff0bff81e2ffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff3cffff04ff
    02ffff04ff0dff80808080ff808080808080ffff0181c280ff0180ffffffffff
    a04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c778545
    9aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718b
    a7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f680
    6923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce1
    19a63400ade7c5ff04ffff02ff2affff04ff02ffff04ff05ffff04ff0bffff04
    ff17ff808080808080ffff04ffff04ff38ffff04ffff0effff0174ff0b80ff80
    8080ff2f8080ffff04ff28ffff04ffff02ff3affff04ff02ffff04ff05ffff04
    ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff17ff8080
    ff8080808080ff0bff81a2ffff02ff26ffff04ff02ffff04ff05ffff04ffff02
    ff3cffff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff2cffff
    0bff2cff81c2ff0580ffff0bff2cff0bff81828080ff04ffff04ffff10ff27ff
    ff010180ffff04ff57ffff04ffff10ff81b7ff819f80ffff04ffff04ff820277
    ffff10ff820377ffff12ff81efffff11ff820277ff81af80808080ff8201f780
    808080ffff02ff32ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ff
    ff04ffff04ff4fffff04ff820277ffff10ff81efff819f808080ff80808080ff
    ff04ff4fffff04ffff04ffff04ff10ffff04ffff10ff8202f7ff0b80ff808080
    ffff02ffff03ffff15ff81afffff0181ff80ffff01ff04ffff04ff24ffff04ff
    ff0112ffff04ffff0effff0173ffff0bffff0101ffff12ff81efffff11ff8202
    77ff81af80808080ffff04ff4fff8080808080ffff04ffff02ff3effff04ff02
    ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff2fff80808080ff808080
    8080ff81df8080ffff01ff02ffff03ff81efffff01ff0880ffff0181df80ff01
    8080ff018080ff8080808080808080ffff02ffff03ffff07ff0580ffff01ff0b
    ffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04
    ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff
    34ffff04ffff0112ffff04ff80ffff04ffff02ff3affff04ff02ffff04ff05ff
    ff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b0886ee342f7e63c1ac0c68616901d5b6baa25bb98bfb294e8f62c593bef85f4
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
