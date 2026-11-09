use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::NONCE_WRAPPER_PUZZLE_HASH};

/// Serialized `p2_next_reward_distributor_epoch` puzzle, compiled from Rue.
///
/// Holds incentives that can only be committed to a reward distributor's next epoch.
/// When spent, it:
///
/// - requires `next_epoch_start` to be on the distributor's epoch grid (a whole number of
///   epochs after the first epoch start);
/// - requires the spend to happen during the epoch before `next_epoch_start`;
/// - asserts the distributor's commit incentives announcement for a commitment slot of
///   `(next_epoch_start, clawback_ph, my_amount)`, where `clawback_ph` is the
///   [`NonceWrapperArgs`](crate::puzzles::NonceWrapperArgs) puzzle hash with this coin's
///   id as the nonce and the clawback inner puzzle hash;
/// - asserts its own coin id and amount.
///
/// It creates no coins, so its value has to be absorbed in the same spend bundle, for
/// example by the distributor's reserve.
pub const P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE: [u8; 479] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff03ffff3dffff11ff82017fff5f80ff81bf80ffff01ff08
    80ffff01ff04ffff04ffff0155ffff04ff82017fff808080ffff04ffff04ffff
    0151ffff04ffff11ff82017fff81bf80ff808080ffff04ffff04ffff013fffff
    04ffff0bffff02ff04ffff04ff06ffff04ff05ffff04ff2fffff04ff8207ffff
    808080808080ffff0163ffff0bffff0102ffff0bffff0101ff82017f80ffff0b
    ffff0102ffff0bffff0101ffff02ff04ffff04ff06ffff04ff0bffff04ffff0b
    ffff0101ff8202ff80ffff04ff17ff80808080808080ffff0bffff0101ff8205
    ff80808080ff808080ffff04ffff04ffff0149ffff04ff8205ffff808080ffff
    04ffff04ffff0146ffff04ff8202ffff808080ff80808080808080ff0180ffff
    04ffff04ffff01ff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff
    0bffff0102ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff
    04ff02ff078080ffff0bffff010180808080ffff01ff02ffff03ff03ffff01ff
    0bffff0102ffff0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0b
    ffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ff
    ff0bffff010180808080ffff01ff0bffff018201018080ff018080ff018080
    "
);

/// Tree hash of [`P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE`].
pub const P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    23522d15e71b71040b05a0ab6be2580ae2b62f72d70c7e83c1cd8aa515437a06
    "
));

/// Curried arguments of [`P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2NextRewardDistributorEpochArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Mod hash of the nonce wrapper ([`NONCE_WRAPPER_PUZZLE_HASH`]).
    pub nonce_mod_hash: Bytes32,
    /// Puzzle hash that will be able to withdraw the committed incentives, before the nonce
    /// wrapper is applied.
    pub clawback_inner_puzzle_hash: Bytes32,
    /// Tree hash of the reward distributor's [`SingletonStruct`](chia_puzzle_types::singleton::SingletonStruct).
    pub reward_distributor_singleton_struct_hash: Bytes32,
    /// Start timestamp of the distributor's first epoch.
    pub reward_distributor_first_epoch_start: u64,
    /// Length of each of the distributor's epochs, in seconds.
    pub reward_distributor_epoch_seconds: u64,
}

impl P2NextRewardDistributorEpochArgs {
    /// Creates the arguments with the standard singleton and nonce wrapper mod hashes.
    pub fn new(
        clawback_inner_puzzle_hash: Bytes32,
        reward_distributor_singleton_struct_hash: TreeHash,
        reward_distributor_first_epoch_start: u64,
        reward_distributor_epoch_seconds: u64,
    ) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            clawback_inner_puzzle_hash,
            reward_distributor_singleton_struct_hash: reward_distributor_singleton_struct_hash
                .into(),
            reward_distributor_first_epoch_start,
            reward_distributor_epoch_seconds,
        }
    }
}

/// Solution to [`P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2NextRewardDistributorEpochSolution {
    /// Start timestamp of the epoch the incentives are committed to.
    pub next_epoch_start: u64,
    /// Coin id of this coin, asserted and used as the clawback nonce.
    pub my_id: Bytes32,
    /// Amount of this coin, asserted and committed as the epoch's rewards.
    pub my_amount: u64,
    /// Current inner puzzle hash of the reward distributor singleton, used to compute the
    /// announcement's origin.
    #[clvm(rest)]
    pub reward_distributor_inner_puzzle_hash: Bytes32,
}

impl Mod for P2NextRewardDistributorEpochArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_NEXT_REWARD_DISTRIBUTOR_EPOCH_PUZZLE_HASH
    }
}
