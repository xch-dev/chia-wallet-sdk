use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized reward distributor unlock puzzle for staked CATs, compiled from Rue.
///
/// Spends the CAT's deposit slot and sends a message to the locked CAT coin approving a
/// delegated puzzle that pays the CAT to the custody puzzle hash. The custody puzzle
/// hash must send a puzzle-to-puzzle message of `"u"` followed by the CAT coin's parent
/// id. Returns the deposit's shares.
pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE: [u8; 555] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff22ffff20ff2f80ffff15ff8202ffff808080ffff
    01ff04ff8202ffffff04ffff04ffff0142ffff04ffff0117ffff04ffff02ff04
    ffff04ff04ffff04ffff0101ffff04ffff04ffff0133ffff04ff5fffff04ff82
    017fffff04ffff04ff5fff8080ff8080808080ffff01ffff52ff808080808080
    80ffff04ffff30ff81bfffff02ff05ffff04ff17ff8203ff8080ff82017f80ff
    8080808080ffff04ffff04ffff0143ffff04ffff0112ffff04ffff0effff0175
    ff81bf80ffff04ff5fff8080808080ffff04ffff04ffff0142ffff04ffff0112
    ffff04ff80ffff04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102
    ffff0bffff0102ffff0bffff0182010180ff0b80ffff0bffff0102ffff02ff06
    ffff04ff06ffff04ffff0bffff0101ffff02ff04ffff04ff04ffff04ff5fffff
    04ff8202ffff82017f8080808080ff80808080ffff0bffff010180808080ff80
    80808080ff8080808080ffff01ff088080ff0180ffff04ffff04ffff01ff02ff
    ff03ffff07ff0380ffff01ff0bffff0102ffff02ff02ffff04ff02ff058080ff
    ff02ff02ffff04ff02ff07808080ffff01ff0bffff0101ff038080ff0180ffff
    01ff02ffff03ff03ffff01ff0bffff0102ffff0bffff0182010480ffff0bffff
    0102ffff0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02
    ff02ffff04ff02ff078080ffff0bffff010180808080ffff01ff0bffff018201
    018080ff018080ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE`].
pub const REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    7146fbf50b1a688eb6263f76d93faee874030d26c8562e3538dab9e806498e5b
    "
));

/// Curried arguments of the CAT unlock puzzle ([`REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCatUnlockingPuzzleArgs<CM> {
    /// CAT maker for the staked asset, used to compute the locked CAT's puzzle hash.
    pub cat_maker: CM,
    /// Hash of the slot puzzle after its first curry for deposit slots
    /// ([`RewardDistributorSlotNonce::DEPOSIT`](crate::puzzles::RewardDistributorSlotNonce::DEPOSIT)).
    pub deposit_slot_1st_curry_hash: Bytes32,
    /// Puzzle hash that holds staked assets: [`P2DelegatedBySingletonLayerArgs`](crate::puzzles::P2DelegatedBySingletonLayerArgs)
    /// for the reward distributor singleton, with nonce 1.
    pub my_p2_puzzle_hash: Bytes32,
}

impl<CM> RewardDistributorCatUnlockingPuzzleArgs<CM> {
    /// Creates the arguments from their parts.
    pub fn new(
        cat_maker: CM,
        my_p2_puzzle_hash: Bytes32,
        deposit_slot_1st_curry_hash: Bytes32,
    ) -> Self {
        Self {
            cat_maker,
            deposit_slot_1st_curry_hash,
            my_p2_puzzle_hash,
        }
    }
}
/// Solution to the CAT unlock puzzle, after the ephemeral state and custody puzzle hash.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCatUnlockingPuzzleSolution<CMS> {
    /// Parent coin id of the locked CAT coin.
    pub cat_parent_id: Bytes32,
    /// Amount of the locked CAT coin.
    pub cat_amount: u64,
    /// Shares recorded in the CAT's deposit slot. Must be positive.
    pub cat_shares: u64,
    /// Rest of the CAT maker's solution, after the inner puzzle hash.
    #[clvm(rest)]
    pub cat_maker_solution_rest: CMS,
}

impl<CM> Mod for RewardDistributorCatUnlockingPuzzleArgs<CM> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_CAT_UNLOCKING_PUZZLE_HASH
    }
}
