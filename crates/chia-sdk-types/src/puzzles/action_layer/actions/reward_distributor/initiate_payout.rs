use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE: [u8; 783] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff82096fff8205df80ff
    8207df80ffff10ffff12ff819fff1780ff82015f8080ffff15ff82015fffff01
    81ff80ffff15ff17ff82015f80ffff20ffff15ff0bff819f808080ffff01ff04
    ffff04ff4fffff04ffff11ff81afff819f80ff82016f8080ffff04ffff02ff1e
    ffff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff8202dfffff
    04ff8205dfffff04ff8207dfff808080808080ff8080808080ffff04ffff02ff
    1affff04ff02ffff04ff05ffff04ffff02ff16ffff04ff02ffff04ff8202dfff
    ff04ffff11ff82096fff82015f80ffff04ff8207dfff808080808080ffff04ff
    8202dfff808080808080ffff04ffff04ff18ffff04ffff0effff0170ffff0bff
    ff0102ffff0bffff0101ff8202df80ffff0bffff0101ff819f808080ff808080
    ffff04ffff04ffff0181d6ffff04ff10ffff04ff8202dfffff04ff819fffff04
    ffff04ff8202dfff8080ff808080808080ff808080808080ffff01ff088080ff
    0180ffff04ffff01ffffff333eff4202ffffffffa04bf5122f344554c53bde2e
    bb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a
    73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb
    8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fb
    a471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff10ff
    ff04ffff0bff52ffff0bff1cffff0bff1cff62ff0580ffff0bff1cffff0bff72
    ffff0bff1cffff0bff1cff62ffff0bffff0101ff0b8080ffff0bff1cff62ff42
    808080ff42808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ffff
    0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b80
    ffff0bffff0101ff17808080ff04ff14ffff04ffff0112ffff04ff80ffff04ff
    ff0bff52ffff0bff1cffff0bff1cff62ff0580ffff0bff1cffff0bff72ffff0b
    ff1cffff0bff1cff62ffff0bffff0101ff0b8080ffff0bff1cff62ff42808080
    ff42808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    fa66364e84040e61475cce8f29692b7f84d319349e73d792a6b0fa4eaf2b8419
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
    pub precision: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorInitiatePayoutActionSolution {
    pub entry_payout_amount: u64,
    pub payout_rounding_error: u128,
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_initial_cumulative_payout: u128,
    #[clvm(rest)]
    pub entry_shares: u64,
}

impl Mod for RewardDistributorInitiatePayoutActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH
    }
}
