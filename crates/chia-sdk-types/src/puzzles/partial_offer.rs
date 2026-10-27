use std::{borrow::Cow, num::NonZeroU64};

use chia_protocol::Bytes32;
use chia_puzzle_types::CoinProof;
use clvm_traits::{
    ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError, clvm_tuple,
};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// The partial offer puzzle from CHIP-0052, as deployed. This is the output of the Rue compiler at
/// the time the CHIP was finalized, and must not be recompiled, since newer compiler versions (and
/// the equivalent Chialisp) produce different bytes and therefore a different puzzle hash.
pub const PARTIAL_OFFER_PUZZLE: [u8; 429] = hex!(
    // Rue
    "
    ff02ffff01ff02ffff01ff04ffff04ffff013fffff04ffff0bff17ffff02ff05
    ffff04ff05ffff04ff8204ffffff04ffff04ff2fffff04ffff02ffff03ffff15
    ff8205ffff81bf80ffff018205ffffff01ff088080ff0180ffff04ffff04ff2f
    ff8080ff80808080ff808080808080ff808080ffff04ffff04ffff0146ffff04
    ffff30ff8204ffffff02ff0bffff04ff820affff8217ff8080ff8216ff80ff80
    8080ffff04ffff03ffff15ff04ff8080ffff04ffff0133ffff04ff820affffff
    04ff04ffff04ffff04ff820affff8080ff8080808080ffff04ffff0101ff8080
    80ffff03ffff02ffff03ff820bffffff01ff02ffff03ffff15ff822bffffff01
    81ff80ffff01ff0101ffff018080ff0180ffff018080ff0180ffff04ffff04ff
    ff0133ff820bff80ff0680ff0680808080ffff04ffff04ffff11ff820b7fffff
    13ffff12ff8202ffff82013f80ff8201bf8080ffff02ff2fff820fff8080ff01
    8080ffff04ffff01ff02ffff03ffff07ff0380ffff01ff0bffff0102ffff02ff
    02ffff04ff02ff058080ffff02ff02ffff04ff02ff07808080ffff01ff0bffff
    0101ff038080ff0180ff018080
    "
);

pub const PARTIAL_OFFER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "e8b38a9da89c523f3a1e4b2679d893e1f565d800ecd4e37b25ef644ae84fe868"
));

/// Each fill of `requested_amount` releases `requested_amount * price_precision / precision` of
/// the offered asset, rounded down.
///
/// This is represented in CLVM as the pair `(price_precision . precision)`. Neither can be zero,
/// since the offer would either release nothing or fail to divide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialOfferPrice {
    pub price_precision: NonZeroU64,
    pub precision: NonZeroU64,
}

impl PartialOfferPrice {
    pub fn new(price_precision: NonZeroU64, precision: NonZeroU64) -> Self {
        Self {
            price_precision,
            precision,
        }
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for PartialOfferPrice {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        clvm_tuple!(self.price_precision.get(), self.precision.get()).to_clvm(encoder)
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for PartialOfferPrice {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let (price_precision, precision) = <(u64, u64)>::from_clvm(decoder, node)?;

        let non_zero = |value| {
            NonZeroU64::new(value).ok_or_else(|| {
                FromClvmError::Custom("partial offer price precisions must be non-zero".to_string())
            })
        };

        Ok(Self::new(non_zero(price_precision)?, non_zero(precision)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PartialOfferArgs<CM, I> {
    /// Computes the full puzzle hash of the offered asset from an inner puzzle hash.
    pub cat_maker: CM,
    /// The full puzzle hash of the settlement payments puzzle for the requested asset.
    pub other_asset_offer_mod: Bytes32,
    pub receiver_puzzle_hash: Bytes32,
    /// Outputs the persistent conditions of every fill, such as an expiration or required fee.
    pub inner_puzzle: I,
    pub min_other_asset_amount_minus_one: u64,
    pub price: PartialOfferPrice,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PartialOfferSolution<C, M, S> {
    /// The parent coin id, inner puzzle hash, and amount of the coin being spent.
    pub my_proof: CoinProof,
    pub other_asset_amount: u64,
    /// The rest of a `CREATE_COIN` condition (puzzle hash, amount, and memos) for the taker's
    /// payout, which isn't signed and must therefore be asserted by the taker.
    pub create_coin_rest: Option<C>,
    pub cat_maker_solution: M,
    #[clvm(rest)]
    pub inner_puzzle_solution: S,
}

impl<CM, I> Mod for PartialOfferArgs<CM, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PARTIAL_OFFER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        PARTIAL_OFFER_PUZZLE_HASH
    }
}

#[cfg(test)]
mod tests {
    use clvmr::Allocator;

    use crate::assert_puzzle_hash;

    use super::*;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(PARTIAL_OFFER_PUZZLE => PARTIAL_OFFER_PUZZLE_HASH);
        Ok(())
    }

    #[test]
    fn test_price_clvm() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let price = PartialOfferPrice::new(NonZeroU64::new(5).unwrap(), NonZeroU64::MAX);
        let ptr = price.to_clvm(&mut allocator)?;
        assert_eq!(PartialOfferPrice::from_clvm(&allocator, ptr)?, price);
        assert_eq!(<(u64, u64)>::from_clvm(&allocator, ptr)?, (5, u64::MAX));

        for (price_precision, precision) in [(0, 1), (1, 0), (0, 0)] {
            let ptr = clvm_tuple!(price_precision, precision).to_clvm(&mut allocator)?;
            assert!(PartialOfferPrice::from_clvm(&allocator, ptr).is_err());
        }

        Ok(())
    }
}
