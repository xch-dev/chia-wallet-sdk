use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE: [u8; 724] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff820277ff82016f80ff
    8201ef80ff4f80ffff20ffff15ff0bff4f808080ffff01ff04ffff04ff27ffff
    04ffff11ff57ff4f80ff778080ffff04ffff02ff1effff04ff02ffff04ff05ff
    ff04ffff02ff16ffff04ff02ffff04ff81afffff04ff82016fffff04ff8201ef
    ff808080808080ff8080808080ffff04ffff02ff1affff04ff02ffff04ff05ff
    ff04ffff02ff16ffff04ff02ffff04ff81afffff04ff820277ffff04ff8201ef
    ff808080808080ffff04ff81afff808080808080ffff04ffff04ff18ffff04ff
    ff0effff0170ffff0bffff0102ffff0bffff0101ff81af80ffff0bffff0101ff
    4f808080ff808080ffff04ffff04ffff0181d6ffff04ff10ffff04ff81afffff
    04ff4fffff04ffff04ff81afff8080ff808080808080ff808080808080ffff01
    ff088080ff0180ffff04ffff01ffffff333eff4202ffffffffa04bf5122f3445
    54c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184
    f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a128
    71fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102
    a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5
    ff04ff10ffff04ffff0bff52ffff0bff1cffff0bff1cff62ff0580ffff0bff1c
    ffff0bff72ffff0bff1cffff0bff1cff62ffff0bffff0101ff0b8080ffff0bff
    1cff62ff42808080ff42808080ffff04ff80ffff04ffff04ff17ff8080ff8080
    808080ffff0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff
    0101ff0b80ffff0bffff0101ff17808080ff04ff14ffff04ffff0112ffff04ff
    80ffff04ffff0bff52ffff0bff1cffff0bff1cff62ff0580ffff0bff1cffff0b
    ff72ffff0bff1cffff0bff1cff62ffff0bffff0101ff0b8080ffff0bff1cff62
    ff42808080ff42808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    ae41bf077dfbfdb93069d841dac67f8856a5637e45cefc9e1ecd00e0025266a9
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorInitiatePayoutActionSolution {
    pub entry_payout_amount: u64,
    pub entry_payout_puzzle_hash: Bytes32,
    pub entry_initial_cumulative_payout: u64,
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
