use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized reward distributor stake action, compiled from Rue.
///
/// Locks assets with the distributor's lock puzzle and adds their shares to an entry. If
/// the entry already exists, its slot is spent, its unclaimed rewards are forfeited back
/// to the remaining rewards, and the entry's custody puzzle hash must send a
/// puzzle-to-puzzle message of `"s"` followed by the tree hash of the forfeited amount
/// (scaled by the precision). The entry slot is re-created with the current cumulative
/// payout and the new total shares. The action increments the ephemeral state (used as a
/// nonce by the lock puzzles), announces `"t"` followed by the new entry slot value's
/// hash, and must run before `last_update + max_second_offset`.
pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE: [u8; 790] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff02ffff01ff04ffff04ffff10ff82013fffff010180
    ffff04ff8202bfffff04ffff10ff8205bfff1180ffff04ffff04ff8213bfffff
    10ff821bbfff0d8080ff820fbf80808080ffff04ffff04ffff0133ffff04ffff
    02ff2bffff04ff3bffff04ff17ffff04ffff0bffff0101ff0280ff8080808080
    ffff04ff80ffff04ffff04ff8205ffff8080ff8080808080ffff04ffff04ffff
    013effff04ffff0effff0174ff0280ff808080ffff04ffff04ffff0155ffff04
    ffff10ff8217bfff2f80ff808080ffff02ffff03ffff15ff8202ffffff0181ff
    80ffff01ff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0173ffff
    0bffff0101ff0d8080ffff04ff8205ffff8080808080ffff04ffff04ffff0142
    ffff04ffff0112ffff04ff80ffff04ffff02ff2bffff04ff3bffff04ff17ffff
    04ffff0bffff0101ffff02ff13ffff04ff13ff8201ff808080ff8080808080ff
    8080808080ff198080ffff01ff02ffff03ffff22ffff20ff820fff80ffff20ff
    820bff8080ffff0119ffff01ff088080ff018080ff018080808080ffff04ffff
    02ff09ffff04ff09ffff04ffff10ff82017fffff010180ffff04ff8202ffffff
    04ff8209dfffff10ff8207ffff08808080808080ff018080ffff04ffff04ffff
    02ff17ffff04ff4fffff04ff82017fff5f808080ffff12ff8203ffffff11ff82
    04efff8202ff808080ff018080ffff04ffff04ffff01ff02ffff03ffff07ff03
    80ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff04
    ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff04ffff01ff0bff
    ff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff0102ffff0bffff
    0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0b
    ffff010180808080ffff01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff
    0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff0580
    ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180808080
    ffff01ff0bffff018201018080ff01808080ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_STAKE_PUZZLE`].
pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    7ab8fcfb1a028a6f2ba8973b6fc80eca8e189654eb5ff3d1fc5fccd0757dd38d
    "
));

/// Curried arguments of the stake action ([`REWARD_DISTRIBUTOR_STAKE_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorStakeActionArgs<LP> {
    /// Hash of the slot puzzle after its first curry for entry slots
    /// ([`RewardDistributorSlotNonce::ENTRY`](crate::puzzles::RewardDistributorSlotNonce::ENTRY)).
    pub entry_slot_1st_curry_hash: Bytes32,
    /// Maximum number of seconds since the distributor's last update. The action asserts
    /// that the spend happens before `last_update + max_second_offset`, so entries can't be
    /// changed with stale reward accounting.
    pub max_second_offset: u64,
    /// Puzzle that verifies the assets being locked, for example a curried
    /// [`RewardDistributorCatLockingPuzzleArgs`](crate::puzzles::RewardDistributorCatLockingPuzzleArgs).
    /// It is run as `(ephemeral_state custody_puzzle_hash . solution)` and returns
    /// `(shares . conditions)`.
    pub lock_puzzle: LP,
}

/// Solution to the stake action.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorStakeActionSolution<LPS> {
    /// Solution passed to the lock puzzle, after the ephemeral state and custody puzzle hash.
    pub lock_puzzle_solution: LPS,
    /// Counter of the entry's existing slot, or -1 if the entry is new.
    pub existing_slot_counter: i128,
    /// Custody puzzle hash of the entry, which receives rewards and unstaked assets.
    pub entry_custody_puzzle_hash: Bytes32,
    /// Cumulative payout recorded in the entry's existing slot, or 0 if the entry is new.
    pub existing_slot_cumulative_payout: u128,
    /// Shares of the entry's existing slot, or 0 if the entry is new.
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

/// Serialized nonce wrapper puzzle: `(a INNER_PUZZLE inner_solution)`, with an unused
/// curried nonce. Wrapping a puzzle with different nonces gives different puzzle hashes
/// that behave the same way.
// run '(mod (NONCE INNER_PUZZLE . inner_solution) (a INNER_PUZZLE inner_solution))' -d
pub const NONCE_WRAPPER_PUZZLE: [u8; 7] = hex!("ff02ff05ff0780");
/// Tree hash of [`NONCE_WRAPPER_PUZZLE`].
pub const NONCE_WRAPPER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "847d971ef523417d555ea9854b1612837155d34d453298defcd310774305f657"
));

/// Curried arguments of the nonce wrapper ([`NONCE_WRAPPER_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct NonceWrapperArgs<N, I> {
    /// Value that only changes the puzzle hash.
    pub nonce: N,
    /// Puzzle that is run with the wrapper's solution.
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
