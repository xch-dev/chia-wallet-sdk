use std::{
    num::NonZeroU64,
    sync::{Arc, Mutex},
};

use bindy::{Error, Result};
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzle_types::{LineageProof, Memos};
use chia_sdk_driver::{self as sdk, Cat, PartialOfferAsset, SpendContext};
use chia_sdk_types::puzzles::PartialOfferPrice as SdkPartialOfferPrice;
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{Action, AsProgram, Clvm, NotarizedPayment, Offer, Program, Puzzle, Spend};

pub fn encode_partial_offer(spend_bundle: SpendBundle) -> Result<String> {
    Ok(sdk::encode_partial_offer(&spend_bundle)?)
}

pub fn decode_partial_offer(partial_offer: String) -> Result<SpendBundle> {
    Ok(sdk::decode_partial_offer(&partial_offer)?)
}

pub trait PartialOfferAssetExt: Sized {
    fn xch() -> Result<Self>;
    fn cat(asset_id: Bytes32, hidden_puzzle_hash: Option<Bytes32>) -> Result<Self>;
    fn is_xch(&self) -> Result<bool>;
    fn asset_id(&self) -> Result<Option<Bytes32>>;
    fn hidden_puzzle_hash(&self) -> Result<Option<Bytes32>>;
    fn puzzle_hash(&self, p2_puzzle_hash: Bytes32) -> Result<Bytes32>;
}

impl PartialOfferAssetExt for PartialOfferAsset {
    fn xch() -> Result<Self> {
        Ok(Self::Xch)
    }

    fn cat(asset_id: Bytes32, hidden_puzzle_hash: Option<Bytes32>) -> Result<Self> {
        Ok(Self::Cat {
            asset_id,
            hidden_puzzle_hash,
        })
    }

    fn is_xch(&self) -> Result<bool> {
        Ok(matches!(self, Self::Xch))
    }

    fn asset_id(&self) -> Result<Option<Bytes32>> {
        match *self {
            Self::Xch => Ok(None),
            Self::Cat { asset_id, .. } => Ok(Some(asset_id)),
        }
    }

    fn hidden_puzzle_hash(&self) -> Result<Option<Bytes32>> {
        match *self {
            Self::Xch => Ok(None),
            Self::Cat {
                hidden_puzzle_hash, ..
            } => Ok(hidden_puzzle_hash),
        }
    }

    fn puzzle_hash(&self, p2_puzzle_hash: Bytes32) -> Result<Bytes32> {
        Ok(self.puzzle_hash(p2_puzzle_hash))
    }
}

/// Mirrors [`SdkPartialOfferPrice`], but the precisions are checked to be non-zero when it's used.
#[derive(Clone, Copy)]
pub struct PartialOfferPrice {
    pub price_precision: u64,
    pub precision: u64,
}

impl TryFrom<PartialOfferPrice> for SdkPartialOfferPrice {
    type Error = Error;

    fn try_from(value: PartialOfferPrice) -> Result<Self> {
        Ok(Self::new(
            non_zero(value.price_precision, "price_precision")?,
            non_zero(value.precision, "precision")?,
        ))
    }
}

impl From<SdkPartialOfferPrice> for PartialOfferPrice {
    fn from(value: SdkPartialOfferPrice) -> Self {
        Self {
            price_precision: value.price_precision.get(),
            precision: value.precision.get(),
        }
    }
}

/// Mirrors [`sdk::PartialOfferInfo`], but the price and minimum are checked to be non-zero when
/// it's used.
#[derive(Clone, Copy)]
pub struct PartialOfferInfo {
    pub offered_asset: PartialOfferAsset,
    pub requested_asset: PartialOfferAsset,
    pub maker_puzzle_hash: Bytes32,
    pub price: PartialOfferPrice,
    pub min_requested_amount: u64,
    pub expiration: Option<u64>,
    pub required_fee: Option<u64>,
}

impl PartialOfferInfo {
    pub fn new(
        offered_asset: PartialOfferAsset,
        requested_asset: PartialOfferAsset,
        maker_puzzle_hash: Bytes32,
        price: PartialOfferPrice,
    ) -> Result<Self> {
        Ok(Self {
            offered_asset,
            requested_asset,
            maker_puzzle_hash,
            price,
            min_requested_amount: 1,
            expiration: None,
            required_fee: None,
        })
    }

    /// Creates a partial offer with the price of `offered_amount` of the offered asset for
    /// `requested_amount` of the requested asset.
    pub fn from_amounts(
        offered_asset: PartialOfferAsset,
        requested_asset: PartialOfferAsset,
        maker_puzzle_hash: Bytes32,
        offered_amount: u64,
        requested_amount: u64,
    ) -> Result<Self> {
        Ok(sdk::PartialOfferInfo::from_amounts(
            offered_asset,
            requested_asset,
            maker_puzzle_hash,
            non_zero(offered_amount, "offered_amount")?,
            non_zero(requested_amount, "requested_amount")?,
        )
        .into())
    }

    /// The hash of the partial offer puzzle, which is one of the two leaves of the 1-of-n puzzle.
    pub fn partial_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(sdk::PartialOfferInfo::try_from(*self)?.partial_puzzle_hash())
    }

    /// The inner (p2) puzzle hash of the offer coin, which the maker sends the offered asset to.
    pub fn p2_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(sdk::PartialOfferInfo::try_from(*self)?.p2_puzzle_hash())
    }

    /// The full puzzle hash of the offer coin.
    pub fn puzzle_hash(&self) -> Result<Bytes32> {
        Ok(sdk::PartialOfferInfo::try_from(*self)?.puzzle_hash())
    }

    /// The amount of the offered asset that is released when `requested_amount` is paid, rounded
    /// down as in the puzzle.
    pub fn offered_amount(&self, requested_amount: u64) -> Result<Option<u64>> {
        Ok(sdk::PartialOfferInfo::try_from(*self)?.offered_amount(requested_amount))
    }

    /// The smallest amount of the requested asset that releases at least `offered_amount` of the
    /// offered asset.
    pub fn requested_amount(&self, offered_amount: u64) -> Result<Option<u64>> {
        Ok(sdk::PartialOfferInfo::try_from(*self)?.requested_amount(offered_amount))
    }

    /// The memos of the `CREATE_COIN` condition that creates the first offer coin.
    pub fn creation_memos(&self, clvm: Clvm) -> Result<Program> {
        let info = sdk::PartialOfferInfo::try_from(*self)?;
        let mut ctx = clvm.0.lock().unwrap();

        let memos = match info.creation_memos(&mut ctx)? {
            Memos::Some(memos) => memos,
            Memos::None => NodePtr::NIL,
        };

        Ok(Program(clvm.0.clone(), memos))
    }

    /// Parses the maker puzzle hash and requested asset from the memos of the `CREATE_COIN`
    /// condition that created the first offer coin.
    pub fn parse_creation_memos(memos: Program) -> Result<Option<PartialOfferCreationMemos>> {
        let ctx = memos.0.lock().unwrap();

        Ok(
            sdk::PartialOfferInfo::parse_creation_memos(&ctx, Memos::Some(memos.1)).map(
                |(maker_puzzle_hash, requested_asset)| PartialOfferCreationMemos {
                    maker_puzzle_hash,
                    requested_asset,
                },
            ),
        )
    }
}

impl TryFrom<PartialOfferInfo> for sdk::PartialOfferInfo {
    type Error = Error;

    fn try_from(value: PartialOfferInfo) -> Result<Self> {
        Ok(Self {
            offered_asset: value.offered_asset,
            requested_asset: value.requested_asset,
            maker_puzzle_hash: value.maker_puzzle_hash,
            price: value.price.try_into()?,
            min_requested_amount: non_zero(value.min_requested_amount, "min_requested_amount")?,
            expiration: value.expiration,
            required_fee: value.required_fee,
        })
    }
}

impl From<sdk::PartialOfferInfo> for PartialOfferInfo {
    fn from(value: sdk::PartialOfferInfo) -> Self {
        Self {
            offered_asset: value.offered_asset,
            requested_asset: value.requested_asset,
            maker_puzzle_hash: value.maker_puzzle_hash,
            price: value.price.into(),
            min_requested_amount: value.min_requested_amount.get(),
            expiration: value.expiration,
            required_fee: value.required_fee,
        }
    }
}

#[derive(Clone)]
pub struct PartialOfferCreationMemos {
    pub maker_puzzle_hash: Bytes32,
    pub requested_asset: PartialOfferAsset,
}

#[derive(Clone)]
pub struct PartialOffer {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub info: PartialOfferInfo,
}

impl PartialOffer {
    fn to_sdk(&self) -> Result<sdk::PartialOffer> {
        Ok(sdk::PartialOffer::new(
            self.coin,
            self.lineage_proof,
            self.info.try_into()?,
        ))
    }

    /// The offer coin as a CAT whose p2 puzzle is the 1-of-n puzzle, if the offered asset is a CAT.
    pub fn cat(&self) -> Result<Option<Cat>> {
        Ok(self.to_sdk()?.cat())
    }

    /// The amount of the offered asset released by paying `requested_amount`, or an error if the
    /// fill is below the minimum, would release nothing, or would release more than is left.
    pub fn offered_amount(&self, requested_amount: u64) -> Result<u64> {
        Ok(self.to_sdk()?.offered_amount(requested_amount)?)
    }

    /// The smallest amount of the requested asset which releases as much of the remaining offered
    /// asset as possible, or `None` if no valid fill is possible.
    pub fn max_requested_amount(&self) -> Result<Option<u64>> {
        Ok(self.to_sdk()?.max_requested_amount())
    }

    /// The partial offer coin that is created by filling this one, if anything remains.
    pub fn child(&self, requested_amount: u64) -> Result<Option<PartialOffer>> {
        Ok(self.to_sdk()?.child(requested_amount).map(Into::into))
    }

    /// The payment to the maker that a fill of `requested_amount` asserts.
    pub fn notarized_payment(&self, clvm: Clvm, requested_amount: u64) -> Result<NotarizedPayment> {
        let partial_offer = self.to_sdk()?;
        let mut ctx = clvm.0.lock().unwrap();

        Ok(partial_offer
            .notarized_payment(&mut ctx, requested_amount)?
            .as_program(&clvm.0))
    }

    /// The spend that releases the offered amount into a settlement payments coin. This doesn't pay
    /// the maker, so it's only valid alongside the payment. Use `fill` to build both.
    pub fn fill_spend(&self, clvm: Clvm, requested_amount: u64) -> Result<Spend> {
        let partial_offer = self.to_sdk()?;
        let mut ctx = clvm.0.lock().unwrap();

        let spend = partial_offer.fill_spend(&mut ctx, requested_amount)?;

        Ok(Spend {
            puzzle: Program(clvm.0.clone(), spend.puzzle),
            solution: Program(clvm.0.clone(), spend.solution),
        })
    }

    /// Fills the offer by paying `requested_amount` of the requested asset to the maker.
    pub fn fill(&self, clvm: Clvm, requested_amount: u64) -> Result<PartialOfferFill> {
        let partial_offer = self.to_sdk()?;

        let fill = {
            let mut ctx = clvm.0.lock().unwrap();
            partial_offer.fill(&mut ctx, requested_amount)?
        };

        Ok(PartialOfferFill::from_sdk(fill, &clvm.0))
    }

    /// The spend that claws back the offer coin with the maker's inner spend, which receives the
    /// full amount.
    pub fn claw_back_spend(&self, spend: Spend) -> Result<Spend> {
        let partial_offer = self.to_sdk()?;
        let ctx_clone = spend.puzzle.0.clone();
        let mut ctx = ctx_clone.lock().unwrap();

        let spend = partial_offer.claw_back_spend(&mut ctx, spend.into())?;

        Ok(Spend {
            puzzle: Program(ctx_clone.clone(), spend.puzzle),
            solution: Program(ctx_clone.clone(), spend.solution),
        })
    }
}

impl From<sdk::PartialOffer> for PartialOffer {
    fn from(value: sdk::PartialOffer) -> Self {
        Self {
            coin: value.coin,
            lineage_proof: value.lineage_proof,
            info: value.info.into(),
        }
    }
}

#[derive(Clone)]
pub struct ParsedPartialOffer {
    pub partial_offer: PartialOffer,
    pub requested_amount: u64,
}

#[derive(Clone)]
pub struct PartialOfferFill {
    pub offer: Offer,
    pub requested_amount: u64,
    pub offered_amount: u64,
    pub required_fee: u64,
    pub child: Option<PartialOffer>,
}

impl PartialOfferFill {
    fn from_sdk(fill: sdk::PartialOfferFill, clvm: &Arc<Mutex<SpendContext>>) -> Self {
        Self {
            offer: Offer {
                inner: fill.offer,
                clvm: clvm.clone(),
            },
            requested_amount: fill.requested_amount,
            offered_amount: fill.offered_amount,
            required_fee: fill.required_fee,
            child: fill.child.map(Into::into),
        }
    }

    /// The actions that pay the maker and the required fee, for the taker. Any additional fee
    /// should be added with `Action.fee`.
    pub fn actions(&self) -> Result<Vec<Action>> {
        let fill = sdk::PartialOfferFill {
            offer: self.offer.inner.clone(),
            requested_amount: self.requested_amount,
            offered_amount: self.offered_amount,
            required_fee: self.required_fee,
            child: None,
        };

        Ok(fill.actions().into_iter().map(Action::from).collect())
    }
}

#[derive(Clone)]
pub struct PartialOfferBundle {
    inner: sdk::PartialOfferBundle,
    clvm: Arc<Mutex<SpendContext>>,
}

impl PartialOfferBundle {
    /// Creates a partial offer from the maker's own spend bundle, which must create the offer coin
    /// with the memos from `PartialOfferInfo.creationMemos`.
    pub fn from_maker_spend_bundle(
        clvm: Clvm,
        spend_bundle: SpendBundle,
        info: PartialOfferInfo,
    ) -> Result<Self> {
        let info = info.try_into()?;
        let mut ctx = clvm.0.lock().unwrap();

        Ok(Self {
            inner: sdk::PartialOfferBundle::from_maker_spend_bundle(&mut ctx, spend_bundle, info)?,
            clvm: clvm.0.clone(),
        })
    }

    /// Creates a partial offer for an offer coin which already exists on-chain.
    pub fn from_coin(clvm: Clvm, partial_offer: PartialOffer) -> Result<Self> {
        Ok(Self {
            inner: sdk::PartialOfferBundle::from_coin(partial_offer.to_sdk()?),
            clvm: clvm.0.clone(),
        })
    }

    /// Parses a partial offer spend bundle, such as one decoded with `decodePartialOffer`. The
    /// offer coin may already have been filled or clawed back if it isn't created by the maker's
    /// spends, so it should be looked up on-chain before filling it.
    pub fn from_spend_bundle(clvm: Clvm, spend_bundle: SpendBundle) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();

        Ok(Self {
            inner: sdk::PartialOfferBundle::from_spend_bundle(&mut ctx, &spend_bundle)?,
            clvm: clvm.0.clone(),
        })
    }

    /// The spend bundle to encode with `encodePartialOffer`, which includes the placeholder coin
    /// spend that describes the offer coin.
    pub fn to_spend_bundle(&self) -> Result<SpendBundle> {
        let mut ctx = self.clvm.lock().unwrap();
        Ok(self.inner.clone().to_spend_bundle(&mut ctx)?)
    }

    pub fn partial_offer(&self) -> Result<PartialOffer> {
        Ok((*self.inner.partial_offer()).into())
    }

    /// The maker's spends that create the offer coin, without the placeholder coin spend.
    pub fn spend_bundle(&self) -> Result<SpendBundle> {
        Ok(self.inner.spend_bundle().clone())
    }

    /// Fills the offer coin by paying `requested_amount` of the requested asset to the maker,
    /// including the maker's spends that create it.
    pub fn fill(&self, requested_amount: u64) -> Result<PartialOfferFill> {
        let fill = {
            let mut ctx = self.clvm.lock().unwrap();
            self.inner.fill(&mut ctx, requested_amount)?
        };

        Ok(PartialOfferFill::from_sdk(fill, &self.clvm))
    }
}

impl Puzzle {
    /// Parses a spend that fills a partial offer coin, returning the offer coin that was spent and
    /// the amount of the requested asset that was paid. If the requested asset is a CAT, it must be
    /// provided (for example, from the creation memos of the first offer coin).
    pub fn parse_partial_offer(
        &self,
        coin: Coin,
        solution: Program,
        requested_asset: Option<PartialOfferAsset>,
    ) -> Result<Option<ParsedPartialOffer>> {
        let puzzle = sdk::Puzzle::from(self.clone());
        let ctx = self.program.0.lock().unwrap();

        Ok(
            sdk::PartialOffer::parse(&ctx, coin, puzzle, solution.1, requested_asset)?.map(
                |(partial_offer, requested_amount)| ParsedPartialOffer {
                    partial_offer: partial_offer.into(),
                    requested_amount,
                },
            ),
        )
    }

    /// Parses the partial offer coin that is recreated by a fill, if anything remains.
    pub fn parse_child_partial_offer(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
        requested_asset: Option<PartialOfferAsset>,
    ) -> Result<Option<PartialOffer>> {
        let parent_puzzle = sdk::Puzzle::from(self.clone());
        let ctx = self.program.0.lock().unwrap();

        Ok(sdk::PartialOffer::parse_child(
            &ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
            requested_asset,
        )?
        .map(Into::into))
    }
}

fn non_zero(value: u64, name: &str) -> Result<NonZeroU64> {
    NonZeroU64::new(value).ok_or_else(|| Error::Custom(format!("{name} must be non-zero")))
}
