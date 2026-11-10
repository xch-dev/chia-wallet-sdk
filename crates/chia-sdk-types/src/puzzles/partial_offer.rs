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

/// The tree hash of [`PARTIAL_OFFER_PUZZLE`].
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
    /// The amount of the offered asset released for every `precision` of the requested asset.
    pub price_precision: NonZeroU64,
    /// The amount of the requested asset that `price_precision` is quoted against.
    pub precision: NonZeroU64,
}

impl PartialOfferPrice {
    /// Creates a new [`PartialOfferPrice`] from its numerator (`price_precision`) and
    /// denominator (`precision`).
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

/// Curried arguments of [`PARTIAL_OFFER_PUZZLE`] from CHIP-0052, which sells an amount of one
/// asset (the offered asset) for another (the requested asset) in any number of partial fills.
///
/// Each fill asserts that the requested asset was paid to `receiver_puzzle_hash` through the
/// settlement payments puzzle, with the offer coin's parent coin id as the nonce. It then releases
/// part of the offered asset according to the `price`, and recreates the offer coin with the
/// rest. The offer coin is usually the partial offer puzzle and the maker's puzzle in a 1-of-n
/// ([`P2OneOfManyArgs`](crate::puzzles::P2OneOfManyArgs)) tree, so that the maker can claw it
/// back at any time.
///
/// `CM` is the CAT maker of the offered asset (such as [`DefaultCatMakerArgs`] or
/// [`XchCatMaker`]) and `I` is the inner puzzle, usually a quoted list of conditions. Either can be
/// a [`TreeHash`] to compute the puzzle hash without allocating the puzzles.
///
/// [`DefaultCatMakerArgs`]: crate::puzzles::DefaultCatMakerArgs
/// [`XchCatMaker`]: crate::puzzles::XchCatMaker
///
/// ## Example
///
/// ```rust
/// # use std::num::NonZeroU64;
/// # use chia_protocol::Bytes32;
/// # use chia_puzzle_types::cat::CatArgs;
/// # use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
/// # use chia_sdk_types::{
/// #     Mod,
/// #     conditions::ReserveFee,
/// #     puzzles::{PartialOfferArgs, PartialOfferPrice, XchCatMaker},
/// # };
/// # use clvm_traits::clvm_quote;
/// # use clvm_utils::ToTreeHash;
/// // Sell XCH for a CAT, releasing 3 mojos of XCH for every 2 mojos of the CAT.
/// let asset_id = Bytes32::new([1; 32]);
/// let maker_puzzle_hash = Bytes32::new([2; 32]);
///
/// let args = PartialOfferArgs {
///     cat_maker: XchCatMaker::mod_hash(),
///     other_asset_offer_mod: CatArgs::curry_tree_hash(asset_id, SETTLEMENT_PAYMENT_HASH.into())
///         .into(),
///     receiver_puzzle_hash: maker_puzzle_hash,
///     // Every fill outputs these conditions, so each one must pay a fee of at least 10 mojos.
///     inner_puzzle: clvm_quote!([ReserveFee::new(10)]).tree_hash(),
///     // Every fill must be for at least 100 mojos of the CAT.
///     min_other_asset_amount_minus_one: 99,
///     price: PartialOfferPrice::new(NonZeroU64::new(3).unwrap(), NonZeroU64::new(2).unwrap()),
/// };
///
/// let partial_puzzle_hash = args.curry_tree_hash();
/// # let _ = partial_puzzle_hash;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PartialOfferArgs<CM, I> {
    /// Computes the full puzzle hash of the offered asset from an inner puzzle hash.
    pub cat_maker: CM,
    /// The full puzzle hash of the settlement payments puzzle for the requested asset.
    pub other_asset_offer_mod: Bytes32,
    /// The puzzle hash that receives the requested asset, which is usually the maker's puzzle hash.
    /// It's also the payment's only memo.
    pub receiver_puzzle_hash: Bytes32,
    /// Outputs the persistent conditions of every fill, such as an expiration or required fee.
    pub inner_puzzle: I,
    /// One less than the minimum amount of the requested asset that each fill must pay, so `0`
    /// allows any fill of at least 1 mojo.
    pub min_other_asset_amount_minus_one: u64,
    /// How much of the offered asset each fill releases.
    pub price: PartialOfferPrice,
}

/// The solution of the [`PartialOfferArgs`] puzzle, which fills part (or all) of the offer.
///
/// The offer coin is recreated with its remaining amount, unless that is zero. `C` is the rest of
/// the taker's `CREATE_COIN` condition, `M` is the CAT maker's solution, and `S` is the inner
/// puzzle's solution.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PartialOfferSolution<C, M, S> {
    /// The parent coin id, inner puzzle hash, and amount of the coin being spent.
    pub my_proof: CoinProof,
    /// The amount of the requested asset that the taker pays in this fill, which must be greater
    /// than `min_other_asset_amount_minus_one`.
    pub other_asset_amount: u64,
    /// The rest of a `CREATE_COIN` condition (puzzle hash, amount, and memos) for the taker's
    /// payout, which isn't signed and must therefore be asserted by the taker.
    pub create_coin_rest: Option<C>,
    /// The solution passed to the CAT maker after the inner puzzle hash, which is empty for the
    /// CAT makers in this crate.
    pub cat_maker_solution: M,
    /// The solution passed to the inner puzzle.
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
