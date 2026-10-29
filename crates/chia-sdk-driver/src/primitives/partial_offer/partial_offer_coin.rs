use std::num::NonZeroU64;

use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    CoinProof, LineageProof,
    cat::CatSolution,
    offer::{NotarizedPayment, Payment},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::puzzles::{
    P2OneOfManySolution, PARTIAL_OFFER_PUZZLE_HASH, PartialOfferArgs, PartialOfferSolution,
    RevocationSolution,
};
use clvm_traits::{FromClvm, clvm_list};
use clvmr::{Allocator, NodePtr};

use crate::{
    Action, AssetInfo, Cat, CatAssetInfo, CatInfo, CatLayer, CatMaker, DriverError, FeeAction,
    Layer, Offer, P2OneOfManyLayer, PartialOfferAsset, PartialOfferInfo, Puzzle, RequestedPayments,
    RevocationLayer, Spend, SpendContext,
};

/// A coin locked in a partial offer (CHIP-0052). Each fill spends it and recreates it with the
/// remaining amount, until it's fully filled or clawed back by the maker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialOffer {
    pub coin: Coin,
    /// The lineage proof of the coin, if the offered asset is a CAT.
    pub lineage_proof: Option<LineageProof>,
    pub info: PartialOfferInfo,
}

/// The result of filling a [`PartialOffer`].
#[derive(Debug, Clone)]
pub struct PartialOfferFill {
    /// An offer which contains the spend of the partial offer coin (and the maker's spends that
    /// create it, if it isn't on-chain yet). It offers the released amount of the offered asset in a
    /// settlement coin, and requests the payment to the maker. It's taken like any other offer,
    /// which asserts both the payment and the payout from the taker's signed spends.
    pub offer: Offer,
    pub requested_amount: u64,
    pub offered_amount: u64,
    /// The fee that the taker must leave in the transaction, which the partial offer coin reserves.
    pub required_fee: u64,
    /// The recreated partial offer coin, if anything remains.
    pub child: Option<PartialOffer>,
}

impl PartialOfferFill {
    /// The actions that pay the maker and the required fee. The required fee is already asserted by
    /// the partial offer coin, so it's not reserved again. Any additional fee should be added with
    /// [`Action::fee`].
    pub fn actions(&self) -> Vec<Action> {
        let mut actions = self.offer.requested_payments().actions();

        if self.required_fee > 0 {
            actions.push(Action::Fee(FeeAction {
                amount: self.required_fee,
                reserved: false,
            }));
        }

        actions
    }
}

impl PartialOffer {
    pub fn new(coin: Coin, lineage_proof: Option<LineageProof>, info: PartialOfferInfo) -> Self {
        Self {
            coin,
            lineage_proof,
            info,
        }
    }

    /// The offer coin as a [`Cat`] whose p2 puzzle is the 1-of-n puzzle, if the offered asset is a
    /// CAT.
    pub fn cat(&self) -> Option<Cat> {
        let PartialOfferAsset::Cat {
            asset_id,
            hidden_puzzle_hash,
        } = self.info.offered_asset
        else {
            return None;
        };

        Some(Cat::new(
            self.coin,
            self.lineage_proof,
            CatInfo::new(asset_id, hidden_puzzle_hash, self.info.p2_puzzle_hash()),
        ))
    }

    /// The amount of the offered asset released by paying `requested_amount`.
    ///
    /// Returns [`DriverError::InvalidPartialOfferFill`] if the amount is below the minimum, would
    /// release nothing, or would release more than the coin has left (in which case the taker would
    /// overpay, since the payout is capped at the coin's amount).
    pub fn offered_amount(&self, requested_amount: u64) -> Result<u64, DriverError> {
        if requested_amount < self.info.min_requested_amount.get() {
            return Err(DriverError::InvalidPartialOfferFill);
        }

        let offered_amount = self
            .info
            .offered_amount(requested_amount)
            .ok_or(DriverError::InvalidPartialOfferFill)?;

        if offered_amount == 0 || offered_amount > self.coin.amount {
            return Err(DriverError::InvalidPartialOfferFill);
        }

        Ok(offered_amount)
    }

    /// The smallest amount of the requested asset which releases as much of the remaining offered
    /// asset as possible without exceeding it. This is all of it, unless rounding leaves a remainder
    /// that is too small to be released by itself, which only the maker can claw back.
    ///
    /// Returns [`None`] if no valid fill is possible.
    pub fn max_requested_amount(&self) -> Option<u64> {
        let price = self.info.price;

        // The largest requested amount whose payout doesn't exceed the coin's amount
        let max_requested_amount =
            ((u128::from(self.coin.amount) + 1) * u128::from(price.precision.get()) - 1)
                / u128::from(price.price_precision.get());
        let max_requested_amount = u64::try_from(max_requested_amount).unwrap_or(u64::MAX);

        let requested_amount = self
            .info
            .requested_amount(self.info.offered_amount(max_requested_amount)?)?
            .max(self.info.min_requested_amount.get());

        self.offered_amount(requested_amount).ok()?;

        Some(requested_amount)
    }

    /// The partial offer coin that is created by filling this one, if anything remains.
    pub fn child(&self, requested_amount: u64) -> Option<Self> {
        let offered_amount = self.info.offered_amount(requested_amount)?;
        let remaining = self.coin.amount.checked_sub(offered_amount)?;

        if remaining == 0 {
            return None;
        }

        Some(Self::new(
            Coin::new(self.coin.coin_id(), self.coin.puzzle_hash, remaining),
            self.cat().map(|cat| cat.child_lineage_proof()),
            self.info,
        ))
    }

    /// The payment to the maker that the fill asserts. The nonce is the parent coin id of the offer
    /// coin, which is why no two partial offer coins with the same maker and requested asset may
    /// share a parent.
    pub fn notarized_payment(
        &self,
        ctx: &mut SpendContext,
        requested_amount: u64,
    ) -> Result<NotarizedPayment, DriverError> {
        Ok(NotarizedPayment::new(
            self.coin.parent_coin_info,
            vec![Payment::new(
                self.info.maker_puzzle_hash,
                requested_amount,
                ctx.hint(self.info.maker_puzzle_hash)?,
            )],
        ))
    }

    /// The spend that fills the offer by paying `requested_amount`, which releases the offered
    /// amount into a settlement payments coin. This doesn't pay the maker, so it's only valid
    /// alongside the payment. Use [`PartialOffer::fill`] to build both.
    pub fn fill_spend(
        &self,
        ctx: &mut SpendContext,
        requested_amount: u64,
    ) -> Result<Spend, DriverError> {
        let offered_amount = self.offered_amount(requested_amount)?;

        let partial_puzzle = self.info.construct_partial_puzzle(ctx)?;

        // The payout always goes to a settlement payments coin, so that the taker can assert it
        // from a signed spend. Otherwise, anyone could change where it goes.
        let partial_solution = ctx.alloc(&PartialOfferSolution {
            my_proof: CoinProof {
                parent_coin_info: self.coin.parent_coin_info,
                inner_puzzle_hash: self.info.p2_puzzle_hash(),
                amount: self.coin.amount,
            },
            other_asset_amount: requested_amount,
            create_coin_rest: Some(clvm_list!(
                Bytes32::from(SETTLEMENT_PAYMENT_HASH),
                offered_amount
            )),
            cat_maker_solution: (),
            inner_puzzle_solution: (),
        })?;

        self.spend_leaf(
            ctx,
            Spend::new(partial_puzzle, partial_solution),
            self.info.partial_puzzle_hash().into(),
        )
    }

    /// Fills the offer by paying `requested_amount` of the requested asset to the maker.
    pub fn fill(
        &self,
        ctx: &mut SpendContext,
        requested_amount: u64,
    ) -> Result<PartialOfferFill, DriverError> {
        self.fill_with_inputs(
            ctx,
            requested_amount,
            SpendBundle::new(Vec::new(), Signature::default()),
        )
    }

    /// Fills the offer, including the maker's spends that create the offer coin.
    pub(crate) fn fill_with_inputs(
        &self,
        ctx: &mut SpendContext,
        requested_amount: u64,
        mut spend_bundle: SpendBundle,
    ) -> Result<PartialOfferFill, DriverError> {
        let offered_amount = self.offered_amount(requested_amount)?;
        let spend = self.fill_spend(ctx, requested_amount)?;

        spend_bundle.coin_spends.push(CoinSpend::new(
            self.coin,
            ctx.serialize(&spend.puzzle)?,
            ctx.serialize(&spend.solution)?,
        ));

        let notarized_payment = self.notarized_payment(ctx, requested_amount)?;
        let mut requested_payments = RequestedPayments::new();
        let mut asset_info = AssetInfo::new();

        match self.info.requested_asset {
            PartialOfferAsset::Xch => requested_payments.xch.push(notarized_payment),
            PartialOfferAsset::Cat {
                asset_id,
                hidden_puzzle_hash,
            } => {
                requested_payments
                    .cats
                    .insert(asset_id, vec![notarized_payment]);
                asset_info.insert_cat(asset_id, CatAssetInfo::new(hidden_puzzle_hash))?;
            }
        }

        let offer =
            Offer::from_input_spend_bundle(ctx, spend_bundle, requested_payments, asset_info)?;

        Ok(PartialOfferFill {
            offer,
            requested_amount,
            offered_amount,
            required_fee: self.info.required_fee.unwrap_or(0),
            child: self.child(requested_amount),
        })
    }

    /// The spend that claws back the offer coin with the maker's inner spend, which receives the
    /// full amount. This can be done at any time, including after the offer expires.
    pub fn claw_back_spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<Spend, DriverError> {
        self.spend_leaf(ctx, inner_spend, self.info.maker_puzzle_hash)
    }

    /// Claws back the offer coin with the maker's inner spend and adds it to the context.
    pub fn claw_back(&self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<(), DriverError> {
        let spend = self.claw_back_spend(ctx, inner_spend)?;
        ctx.spend(self.coin, spend)
    }

    pub(crate) fn spend_leaf(
        &self,
        ctx: &mut SpendContext,
        leaf_spend: Spend,
        leaf_hash: Bytes32,
    ) -> Result<Spend, DriverError> {
        let merkle_tree = self.info.merkle_tree();

        let merkle_proof = merkle_tree
            .proof(leaf_hash)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let mut spend = P2OneOfManyLayer::new(merkle_tree.root()).construct_spend(
            ctx,
            P2OneOfManySolution::new(merkle_proof, leaf_spend.puzzle, leaf_spend.solution),
        )?;

        let Some(cat) = self.cat() else {
            return Ok(spend);
        };

        if let Some(hidden_puzzle_hash) = cat.info.hidden_puzzle_hash {
            spend = RevocationLayer::new(hidden_puzzle_hash, cat.info.p2_puzzle_hash)
                .construct_spend(
                    ctx,
                    RevocationSolution::new(false, spend.puzzle, spend.solution),
                )?;
        }

        // The offer coin is spent in a CAT ring by itself, since its outputs always add up to its
        // amount (the recreated coin and the payout).
        CatLayer::new(cat.info.asset_id, spend.puzzle).construct_spend(
            ctx,
            CatSolution {
                lineage_proof: cat.lineage_proof,
                inner_puzzle_solution: spend.solution,
                prev_coin_id: cat.coin.coin_id(),
                this_coin_info: cat.coin,
                next_coin_proof: CoinProof {
                    parent_coin_info: cat.coin.parent_coin_info,
                    inner_puzzle_hash: cat.info.inner_puzzle_hash().into(),
                    amount: cat.coin.amount,
                },
                prev_subtotal: 0,
                extra_delta: 0,
            },
        )
    }

    /// Parses a spend that fills a partial offer coin, returning the offer coin that was spent and
    /// the amount of the requested asset that was paid. Returns [`None`] if the spend isn't a fill,
    /// for example if it's a claw back.
    ///
    /// The puzzle only reveals the hash of the requested asset's settlement puzzle, so CATs can't
    /// be identified without knowing them in advance (for example, from the memos of the coin that
    /// created the first offer coin). If the requested asset is a CAT which isn't provided, this
    /// returns [`DriverError::MissingAssetInfo`].
    pub fn parse(
        allocator: &Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
        requested_asset: Option<PartialOfferAsset>,
    ) -> Result<Option<(Self, u64)>, DriverError> {
        let (offered_asset, lineage_proof, p2_puzzle, p2_solution) =
            if let Some(parsed) = Cat::parse(allocator, coin, puzzle, solution)? {
                if parsed.revoked {
                    return Ok(None);
                }

                (
                    PartialOfferAsset::Cat {
                        asset_id: parsed.cat.info.asset_id,
                        hidden_puzzle_hash: parsed.cat.info.hidden_puzzle_hash,
                    },
                    parsed.cat.lineage_proof,
                    parsed.p2_puzzle,
                    parsed.p2_solution,
                )
            } else {
                (PartialOfferAsset::Xch, None, puzzle, solution)
            };

        let Some(p2_layer) = P2OneOfManyLayer::parse_puzzle(allocator, p2_puzzle)? else {
            return Ok(None);
        };
        let p2_solution = P2OneOfManyLayer::parse_solution(allocator, p2_solution)?;

        let partial_puzzle = Puzzle::parse(allocator, p2_solution.puzzle);

        let Some(partial_puzzle) = partial_puzzle.as_curried() else {
            return Ok(None);
        };

        if partial_puzzle.mod_hash != PARTIAL_OFFER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = PartialOfferArgs::<NodePtr, NodePtr>::from_clvm(allocator, partial_puzzle.args)?;
        let partial_solution = PartialOfferSolution::<NodePtr, NodePtr, NodePtr>::from_clvm(
            allocator,
            p2_solution.solution,
        )?;

        let cat_maker =
            CatMaker::parse_puzzle(allocator, Puzzle::parse(allocator, args.cat_maker))?;

        if cat_maker != Some(offered_asset.cat_maker()) {
            return Ok(None);
        }

        let requested_asset = if args.other_asset_offer_mod == SETTLEMENT_PAYMENT_HASH.into() {
            PartialOfferAsset::Xch
        } else {
            requested_asset
                .filter(|asset| {
                    asset.puzzle_hash(SETTLEMENT_PAYMENT_HASH.into()) == args.other_asset_offer_mod
                })
                .ok_or(DriverError::MissingAssetInfo)?
        };

        let (expiration, required_fee) =
            PartialOfferInfo::parse_persistent_conditions(allocator, args.inner_puzzle)?;

        let info = PartialOfferInfo {
            offered_asset,
            requested_asset,
            maker_puzzle_hash: args.receiver_puzzle_hash,
            price: args.price,
            min_requested_amount: NonZeroU64::MIN
                .checked_add(args.min_other_asset_amount_minus_one)
                .ok_or(DriverError::InvalidPartialOffer)?,
            expiration,
            required_fee,
        };

        if info.merkle_tree().root() != p2_layer.merkle_root
            || info.puzzle_hash() != coin.puzzle_hash
        {
            return Ok(None);
        }

        Ok(Some((
            Self::new(coin, lineage_proof, info),
            partial_solution.other_asset_amount,
        )))
    }

    /// Parses the partial offer coin that is recreated by a fill, if anything remains.
    /// See [`PartialOffer::parse`] for details.
    pub fn parse_child(
        allocator: &Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        requested_asset: Option<PartialOfferAsset>,
    ) -> Result<Option<Self>, DriverError> {
        let Some((parent, requested_amount)) = Self::parse(
            allocator,
            parent_coin,
            parent_puzzle,
            parent_solution,
            requested_asset,
        )?
        else {
            return Ok(None);
        };

        Ok(parent.child(requested_amount))
    }
}
