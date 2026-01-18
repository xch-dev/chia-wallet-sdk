use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE: [u8; 880] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff12ffff11ff8209dfff820bbf80ff
    820fbf80ffff10ffff12ff82013fff1780ff8202bf8080ffff15ff8202bfffff
    0181ff80ffff15ff17ff8202bf80ffff20ffff15ff0bff82013f808080ffff01
    ff04ffff04ff819fffff04ffff11ff82015fff82013f80ff8201df8080ffff04
    ffff02ff3effff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff
    8205bfffff04ff820bbfffff04ff820fbfff808080808080ff8080808080ffff
    04ffff02ff16ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04
    ff8205bfffff04ffff11ff8209dfff8202bf80ffff04ff820fbfff8080808080
    80ffff04ff8205bfff808080808080ffff04ffff04ff18ffff04ffff0effff01
    70ffff0bffff0102ffff0bffff0101ff8205bf80ffff0bffff0101ff82013f80
    8080ff808080ffff04ffff04ffff0181d6ffff04ff10ffff04ff8205bfffff04
    ff82013fffff04ffff04ff8205bfff8080ff808080808080ffff02ffff03ff2f
    ffff01ff04ffff04ff14ffff04ffff0112ffff04ffff0effff0170ffff0bffff
    0102ffff0bffff0101ff82013f80ffff0bffff0101ff8202bf808080ffff04ff
    8205bfff8080808080ff8080ff8080ff01808080808080ffff01ff088080ff01
    80ffff04ffff01ffffff333eff4342ffff02ffffa04bf5122f344554c53bde2e
    bb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a
    73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb
    8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fb
    a471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff10
    ffff04ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff
    7affff0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff
    4a808080ff4a808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ff
    ff0bffff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ff0b
    80ffff0bffff0101ff17808080ff04ff1cffff04ffff0112ffff04ff80ffff04
    ffff0bff5affff0bff12ffff0bff12ff6aff0580ffff0bff12ffff0bff7affff
    0bff12ffff0bff12ff6affff0bffff0101ff0b8080ffff0bff12ff6aff4a8080
    80ff4a808080ff8080808080ff018080
    "
);

pub const REWARD_DISTRIBUTOR_INITIATE_PAYOUT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    5bdb22c8dfea49632dbc999dc224dd956b05f57d16c57d545732451f961454ff
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorInitiatePayoutActionArgs {
    pub entry_slot_1st_curry_hash: Bytes32,
    pub payout_threshold: u64,
    pub precision: u64,
    pub require_approval: bool,
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
