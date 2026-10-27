use chia_protocol::Bytes32;
use chia_puzzle_types::{LineageProof, Memos, cat::CatArgs};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    Condition, Conditions, MerkleTree, Mod,
    puzzles::{P2OneOfManyArgs, PartialOfferArgs, PartialOfferPrice, RevocationArgs},
};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{CatMaker, DriverError, SpendContext};

/// One side of a partial offer, which is either XCH, a CAT, or a revocable CAT.
///
/// This is also the CLVM representation used in the partial offer hint and the memos of the coin
/// that creates the first partial offer coin, as specified by CHIP-0052.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PartialOfferAsset {
    pub asset_id: Option<Bytes32>,
    pub hidden_puzzle_hash: Option<Bytes32>,
}

impl PartialOfferAsset {
    pub fn xch() -> Self {
        Self {
            asset_id: None,
            hidden_puzzle_hash: None,
        }
    }

    pub fn cat(asset_id: Bytes32, hidden_puzzle_hash: Option<Bytes32>) -> Self {
        Self {
            asset_id: Some(asset_id),
            hidden_puzzle_hash,
        }
    }

    pub fn is_xch(&self) -> bool {
        self.asset_id.is_none()
    }

    /// The CAT maker that the partial offer puzzle uses to compute the full puzzle hash of a coin
    /// of this asset from its inner puzzle hash.
    pub fn cat_maker(&self) -> CatMaker {
        match (self.asset_id, self.hidden_puzzle_hash) {
            (None, _) => CatMaker::Xch,
            (Some(asset_id), None) => CatMaker::Default {
                tail_hash_hash: asset_id.tree_hash(),
            },
            (Some(asset_id), Some(hidden_puzzle_hash)) => CatMaker::Revocable {
                tail_hash_hash: asset_id.tree_hash(),
                hidden_puzzle_hash_hash: hidden_puzzle_hash.tree_hash(),
            },
        }
    }

    /// The full puzzle hash of a coin of this asset with the given inner (p2) puzzle hash.
    pub fn puzzle_hash(&self, p2_puzzle_hash: Bytes32) -> Bytes32 {
        let Some(asset_id) = self.asset_id else {
            return p2_puzzle_hash;
        };

        let inner_puzzle_hash = match self.hidden_puzzle_hash {
            Some(hidden_puzzle_hash) => {
                RevocationArgs::new(hidden_puzzle_hash, p2_puzzle_hash).curry_tree_hash()
            }
            None => p2_puzzle_hash.into(),
        };

        CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash).into()
    }

    fn validate(&self) -> Result<(), DriverError> {
        if self.asset_id.is_none() && self.hidden_puzzle_hash.is_some() {
            return Err(DriverError::InvalidPartialOffer);
        }

        Ok(())
    }
}

/// The terms of a partial offer, as defined by CHIP-0052. The offered asset is locked in a coin
/// which anyone can spend to take part of it, as long as they pay the maker the requested asset at
/// the fixed price. The maker can claw back what remains at any time.
///
/// Each fill pays `requested_amount` of the requested asset to the maker, and releases
/// `requested_amount * price.price_precision / price.precision` (rounded down) of the offered
/// asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialOfferInfo {
    pub offered_asset: PartialOfferAsset,
    pub requested_asset: PartialOfferAsset,
    /// The puzzle hash that receives the requested asset, and that can claw back the offer.
    pub maker_puzzle_hash: Bytes32,
    pub price: PartialOfferPrice,
    /// The minimum amount of the requested asset that each fill must pay.
    pub min_requested_amount: u64,
    /// The timestamp (in seconds) at which the offer can no longer be filled. Once it expires, the
    /// remaining amount stays locked until the maker claws it back.
    pub expiration: Option<u64>,
    /// The minimum blockchain fee that each fill must pay.
    pub required_fee: Option<u64>,
}

impl PartialOfferInfo {
    pub fn new(
        offered_asset: PartialOfferAsset,
        requested_asset: PartialOfferAsset,
        maker_puzzle_hash: Bytes32,
        price: PartialOfferPrice,
    ) -> Self {
        Self {
            offered_asset,
            requested_asset,
            maker_puzzle_hash,
            price,
            min_requested_amount: 1,
            expiration: None,
            required_fee: None,
        }
    }

    /// Creates a partial offer with the price of `offered_amount` of the offered asset for
    /// `requested_amount` of the requested asset.
    pub fn from_amounts(
        offered_asset: PartialOfferAsset,
        requested_asset: PartialOfferAsset,
        maker_puzzle_hash: Bytes32,
        offered_amount: u64,
        requested_amount: u64,
    ) -> Self {
        let divisor = gcd(offered_amount, requested_amount).max(1);

        Self::new(
            offered_asset,
            requested_asset,
            maker_puzzle_hash,
            PartialOfferPrice::new(offered_amount / divisor, requested_amount / divisor),
        )
    }

    #[must_use]
    pub fn with_min_requested_amount(mut self, min_requested_amount: u64) -> Self {
        self.min_requested_amount = min_requested_amount;
        self
    }

    #[must_use]
    pub fn with_expiration(mut self, expiration: u64) -> Self {
        self.expiration = Some(expiration);
        self
    }

    #[must_use]
    pub fn with_required_fee(mut self, required_fee: u64) -> Self {
        self.required_fee = Some(required_fee);
        self
    }

    /// Checks that the terms are well formed. Neither precision can be zero (or the offer would
    /// either pay nothing or never be fillable), each fill must pay at least one mojo, and the two
    /// assets can't be the same (or a taker could pay the maker with the offered asset itself).
    pub fn validate(&self) -> Result<(), DriverError> {
        self.offered_asset.validate()?;
        self.requested_asset.validate()?;

        if self.price.price_precision == 0 || self.price.precision == 0 {
            return Err(DriverError::InvalidPartialOfferPrice);
        }

        if self.min_requested_amount == 0 || self.offered_asset == self.requested_asset {
            return Err(DriverError::InvalidPartialOffer);
        }

        Ok(())
    }

    /// The conditions that are output every time the offer is filled.
    pub fn persistent_conditions<T>(&self) -> Conditions<T> {
        let mut conditions = Conditions::default();

        if let Some(expiration) = self.expiration {
            conditions = conditions.assert_before_seconds_absolute(expiration);
        }

        if let Some(required_fee) = self.required_fee {
            conditions = conditions.reserve_fee(required_fee);
        }

        conditions
    }

    /// Parses the expiration and required fee from the quoted persistent conditions of a partial
    /// offer. Only the conditions that [`Self::persistent_conditions`] creates are supported, so
    /// that the terms can always be displayed in full.
    pub fn parse_persistent_conditions(
        allocator: &Allocator,
        quoted_conditions: NodePtr,
    ) -> Result<(Option<u64>, Option<u64>), DriverError> {
        let (_, conditions) =
            <(NodePtr, Vec<Condition<NodePtr>>)>::from_clvm(allocator, quoted_conditions)?;

        let expiration = conditions
            .iter()
            .find_map(Condition::as_assert_before_seconds_absolute)
            .map(|condition| condition.seconds);

        let required_fee = conditions
            .iter()
            .find_map(Condition::as_reserve_fee)
            .map(|condition| condition.amount);

        let info = Self {
            expiration,
            required_fee,
            ..Self::new(
                PartialOfferAsset::xch(),
                PartialOfferAsset::xch(),
                Bytes32::default(),
                PartialOfferPrice::new(0, 0),
            )
        };

        if tree_hash(allocator, quoted_conditions) != info.quoted_conditions_hash() {
            return Err(DriverError::UnsupportedPartialOfferConditions);
        }

        Ok((expiration, required_fee))
    }

    fn quoted_conditions_hash(&self) -> TreeHash {
        clvm_quote!(self.persistent_conditions::<TreeHash>()).tree_hash()
    }

    fn min_other_asset_amount_minus_one(&self) -> u64 {
        self.min_requested_amount.saturating_sub(1)
    }

    /// The hash of the partial offer puzzle, which is one of the two leaves of the 1-of-n puzzle.
    pub fn partial_puzzle_hash(&self) -> TreeHash {
        PartialOfferArgs {
            cat_maker: self.offered_asset.cat_maker().curry_tree_hash(),
            other_asset_offer_mod: self
                .requested_asset
                .puzzle_hash(SETTLEMENT_PAYMENT_HASH.into()),
            receiver_puzzle_hash: self.maker_puzzle_hash,
            inner_puzzle: self.quoted_conditions_hash(),
            min_other_asset_amount_minus_one: self.min_other_asset_amount_minus_one(),
            price: self.price,
        }
        .curry_tree_hash()
    }

    pub fn construct_partial_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let cat_maker = self.offered_asset.cat_maker().get_puzzle(ctx)?;

        ctx.curry(PartialOfferArgs {
            cat_maker,
            other_asset_offer_mod: self
                .requested_asset
                .puzzle_hash(SETTLEMENT_PAYMENT_HASH.into()),
            receiver_puzzle_hash: self.maker_puzzle_hash,
            inner_puzzle: clvm_quote!(self.persistent_conditions::<NodePtr>()),
            min_other_asset_amount_minus_one: self.min_other_asset_amount_minus_one(),
            price: self.price,
        })
    }

    /// The 1-of-n tree with the partial offer puzzle and the maker's puzzle as leaves.
    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[self.partial_puzzle_hash().into(), self.maker_puzzle_hash])
    }

    /// The puzzle hash of the 1-of-n puzzle, which is the inner (p2) puzzle hash of the offer coin.
    /// This is what the maker sends the offered asset to when creating the offer.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        P2OneOfManyArgs::new(self.merkle_tree().root())
            .curry_tree_hash()
            .into()
    }

    /// The full puzzle hash of the offer coin.
    pub fn puzzle_hash(&self) -> Bytes32 {
        self.offered_asset.puzzle_hash(self.p2_puzzle_hash())
    }

    /// The amount of the offered asset that is released when `requested_amount` is paid, rounded
    /// down as in the puzzle. Returns [`None`] if the amount doesn't fit in a coin.
    pub fn offered_amount(&self, requested_amount: u64) -> Option<u64> {
        let amount = u128::from(requested_amount) * u128::from(self.price.price_precision)
            / u128::from(self.price.precision);

        amount.try_into().ok()
    }

    /// The smallest amount of the requested asset that releases at least `offered_amount` of the
    /// offered asset. Returns [`None`] if the amount doesn't fit in a coin.
    pub fn requested_amount(&self, offered_amount: u64) -> Option<u64> {
        let amount = (u128::from(offered_amount) * u128::from(self.price.precision))
            .div_ceil(u128::from(self.price.price_precision));

        amount.try_into().ok()
    }

    /// The memos of the `CREATE_COIN` condition that creates the first offer coin, which hint it to
    /// the maker along with the requested asset, so that it can be discovered on-chain.
    pub fn creation_memos(&self, ctx: &mut SpendContext) -> Result<Memos, DriverError> {
        ctx.memos(&(self.maker_puzzle_hash, self.requested_asset))
    }

    /// Parses the maker puzzle hash and requested asset from the memos of the `CREATE_COIN`
    /// condition that created the first offer coin.
    pub fn parse_creation_memos(
        allocator: &Allocator,
        memos: Memos,
    ) -> Option<(Bytes32, PartialOfferAsset)> {
        let Memos::Some(memos) = memos else {
            return None;
        };

        <(Bytes32, PartialOfferAsset)>::from_clvm(allocator, memos).ok()
    }

    pub fn alloc_hint(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Option<LineageProof>,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&PartialOfferHint {
            lineage_proof,
            offered_asset: self.offered_asset,
            requested_asset: self.requested_asset,
            price: self.price,
            maker_puzzle_hash: self.maker_puzzle_hash,
            inner_puzzle: clvm_quote!(self.persistent_conditions::<NodePtr>()),
            min_other_asset_amount_minus_one: self.min_other_asset_amount_minus_one(),
        })
    }

    /// Parses and validates a partial offer hint, returning the terms and the lineage proof.
    pub fn parse_hint(
        allocator: &Allocator,
        hint: NodePtr,
    ) -> Result<(Self, Option<LineageProof>), DriverError> {
        let hint = PartialOfferHint::<NodePtr>::from_clvm(allocator, hint)?;
        let (expiration, required_fee) =
            Self::parse_persistent_conditions(allocator, hint.inner_puzzle)?;

        let info = Self {
            offered_asset: hint.offered_asset,
            requested_asset: hint.requested_asset,
            maker_puzzle_hash: hint.maker_puzzle_hash,
            price: hint.price,
            min_requested_amount: hint
                .min_other_asset_amount_minus_one
                .checked_add(1)
                .ok_or(DriverError::InvalidPartialOffer)?,
            expiration,
            required_fee,
        };

        info.validate()?;

        Ok((info, hint.lineage_proof))
    }
}

/// The puzzle reveal of the placeholder coin spend in an encoded partial offer, which describes
/// the offer coin. The placeholder coin has the parent coin id and amount of the offer coin.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PartialOfferHint<I> {
    pub lineage_proof: Option<LineageProof>,
    pub offered_asset: PartialOfferAsset,
    pub requested_asset: PartialOfferAsset,
    pub price: PartialOfferPrice,
    pub maker_puzzle_hash: Bytes32,
    /// The quoted persistent conditions.
    pub inner_puzzle: I,
    pub min_other_asset_amount_minus_one: u64,
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
