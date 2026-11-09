use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized reward distributor lock puzzle for CAT staking, compiled from Rue.
///
/// Verifies that a CAT coin with a positive amount is paid to the distributor's p2 puzzle
/// hash through a settlement payment, whose nonce is the tree hash of
/// `(ephemeral_state . my_id)` and whose memos hint `(custody_puzzle_hash . my_p2_puzzle_hash)`.
/// It creates a deposit slot for the CAT, announces `"l"` followed by the settlement
/// announcement id, and returns the CAT amount as the number of shares.
pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE: [u8; 576] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff15ff8202ffff8080ffff01ff02ffff01ff04ff82
    05ffffff04ffff04ffff0133ffff04ffff0bffff0102ffff0bffff0182010280
    ffff0bffff0102ffff0bffff0102ffff0bffff0182010180ff5f80ffff0bffff
    0102ffff02ff0dffff04ff0dffff04ffff0bffff0101ffff02ff09ffff04ff09
    ffff04ff82017fffff04ff8205ffff8205ff8080808080ff80808080ffff0bff
    ff010180808080ffff04ff80ffff04ffff04ff82017fff8080ff8080808080ff
    ff04ffff04ffff013fffff04ff02ff808080ffff04ffff04ffff013effff04ff
    ff0effff016cff0280ff808080ffff04ffff04ffff0146ffff04ff8202ffff80
    8080ff808080808080ffff04ffff0bffff02ff05ffff04ff0bff8203ff8080ff
    ff02ff04ffff04ff04ffff04ffff02ff04ffff04ff04ffff04ff5fff82017f80
    8080ffff04ffff04ff17ffff04ff8202ffffff04ffff04ffff02ff04ffff04ff
    04ffff04ff81bfff17808080ff8080ff80808080ff808080808080ff018080ff
    ff01ff088080ff0180ffff04ffff04ffff01ff02ffff03ffff07ff0380ffff01
    ff0bffff0102ffff02ff02ffff04ff02ff058080ffff02ff02ffff04ff02ff07
    808080ffff01ff0bffff0101ff038080ff0180ffff01ff02ffff03ff03ffff01
    ff0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff
    0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080
    ffff0bffff010180808080ffff01ff0bffff018201018080ff018080ff018080
    "
);

/// Tree hash of [`REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE`].
pub const REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b429e5ee56a5b155135c9cce3c21a8bd72a7a90dc68167230c9ce7f2bb12e4eb
    "
));

/// Curried arguments of the CAT lock puzzle ([`REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE`]).
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorCatLockingPuzzleArgs<CM> {
    /// CAT maker for the staked asset (for example a curried
    /// [`DefaultCatMakerArgs`](crate::puzzles::DefaultCatMakerArgs) puzzle).
    pub cat_maker: CM,
    /// Mod hash of the settlement payments puzzle.
    pub offer_mod_hash: Bytes32,
    /// Puzzle hash that holds staked assets: [`P2DelegatedBySingletonLayerArgs`](crate::puzzles::P2DelegatedBySingletonLayerArgs)
    /// for the reward distributor singleton, with nonce 1.
    pub my_p2_puzzle_hash: Bytes32,
    /// Hash of the slot puzzle after its first curry for deposit slots
    /// ([`RewardDistributorSlotNonce::DEPOSIT`](crate::puzzles::RewardDistributorSlotNonce::DEPOSIT)).
    pub deposit_slot_1st_curry_hash: Bytes32,
}

impl<CM> RewardDistributorCatLockingPuzzleArgs<CM> {
    /// Creates the arguments with the standard settlement payments mod hash.
    pub fn new(
        cat_maker: CM,
        my_p2_puzzle_hash: Bytes32,
        deposit_slot_1st_curry_hash: Bytes32,
    ) -> Self {
        Self {
            cat_maker,
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            my_p2_puzzle_hash,
            deposit_slot_1st_curry_hash,
        }
    }
}

/// Solution to the CAT lock puzzle, after the ephemeral state and custody puzzle hash.
#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorCatLockingPuzzleSolution<CMS> {
    /// Coin id of the reward distributor singleton coin being spent.
    pub my_id: Bytes32,
    /// Amount of the CAT coin being staked. Must be positive.
    pub cat_amount: u64,
    /// Rest of the CAT maker's solution, after the inner puzzle hash.
    #[clvm(rest)]
    pub cat_maker_solution_rest: CMS,
}

impl<CM> Mod for RewardDistributorCatLockingPuzzleArgs<CM> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_CAT_LOCKING_PUZZLE_HASH
    }
}
