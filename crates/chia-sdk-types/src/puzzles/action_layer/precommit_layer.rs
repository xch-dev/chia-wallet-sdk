use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// Serialized precommit layer puzzle, compiled from Rue.
///
/// A precommit coin proves that a value was committed to a registry singleton some time
/// ago, which keeps others from front-running a registration. It can be spent after
/// [`relative_block_height`](PrecommitLayer1stCurryArgs::relative_block_height) blocks,
/// when the registry singleton sends it a message, and pays its full amount either to the
/// payout puzzle hash (to register) or to the refund puzzle hash.
///
/// Curried with [`PrecommitLayer1stCurryArgs`], then [`PrecommitLayer2ndCurryArgs`].
/// The solution is [`PrecommitLayerSolution`].
pub const PRECOMMIT_LAYER_PUZZLE: [u8; 343] = hex!(
    // Rue
    "
    ff02ffff01ff04ffff04ffff0152ffff04ff17ff808080ffff04ffff04ffff01
    49ffff04ff8202ffff808080ffff04ffff04ffff0133ffff04ffff03ff82017f
    ff2fff5f80ffff04ff8202ffffff04ffff04ffff03ff82017fff2fff5f80ff80
    80ff8080808080ffff04ffff04ffff0143ffff04ffff0113ffff04ff82017fff
    ff04ffff0bffff0102ffff0bffff0182010280ffff0bffff0102ffff0bffff01
    02ffff0bffff0182010180ff0580ffff0bffff0102ffff02ff02ffff04ff02ff
    ff04ff0bffff04ff8203ffff8080808080ffff0bffff010180808080ff808080
    8080ff8080808080ffff04ffff01ff02ffff03ff03ffff01ff0bffff0102ffff
    0bffff0182010480ffff0bffff0102ffff0bffff0102ffff0bffff0182010180
    ff0580ffff0bffff0102ffff02ff02ffff04ff02ff078080ffff0bffff010180
    808080ffff01ff0bffff018201018080ff0180ff018080
    "
);

/// Tree hash of [`PRECOMMIT_LAYER_PUZZLE`].
pub const PRECOMMIT_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    9b3be8822ce5583da2987336d59cf4bdded39d2747838249fd9afe0238cab6ea
    "
));

/// First curry of the precommit layer ([`PRECOMMIT_LAYER_PUZZLE`]), shared by all
/// precommit coins of a registry.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct PrecommitLayer1stCurryArgs {
    /// Mod hash of the singleton top layer.
    pub singleton_mod_hash: Bytes32,
    /// Tree hash of the [`SingletonStruct`](chia_puzzle_types::singleton::SingletonStruct)
    /// of the registry singleton that controls the precommit coin.
    pub singleton_struct_hash: Bytes32,
    /// Number of blocks that must pass after the coin is created before it can be spent.
    pub relative_block_height: u32,
    /// Puzzle hash that receives the coin's amount in [`PrecommitSpendMode::REGISTER`] mode.
    pub payout_puzzle_hash: Bytes32,
}

/// Second curry of the precommit layer, specific to one commitment.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct PrecommitLayer2ndCurryArgs<V> {
    /// Puzzle hash that receives the coin's amount in [`PrecommitSpendMode::REFUND`] mode.
    pub refund_puzzle_hash: Bytes32,
    /// Value being committed to. The puzzle itself ignores it, but the registry's actions
    /// recompute the precommit puzzle hash from it.
    pub value: V,
}

/// How a precommit coin is spent. The registry singleton sends the mode as its message.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(atom)]
pub enum PrecommitSpendMode {
    /// Return the coin's amount to the refund puzzle hash.
    REFUND = 0,
    /// Use the commitment: send the coin's amount to the payout puzzle hash.
    REGISTER = 1,
}

/// Solution to the precommit layer puzzle.
#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct PrecommitLayerSolution {
    /// Whether to refund or register.
    pub mode: PrecommitSpendMode,
    /// Amount of the precommit coin, which is asserted and sent to the destination.
    pub my_amount: u64,
    /// Inner puzzle hash of the registry singleton coin that sends the message.
    #[clvm(rest)]
    pub singleton_inner_puzzle_hash: Bytes32,
}

impl Mod for PrecommitLayer1stCurryArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PRECOMMIT_LAYER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        PRECOMMIT_LAYER_PUZZLE_HASH
    }
}
