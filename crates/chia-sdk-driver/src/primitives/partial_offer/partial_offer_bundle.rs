use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::LineageProof;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{Condition, conditions::CreateCoin, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{
    Cat, CatInfo, DriverError, PartialOffer, PartialOfferFill, PartialOfferInfo, Puzzle,
    SpendContext,
};

/// The puzzle hash of the placeholder coin spend in an encoded partial offer, whose puzzle reveal
/// is the [`PartialOfferHint`](crate::PartialOfferHint).
pub const PARTIAL_OFFER_HINT_PUZZLE_HASH: Bytes32 = Bytes32::new([1; 32]);

/// A partial offer as it's shared, which is the offer coin along with the maker's spends that
/// create it. If the coin is already on-chain, there are no spends.
///
/// This is encoded as a spend bundle with an additional placeholder coin spend that describes the
/// offer coin, as specified by CHIP-0052.
#[derive(Debug, Clone)]
pub struct PartialOfferBundle {
    partial_offer: PartialOffer,
    spend_bundle: SpendBundle,
}

impl PartialOfferBundle {
    /// Creates a partial offer from the maker's own spend bundle, which must create the offer coin.
    ///
    /// The bundle is checked to make sure that the partial offer is safe to share:
    /// * The offer coin must be created by a spend that commits to its outputs, rather than by a
    ///   settlement payments coin (whose solution anyone could change to take the funds).
    /// * Exactly one offer coin must be created, with the memos from
    ///   [`PartialOfferInfo::creation_memos`].
    /// * The parent of the offer coin must not create any other partial offer coin with the same
    ///   maker and requested asset (identified by its creation memos), since they would share the
    ///   payment nonce and a single payment could fill all of them.
    pub fn from_maker_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: SpendBundle,
        info: PartialOfferInfo,
    ) -> Result<Self, DriverError> {
        info.validate()?;

        let puzzle_hash = info.puzzle_hash();
        let mut found = None;

        for coin_spend in &spend_bundle.coin_spends {
            let (puzzle, solution) = deserialize(allocator, coin_spend)?;

            for (index, create_coin) in create_coins(allocator, puzzle, solution)?
                .into_iter()
                .enumerate()
            {
                if create_coin.puzzle_hash != puzzle_hash {
                    continue;
                }

                if found.is_some() {
                    return Err(DriverError::PartialOfferSiblings);
                }

                found = Some((coin_spend, index, create_coin));
            }
        }

        let Some((parent_spend, index, create_coin)) = found else {
            return Err(DriverError::PartialOfferNotCreated);
        };

        if PartialOfferInfo::parse_creation_memos(allocator, create_coin.memos)
            != Some((info.maker_puzzle_hash, info.requested_asset))
        {
            return Err(DriverError::InvalidMemo);
        }

        let (puzzle, solution) = deserialize(allocator, parent_spend)?;

        for (other_index, other) in create_coins(allocator, puzzle, solution)?
            .into_iter()
            .enumerate()
        {
            if other_index != index
                && PartialOfferInfo::parse_creation_memos(allocator, other.memos)
                    == Some((info.maker_puzzle_hash, info.requested_asset))
            {
                return Err(DriverError::PartialOfferSiblings);
            }
        }

        let parent_p2_puzzle_hash =
            match CatInfo::parse(allocator, Puzzle::parse(allocator, puzzle))? {
                Some((cat_info, _)) => cat_info.p2_puzzle_hash,
                None => parent_spend.coin.puzzle_hash,
            };

        if parent_p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() {
            return Err(DriverError::UnsignedPartialOfferCreation);
        }

        let coin = Coin::new(parent_spend.coin.coin_id(), puzzle_hash, create_coin.amount);
        let lineage_proof = created_lineage_proof(allocator, parent_spend, coin)?;

        Ok(Self {
            partial_offer: PartialOffer::new(coin, lineage_proof, info),
            spend_bundle,
        })
    }

    /// Creates a partial offer for an offer coin which already exists on-chain.
    pub fn from_coin(partial_offer: PartialOffer) -> Self {
        Self {
            partial_offer,
            spend_bundle: SpendBundle::new(Vec::new(), Signature::default()),
        }
    }

    /// Parses a decoded partial offer. The terms are validated, but the offer coin may already
    /// have been filled or clawed back if it isn't created by the maker's spends, so it should be
    /// looked up on-chain (along with its latest descendant) before filling it.
    pub fn from_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: &SpendBundle,
    ) -> Result<Self, DriverError> {
        let mut hint_spend = None;
        let mut maker_spend_bundle =
            SpendBundle::new(Vec::new(), spend_bundle.aggregated_signature.clone());

        for coin_spend in &spend_bundle.coin_spends {
            if coin_spend.coin.puzzle_hash != PARTIAL_OFFER_HINT_PUZZLE_HASH {
                maker_spend_bundle.coin_spends.push(coin_spend.clone());
            } else if hint_spend.replace(coin_spend).is_some() {
                return Err(DriverError::ConflictingOfferInputs);
            }
        }

        let hint_spend = hint_spend.ok_or(DriverError::MissingHint)?;
        let hint = hint_spend.puzzle_reveal.to_clvm(allocator)?;
        let (info, hint_lineage_proof) = PartialOfferInfo::parse_hint(allocator, hint)?;

        let coin = Coin::new(
            hint_spend.coin.parent_coin_info,
            info.puzzle_hash(),
            hint_spend.coin.amount,
        );

        let parent_spend = maker_spend_bundle
            .coin_spends
            .iter()
            .find(|coin_spend| coin_spend.coin.coin_id() == coin.parent_coin_info);

        // If the maker's spends create the offer coin, the lineage proof can be derived from them
        let lineage_proof = if let Some(parent_spend) = parent_spend {
            let (puzzle, solution) = deserialize(allocator, parent_spend)?;

            if !create_coins(allocator, puzzle, solution)?
                .iter()
                .any(|create_coin| {
                    create_coin.puzzle_hash == coin.puzzle_hash && create_coin.amount == coin.amount
                })
            {
                return Err(DriverError::PartialOfferNotCreated);
            }

            created_lineage_proof(allocator, parent_spend, coin)?
        } else {
            hint_lineage_proof
        };

        Ok(Self {
            partial_offer: PartialOffer::new(coin, lineage_proof, info),
            spend_bundle: maker_spend_bundle,
        })
    }

    /// The spend bundle to encode, which includes the placeholder coin spend that describes the
    /// offer coin.
    pub fn to_spend_bundle(self, ctx: &mut SpendContext) -> Result<SpendBundle, DriverError> {
        let hint = self
            .partial_offer
            .info
            .alloc_hint(ctx, self.partial_offer.lineage_proof)?;

        let mut spend_bundle = self.spend_bundle;

        spend_bundle.coin_spends.push(CoinSpend::new(
            Coin::new(
                self.partial_offer.coin.parent_coin_info,
                PARTIAL_OFFER_HINT_PUZZLE_HASH,
                self.partial_offer.coin.amount,
            ),
            ctx.serialize(&hint)?,
            ctx.serialize(&NodePtr::NIL)?,
        ));

        Ok(spend_bundle)
    }

    pub fn partial_offer(&self) -> &PartialOffer {
        &self.partial_offer
    }

    /// The maker's spends that create the offer coin, without the placeholder coin spend.
    pub fn spend_bundle(&self) -> &SpendBundle {
        &self.spend_bundle
    }

    /// Fills the offer coin by paying `requested_amount` of the requested asset to the maker,
    /// including the maker's spends that create it.
    pub fn fill(
        &self,
        ctx: &mut SpendContext,
        requested_amount: u64,
    ) -> Result<PartialOfferFill, DriverError> {
        self.partial_offer
            .fill_with_inputs(ctx, requested_amount, self.spend_bundle.clone())
    }
}

fn deserialize(
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
) -> Result<(NodePtr, NodePtr), DriverError> {
    Ok((
        coin_spend.puzzle_reveal.to_clvm(allocator)?,
        coin_spend.solution.to_clvm(allocator)?,
    ))
}

fn create_coins(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<Vec<CreateCoin<NodePtr>>, DriverError> {
    let output = run_puzzle(allocator, puzzle, solution)?;

    Ok(Vec::<Condition>::from_clvm(allocator, output)?
        .into_iter()
        .filter_map(Condition::into_create_coin)
        .collect())
}

/// The lineage proof of a CAT coin created by the parent spend, or [`None`] for XCH.
fn created_lineage_proof(
    allocator: &mut Allocator,
    parent_spend: &CoinSpend,
    coin: Coin,
) -> Result<Option<LineageProof>, DriverError> {
    let (puzzle, solution) = deserialize(allocator, parent_spend)?;
    let puzzle = Puzzle::parse(allocator, puzzle);

    let Some(children) = Cat::parse_children(allocator, parent_spend.coin, puzzle, solution)?
    else {
        return Ok(None);
    };

    children
        .into_iter()
        .find(|child| child.coin == coin)
        .map(|child| child.lineage_proof)
        .ok_or(DriverError::PartialOfferNotCreated)
}
