use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    Mod,
    puzzles::{RewardDistributorEntryPayoutInfo, RewardDistributorEntrySlotValue},
};

/// Serialized reward distributor remove entry action, used by managed distributors.
///
/// Removes an entry: it spends the entry slot, pays the entry's pending reward from the
/// reserve, and decreases the active shares. The payout's rounding error goes back to the
/// remaining rewards. The manager singleton must send a puzzle-to-puzzle message of `"r"`
/// followed by the tree hash of `(payout_puzzle_hash . shares)`.
pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE: [u8; 683] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff22ffff09ffff12ffff11ff8213bfff820b
    7f80ff820f7f80ffff10ffff12ff8204ffff5f80ff8206ff8080ffff15ff8206
    ffffff0181ff8080ffff15ff5fff8206ff8080ffff01ff04ffff04ff82013fff
    ff04ffff11ff8202bfff8204ff80ffff04ffff11ff8205bfff820f7f80ffff04
    ffff04ff8213bfffff10ff821bbfff8206ff8080ff820fbf80808080ffff04ff
    ff04ffff0143ffff04ffff0112ffff04ffff0effff0172ffff0bffff0102ffff
    0bffff0101ff82057f80ffff0bffff0101ff820f7f808080ffff04ffff02ff04
    ffff04ff06ffff04ff05ffff04ff0bffff04ff8203ffff808080808080ff8080
    808080ffff04ffff04ffff0155ffff04ffff10ff8217bfff2f80ff808080ffff
    04ffff04ffff0142ffff04ffff0112ffff04ff80ffff04ffff02ff04ffff04ff
    06ffff04ff17ffff04ffff0bffff0101ffff0bffff0102ffff0bffff0101ff82
    027f80ffff0bffff0102ffff0bffff0101ff82057f80ffff0bffff0102ffff0b
    ffff0101ff820b7f80ffff0bffff0101ff820f7f8080808080ff8080808080ff
    8080808080ffff04ffff04ffff0181d6ffff04ffff0133ffff04ff82057fffff
    04ff8204ffffff04ffff04ff82057fff8080ff808080808080ff808080808080
    ffff01ff088080ff0180ffff04ffff04ffff01ff0bffff0102ffff0bffff0182
    010280ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580ffff
    0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080ffff
    01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201
    018080ff018080ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE`].
pub const REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    6cdf7feefe369fa694e71ee9c40a383bfda5ef43eab52034195990d20086d2b9
    "
));

/// Curried arguments of the remove entry action ([`REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorRemoveEntryActionArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Tree hash of the manager singleton's [`SingletonStruct`](chia_puzzle_types::singleton::SingletonStruct).
    pub manager_singleton_struct_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for entry slots
    /// ([`RewardDistributorSlotNonce::ENTRY`](crate::puzzles::RewardDistributorSlotNonce::ENTRY)).
    pub entry_slot_1st_curry_hash: Bytes32,
    /// Maximum number of seconds since the distributor's last update. The action asserts
    /// that the spend happens before `last_update + max_seconds_offset`, so entries can't be
    /// changed with stale reward accounting.
    pub max_seconds_offset: u64,
    /// Fixed-point precision of the distributor's cumulative payout and remaining rewards.
    pub precision: u64,
}

/// Solution to the remove entry action.
#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorRemoveEntryActionSolution {
    /// Current value of the entry's slot.
    pub entry_slot: RewardDistributorEntrySlotValue,
    /// Payout owed to the entry, computed from its shares and the cumulative payout.
    pub entry_payout_info: RewardDistributorEntryPayoutInfo,
    /// Inner puzzle hash of the manager singleton's coin, used to verify the message's
    /// sender.
    #[clvm(rest)]
    pub manager_singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for RewardDistributorRemoveEntryActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_REMOVE_ENTRY_PUZZLE_HASH
    }
}
