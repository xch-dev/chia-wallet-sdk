use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};

/// Metadata describing the terms of an option contract, stored in the launcher solution's key value list.
///
/// It can be read back with [`OptionContract::parse_metadata`](crate::OptionContract::parse_metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct OptionMetadata {
    /// The absolute timestamp (in seconds) at which the option expires.
    pub expiration_seconds: u64,
    /// The asset and amount that must be paid to exercise the option.
    pub strike_type: OptionType,
}

impl OptionMetadata {
    /// Creates a new [`OptionMetadata`].
    pub fn new(expiration_seconds: u64, strike_type: OptionType) -> Self {
        Self {
            expiration_seconds,
            strike_type,
        }
    }
}

/// The asset and amount of an option's strike payment, which the creator receives when it's exercised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
pub enum OptionType {
    /// A payment in XCH.
    Xch {
        /// The amount in mojos.
        amount: u64,
    },
    /// A payment in a CAT.
    Cat {
        /// The asset id of the CAT.
        asset_id: Bytes32,
        /// The amount in mojos.
        amount: u64,
    },
    /// A payment in a revocable CAT.
    RevocableCat {
        /// The asset id of the CAT.
        asset_id: Bytes32,
        /// The hidden puzzle hash of the CAT's revocation layer.
        hidden_puzzle_hash: Bytes32,
        /// The amount in mojos.
        amount: u64,
    },
    /// A payment of an NFT.
    Nft {
        /// The launcher id of the NFT.
        launcher_id: Bytes32,
        /// The full puzzle hash of the NFT when its p2 puzzle is the settlement payments puzzle.
        settlement_puzzle_hash: Bytes32,
        /// The amount of the NFT coin.
        amount: u64,
    },
}

impl OptionType {
    /// Returns the amount of the strike payment.
    pub fn amount(&self) -> u64 {
        match self {
            OptionType::Xch { amount }
            | OptionType::Cat { amount, .. }
            | OptionType::RevocableCat { amount, .. }
            | OptionType::Nft { amount, .. } => *amount,
        }
    }

    /// Returns whether the strike payment is hinted with the creator's puzzle hash, which is the
    /// case for CATs and NFTs but not XCH.
    pub fn is_hinted(&self) -> bool {
        matches!(
            self,
            OptionType::Cat { .. } | OptionType::RevocableCat { .. } | OptionType::Nft { .. }
        )
    }
}
