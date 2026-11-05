use chia_protocol::Bytes32;
use clvm_traits::{ClvmDecoder, FromClvm, FromClvmError, ToClvm};

use super::{decode_canonical_int, expect_atom};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NewMetadataInfo<M> {
    pub new_metadata: M,
    pub new_updater_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NewMetadataOutput<M, C> {
    pub metadata_info: NewMetadataInfo<M>,
    pub conditions: C,
}

/// A trade price in a [`TransferNft`](crate::conditions::TransferNft) condition.
///
/// Parsing this is strict, so that the condition serializes back to exactly what it was parsed from.
/// The amount must be canonically encoded, and nothing can follow the puzzle hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm)]
#[clvm(list)]
pub struct TradePrice {
    pub amount: u64,
    pub puzzle_hash: Bytes32,
}

impl<D: ClvmDecoder> FromClvm<D> for TradePrice {
    fn from_clvm(decoder: &D, node: D::Node) -> Result<Self, FromClvmError> {
        let (amount, rest) = decoder.decode_pair(&node)?;
        let (puzzle_hash, rest) = decoder.decode_pair(&rest)?;
        expect_atom(decoder, &rest, &[])?;

        Ok(Self {
            amount: decode_canonical_int(decoder, amount)?,
            puzzle_hash: Bytes32::from_clvm(decoder, puzzle_hash)?,
        })
    }
}

impl TradePrice {
    pub fn new(amount: u64, puzzle_hash: Bytes32) -> Self {
        Self {
            amount,
            puzzle_hash,
        }
    }
}
