use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    CoinProof, LineageProof, Memos,
    cat::{CatSolution, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs},
};
use chia_sdk_types::{
    Condition, Conditions,
    conditions::{CreateCoin, RunCatTail},
    puzzles::{FeeLayerSolution, RevocationSolution},
    run_puzzle,
};
use clvm_traits::FromClvm;
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, FeeLayer, Layer, Puzzle, RevocationLayer, Spend, SpendContext};

mod cat_info;
mod cat_spend;
mod single_cat_spend;

pub use cat_info::*;
pub use cat_spend::*;
pub use single_cat_spend::*;

/// Contains all information needed to spend the outer puzzles of CAT coins.
/// The [`CatInfo`] is used to construct the puzzle, but the [`LineageProof`] is needed for the solution.
///
/// The only thing missing to create a valid coin spend is the inner puzzle and solution.
/// However, this is handled separately to provide as much flexibility as possible.
///
/// This type should contain all of the information you need to store in a database for later.
/// As long as you can figure out what puzzle the p2 puzzle hash corresponds to and spend it,
/// you have enough information to spend the CAT coin.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cat {
    /// The coin that this [`Cat`] represents. Its puzzle hash should match the [`CatInfo::puzzle_hash`].
    pub coin: Coin,

    /// The lineage proof is needed by the CAT puzzle to prove that this coin is a legitimate CAT.
    /// It's typically obtained by looking up and parsing the parent coin.
    ///
    /// This can get a bit tedious, so a helper method [`Cat::parse_children`] is provided to parse
    /// the child [`Cat`] objects from the parent (once you have looked up its information on-chain).
    ///
    /// Note that while the lineage proof is needed for most coins, it is optional if you are
    /// issuing more of the CAT by running its TAIL program.
    pub lineage_proof: Option<LineageProof>,

    /// The information needed to construct the outer puzzle of a CAT. See [`CatInfo`] for more details.
    pub info: CatInfo,
}

impl Cat {
    pub fn new(coin: Coin, lineage_proof: Option<LineageProof>, info: CatInfo) -> Self {
        Self {
            coin,
            lineage_proof,
            info,
        }
    }

    pub fn issue_with_coin(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(GenesisByCoinIdTailArgs::new(parent_coin_id))?;

        Self::issue(
            ctx,
            parent_coin_id,
            None,
            None,
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_with_key(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        public_key: PublicKey,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(public_key))?;

        Self::issue(
            ctx,
            parent_coin_id,
            None,
            None,
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_revocable_with_coin(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        hidden_puzzle_hash: Bytes32,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(GenesisByCoinIdTailArgs::new(parent_coin_id))?;

        Self::issue(
            ctx,
            parent_coin_id,
            Some(hidden_puzzle_hash),
            None,
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_revocable_with_key(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        public_key: PublicKey,
        hidden_puzzle_hash: Bytes32,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(public_key))?;

        Self::issue(
            ctx,
            parent_coin_id,
            Some(hidden_puzzle_hash),
            None,
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_fee_with_coin(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        fee_policy: FeePolicy,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(GenesisByCoinIdTailArgs::new(parent_coin_id))?;

        Self::issue(
            ctx,
            parent_coin_id,
            None,
            Some(fee_policy),
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_fee_with_key(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        public_key: PublicKey,
        fee_policy: FeePolicy,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(public_key))?;

        Self::issue(
            ctx,
            parent_coin_id,
            None,
            Some(fee_policy),
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_revocable_fee_with_coin(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        hidden_puzzle_hash: Bytes32,
        fee_policy: FeePolicy,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(GenesisByCoinIdTailArgs::new(parent_coin_id))?;

        Self::issue(
            ctx,
            parent_coin_id,
            Some(hidden_puzzle_hash),
            Some(fee_policy),
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue_revocable_fee_with_key(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        public_key: PublicKey,
        hidden_puzzle_hash: Bytes32,
        fee_policy: FeePolicy,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(public_key))?;

        Self::issue(
            ctx,
            parent_coin_id,
            Some(hidden_puzzle_hash),
            Some(fee_policy),
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
        fee_policy: Option<FeePolicy>,
        amount: u64,
        run_tail: RunCatTail<NodePtr, NodePtr>,
        conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let delegated_spend = ctx.delegated_spend(conditions.with(run_tail))?;
        let eve_info = CatInfo::new(
            ctx.tree_hash(run_tail.program).into(),
            hidden_puzzle_hash,
            ctx.tree_hash(delegated_spend.puzzle).into(),
        )
        .with_fee_policy(fee_policy);

        let eve = Cat::new(
            Coin::new(parent_coin_id, eve_info.puzzle_hash().into(), amount),
            None,
            eve_info,
        );

        let children = Cat::spend_all(ctx, &[CatSpend::new(eve.clone(), delegated_spend)])?;

        Ok((
            Conditions::new().create_coin(eve.coin.puzzle_hash, eve.coin.amount, Memos::None),
            children,
        ))
    }

    /// Constructs a [`CoinSpend`](chia_protocol::CoinSpend) for each [`CatSpend`] in the list.
    /// The spends are added to the [`SpendContext`] (in order) for convenience.
    ///
    /// All of the ring announcements and proofs required by the CAT puzzle are calculated automatically.
    /// This requires running the inner spends to get the conditions, so any errors will be propagated.
    ///
    /// It's important not to spend CATs with different asset IDs at the same time, since they are not
    /// compatible.
    ///
    /// Additionally, you should group all CAT spends done in the same transaction together
    /// so that the value of one coin can be freely used in the output of another. If you spend them
    /// separately, there will be multiple announcement rings and a non-zero delta will be calculated.
    pub fn spend_all(
        ctx: &mut SpendContext,
        cat_spends: &[CatSpend],
    ) -> Result<Vec<Cat>, DriverError> {
        let len = cat_spends.len();

        let mut total_delta = 0;
        let mut prev_subtotals = Vec::new();
        let mut run_tail_index = None;
        let mut children = Vec::new();

        for (index, item) in cat_spends.iter().enumerate() {
            let output = ctx.run(item.spend.puzzle, item.spend.solution)?;
            let conditions: Vec<Condition> = ctx.extract(output)?;

            // If this is the first TAIL reveal, we're going to keep track of it
            if run_tail_index.is_none() && conditions.iter().any(Condition::is_run_cat_tail) {
                run_tail_index = Some(index);
            }

            let create_coins: Vec<CreateCoin<NodePtr>> = conditions
                .into_iter()
                .filter_map(Condition::into_create_coin)
                .collect();

            // Calculate the delta of inputs and outputs
            let delta = create_coins
                .iter()
                .fold(i128::from(item.cat.coin.amount), |delta, create_coin| {
                    delta - i128::from(create_coin.amount)
                });

            // Add the previous subtotal for this coin
            prev_subtotals.push(total_delta);

            // Add the delta to the total
            total_delta += delta;

            for create_coin in create_coins {
                children.push(
                    item.cat
                        .child_from_p2_create_coin(ctx, create_coin, item.hidden),
                );
            }
        }

        // If the TAIL was revealed, we need to adjust the subsequent previous subtotals to account for the extra delta
        if let Some(tail_index) = run_tail_index {
            let tail_adjustment = -total_delta;

            prev_subtotals
                .iter_mut()
                .skip(tail_index + 1)
                .for_each(|subtotal| {
                    *subtotal += tail_adjustment;
                });
        }

        for (index, item) in cat_spends.iter().enumerate() {
            // Find information of neighboring coins on the ring.
            let prev = &cat_spends[if index == 0 { len - 1 } else { index - 1 }];
            let next = &cat_spends[if index == len - 1 { 0 } else { index + 1 }];

            let next_inner_puzzle_hash = next.cat.info.inner_puzzle_hash();

            item.cat.spend(
                ctx,
                SingleCatSpend {
                    p2_spend: item.spend,
                    prev_coin_id: prev.cat.coin.coin_id(),
                    next_coin_proof: CoinProof {
                        parent_coin_info: next.cat.coin.parent_coin_info,
                        inner_puzzle_hash: next_inner_puzzle_hash.into(),
                        amount: next.cat.coin.amount,
                    },
                    prev_subtotal: prev_subtotals[index].try_into()?,
                    // If the TAIL was revealed, we need to add the extra delta needed to net the spend to zero
                    extra_delta: if run_tail_index.is_some_and(|i| i == index) {
                        -total_delta.try_into()?
                    } else {
                        0
                    },
                    revoke: item.hidden,
                },
            )?;
        }

        Ok(children)
    }

    /// Spends this CAT coin with the provided solution parameters. Other parameters are inferred from
    /// the [`Cat`] instance.
    ///
    /// This is useful if you have already calculated the conditions and want to spend the coin directly.
    /// However, it's more common to use [`Cat::spend_all`] which handles the details of calculating the
    /// solution (including ring announcements) for multiple CATs and spending them all at once.
    pub fn spend(&self, ctx: &mut SpendContext, info: SingleCatSpend) -> Result<(), DriverError> {
        let mut spend = info.p2_spend;

        if let Some(hidden_puzzle_hash) = self.info.hidden_puzzle_hash {
            spend = RevocationLayer::new(hidden_puzzle_hash, self.info.p2_puzzle_hash)
                .construct_spend(
                    ctx,
                    RevocationSolution::new(info.revoke, spend.puzzle, spend.solution),
                )?;
        }

        if let Some(fee_policy) = &self.info.fee_policy {
            let has_hidden_revoke_layer = self.info.hidden_puzzle_hash.is_some();
            spend = FeeLayer::new(
                fee_policy.issuer_fee_puzzle_hash,
                fee_policy.fee_basis_points,
                fee_policy.min_fee,
                fee_policy.allow_zero_price,
                fee_policy.allow_revoke_fee_bypass,
                has_hidden_revoke_layer,
                spend.puzzle,
            )
            .construct_spend(
                ctx,
                FeeLayerSolution::new(spend.solution),
            )?;
        }

        spend = CatLayer::new(self.info.asset_id, spend.puzzle).construct_spend(
            ctx,
            CatSolution {
                lineage_proof: self.lineage_proof,
                inner_puzzle_solution: spend.solution,
                prev_coin_id: info.prev_coin_id,
                this_coin_info: self.coin,
                next_coin_proof: info.next_coin_proof,
                extra_delta: info.extra_delta,
                prev_subtotal: info.prev_subtotal,
            },
        )?;

        ctx.spend(self.coin, spend)?;

        Ok(())
    }

    /// Creates a [`LineageProof`] for which would be valid for any children created by this [`Cat`].
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a new [`Cat`] that represents a child of this one.
    /// The child will have the same revocation layer (or lack thereof) as the current [`Cat`].
    ///
    /// If you need to construct a child without the revocation layer, use [`Cat::unrevocable_child`].
    pub fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        let mut info = self.info.clone();
        info.p2_puzzle_hash = p2_puzzle_hash;
        self.child_with(info, amount)
    }

    /// Creates a new [`Cat`] that represents a child of this one.
    /// The child will not have a revocation layer.
    ///
    /// If you need to construct a child with the same revocation layer, use [`Cat::child`].
    pub fn unrevocable_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        let mut info = self.info.clone();
        info.p2_puzzle_hash = p2_puzzle_hash;
        info.hidden_puzzle_hash = None;
        self.child_with(info, amount)
    }

    /// Creates a new [`Cat`] that represents a child of this one.
    ///
    /// You can specify the [`CatInfo`] to use for the child manually.
    /// In most cases, you will want to use [`Cat::child`] or [`Cat::unrevocable_child`] instead.
    pub fn child_with(&self, info: CatInfo, amount: u64) -> Self {
        Self {
            coin: Coin::new(self.coin.coin_id(), info.puzzle_hash().into(), amount),
            lineage_proof: Some(self.child_lineage_proof()),
            info,
        }
    }

    /// Parses a [`Cat`] and its p2 spend from a coin spend by extracting the [`CatLayer`] and [`RevocationLayer`] if present.
    ///
    /// If the puzzle is not a CAT, this will return [`None`] instead of an error.
    /// However, if the puzzle should have been a CAT but had a parsing error, this will return an error.
    pub fn parse(
        allocator: &Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<Option<(Self, Puzzle, NodePtr)>, DriverError> {
        let Some(cat_layer) = CatLayer::<Puzzle>::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };
        let cat_solution = CatLayer::<Puzzle>::parse_solution(allocator, solution)?;
        let mut fee_policy = None;
        let mut inner_puzzle = cat_layer.inner_puzzle;
        let mut inner_solution = cat_solution.inner_puzzle_solution;

        if let Some(fee_layer) = FeeLayer::<Puzzle>::parse_puzzle(allocator, inner_puzzle)? {
            let fee_solution = FeeLayer::<Puzzle>::parse_solution(allocator, inner_solution)?;
            fee_policy = Some(FeePolicy::new(
                fee_layer.issuer_fee_puzzle_hash,
                fee_layer.fee_basis_points,
                fee_layer.min_fee,
                fee_layer.allow_zero_price,
                fee_layer.allow_revoke_fee_bypass,
            ));
            inner_puzzle = fee_layer.inner_puzzle;
            inner_solution = fee_solution.inner_solution;
        }

        if let Some(revocation_layer) = RevocationLayer::parse_puzzle(allocator, inner_puzzle)? {
            let revocation_solution = RevocationLayer::parse_solution(allocator, inner_solution)?;

            let info = Self::new(
                coin,
                cat_solution.lineage_proof,
                CatInfo::new(
                    cat_layer.asset_id,
                    Some(revocation_layer.hidden_puzzle_hash),
                    revocation_layer.inner_puzzle_hash,
                )
                .with_fee_policy(fee_policy),
            );

            Ok(Some((
                info,
                Puzzle::parse(allocator, revocation_solution.puzzle),
                revocation_solution.solution,
            )))
        } else {
            let info = Self::new(
                coin,
                cat_solution.lineage_proof,
                CatInfo::new(
                    cat_layer.asset_id,
                    None,
                    inner_puzzle.curried_puzzle_hash().into(),
                )
                .with_fee_policy(fee_policy),
            );

            Ok(Some((info, inner_puzzle, inner_solution)))
        }
    }

    /// Parses the children of a [`Cat`] from the parent coin spend.
    ///
    /// This can be used to construct a valid spendable [`Cat`] for a hinted coin.
    /// You simply need to look up the parent coin's spend, parse the children, and
    /// find the one that matches the hinted coin.
    ///
    /// There is special handling for the revocation layer.
    /// See [`Cat::child_from_p2_create_coin`] for more details.
    pub fn parse_children(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<Vec<Self>>, DriverError> {
        let Some(parent_layer) = CatLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)? else {
            return Ok(None);
        };
        let parent_solution = CatLayer::<Puzzle>::parse_solution(allocator, parent_solution)?;

        let mut hidden_puzzle_hash = None;
        let mut fee_policy = None;
        let mut inner_spend = Spend::new(
            parent_layer.inner_puzzle.ptr(),
            parent_solution.inner_puzzle_solution,
        );
        let mut p2_puzzle_hash = parent_layer.inner_puzzle.curried_puzzle_hash().into();
        let mut revoke = false;

        if let Some(fee_layer) = FeeLayer::<Puzzle>::parse_puzzle(
            allocator,
            Puzzle::parse(allocator, inner_spend.puzzle),
        )? {
            fee_policy = Some(FeePolicy::new(
                fee_layer.issuer_fee_puzzle_hash,
                fee_layer.fee_basis_points,
                fee_layer.min_fee,
                fee_layer.allow_zero_price,
                fee_layer.allow_revoke_fee_bypass,
            ));

            let fee_solution = FeeLayer::<Puzzle>::parse_solution(allocator, inner_spend.solution)?;
            inner_spend = Spend::new(fee_layer.inner_puzzle.ptr(), fee_solution.inner_solution);
            p2_puzzle_hash = fee_layer.inner_puzzle.curried_puzzle_hash().into();
        }

        if let Some(revocation_layer) =
            RevocationLayer::parse_puzzle(allocator, Puzzle::parse(allocator, inner_spend.puzzle))?
        {
            hidden_puzzle_hash = Some(revocation_layer.hidden_puzzle_hash);
            p2_puzzle_hash = revocation_layer.inner_puzzle_hash;

            let revocation_solution =
                RevocationLayer::parse_solution(allocator, inner_spend.solution)?;

            inner_spend = Spend::new(revocation_solution.puzzle, revocation_solution.solution);
            revoke = revocation_solution.hidden;
        }

        let cat = Cat::new(
            parent_coin,
            parent_solution.lineage_proof,
            CatInfo::new(parent_layer.asset_id, hidden_puzzle_hash, p2_puzzle_hash)
                .with_fee_policy(fee_policy),
        );

        let output =
            run_puzzle(allocator, inner_spend.puzzle, inner_spend.solution).map_err(|e| {
                let inner_puzzle_hash = clvm_utils::tree_hash(allocator, inner_spend.puzzle);
                DriverError::Custom(format!(
                    "failed running inner CAT spend (coin_id={}, inner_puzzle_hash={}): {e}",
                    parent_coin.coin_id(),
                    Bytes32::from(inner_puzzle_hash)
                ))
            })?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let outputs = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .map(|create_coin| cat.child_from_p2_create_coin(allocator, create_coin, revoke))
            .collect();

        Ok(Some(outputs))
    }

    /// Creates a new [`Cat`] that reflects the create coin condition in the p2 spend's conditions.
    ///
    /// There is special handling for the revocation layer:
    /// 1. If there is no revocation layer for the parent, the child will not have one either.
    /// 2. If the parent was not revoked, the child will have the same revocation layer.
    /// 3. If the parent was revoked, the child will not have a revocation layer.
    /// 4. If the parent was revoked, and the child was hinted (and wrapped with the revocation layer), it will detect it.
    pub fn child_from_p2_create_coin(
        &self,
        allocator: &Allocator,
        create_coin: CreateCoin<NodePtr>,
        revoke: bool,
    ) -> Self {
        // Child with the same hidden puzzle hash as the parent
        let child = self.child(create_coin.puzzle_hash, create_coin.amount);

        // If the parent is not revocable, we don't need to add a revocation layer
        let Some(hidden_puzzle_hash) = self.info.hidden_puzzle_hash else {
            return child;
        };

        // If we're not doing a revocation spend, we know it's wrapped in the same revocation layer
        if !revoke {
            return child;
        }

        // Child without a hidden puzzle hash but with the create coin puzzle hash as the p2 puzzle hash
        let unrevocable_child = self.unrevocable_child(create_coin.puzzle_hash, create_coin.amount);

        // If the hint is missing, just assume the child doesn't have a hidden puzzle hash
        let Memos::Some(memos) = create_coin.memos else {
            return unrevocable_child;
        };

        let Some((hint, _)) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos).ok() else {
            return unrevocable_child;
        };

        // If the hint wrapped in the revocation layer of the parent matches the create coin's puzzle hash,
        // then we know that the hint is the p2 puzzle hash and the child has the same revocation layer as the parent
        if create_coin.puzzle_hash
            == RevocationLayer::new(hidden_puzzle_hash, hint)
                .tree_hash()
                .into()
        {
            return self.child(hint, create_coin.amount);
        }

        // Otherwise, we can't determine whether there is a revocation layer or not, so we will just assume it's unrevocable
        // In practice, this should never happen while parsing a coin which is still spendable (not an ephemeral spend)
        // If it does, a new hinting mechanism should be introduced in the future to accommodate this, but for now this is the best we can do
        unrevocable_child
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_protocol::{CoinSpend, SpendBundle};
    use chia_puzzle_types::{
        cat::{CatSolution, EverythingWithSignatureTailArgs},
        offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
        standard::StandardSolution,
    };
    use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
    use chia_sdk_test::{BlsPairWithCoin, Simulator};
    use chia_sdk_types::{
        Condition,
        Mod,
        puzzles::{FeeLayerSolution, FeeTradePrice, FeeTradePriceFeePolicy, RevocationArgs},
    };
    use clvm_traits::{FromClvm, ToClvm};
    use clvm_utils::{tree_hash_atom, tree_hash_pair};
    use indexmap::indexmap;
    use rstest::rstest;

    use crate::{
        AssetInfo, CatAssetInfo, FeePolicy, Offer, OfferCoins, Relation, RequestedPayments,
        SettlementLayer, SpendWithConditions, Spends, StandardLayer,
    };

    use super::*;

    #[test]
    fn test_single_issuance_cat() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::issue_with_coin(
            ctx,
            alice.coin.coin_id(),
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = cats[0];
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(
            cat.info.asset_id,
            GenesisByCoinIdTailArgs::curry_tree_hash(alice.coin.coin_id()).into()
        );
        assert!(sim.coin_state(cat.coin.coin_id()).is_some());

        Ok(())
    }

    #[test]
    fn test_multi_issuance_cat() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::issue_with_key(
            ctx,
            alice.coin.coin_id(),
            alice.pk,
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = cats[0];
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(
            cat.info.asset_id,
            EverythingWithSignatureTailArgs::curry_tree_hash(alice.pk).into()
        );
        assert!(sim.coin_state(cat.coin.coin_id()).is_some());

        Ok(())
    }

    #[test]
    fn test_zero_cat_issuance() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(0);
        let alice_p2 = StandardLayer::new(alice.pk);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::issue_with_coin(
            ctx,
            alice.coin.coin_id(),
            0,
            Conditions::new().create_coin(alice.puzzle_hash, 0, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let cat = cats[0];
        assert_eq!(cat.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(
            cat.info.asset_id,
            GenesisByCoinIdTailArgs::curry_tree_hash(alice.coin.coin_id()).into()
        );
        assert!(sim.coin_state(cat.coin.coin_id()).is_some());

        let cat_spend = CatSpend::new(
            cat,
            alice_p2.spend_with_conditions(
                ctx,
                Conditions::new().create_coin(alice.puzzle_hash, 0, memos),
            )?,
        );
        Cat::spend_all(ctx, &[cat_spend])?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_missing_cat_issuance_output() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (issue_cat, _cats) =
            Cat::issue_with_coin(ctx, alice.coin.coin_id(), 1, Conditions::new())?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        assert_eq!(
            sim.spend_coins(ctx.take(), &[alice.sk])
                .unwrap_err()
                .to_string(),
            "Signer error: Eval error: clvm raise"
        );

        Ok(())
    }

    #[test]
    fn test_exceeded_cat_issuance_output() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, _cats) = Cat::issue_with_coin(
            ctx,
            alice.coin.coin_id(),
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 2, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        assert_eq!(
            sim.spend_coins(ctx.take(), &[alice.sk])
                .unwrap_err()
                .to_string(),
            "Signer error: Eval error: clvm raise"
        );

        Ok(())
    }

    #[rstest]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(10)]
    fn test_cat_spends(#[case] coins: usize) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        // All of the amounts are different to prevent coin id collisions.
        let mut amounts = Vec::with_capacity(coins);

        for amount in 0..coins {
            amounts.push(amount as u64);
        }

        // Create the coin with the sum of all the amounts we need to issue.
        let sum = amounts.iter().sum::<u64>();

        let alice = sim.bls(sum);
        let alice_p2 = StandardLayer::new(alice.pk);

        // Issue the CAT coins with those amounts.
        let mut conditions = Conditions::new();

        let memos = ctx.hint(alice.puzzle_hash)?;
        for &amount in &amounts {
            conditions = conditions.create_coin(alice.puzzle_hash, amount, memos);
        }

        let (issue_cat, mut cats) =
            Cat::issue_with_coin(ctx, alice.coin.coin_id(), sum, conditions)?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Spend the CAT coins a few times.
        for _ in 0..3 {
            let cat_spends: Vec<CatSpend> = cats
                .iter()
                .map(|cat| {
                    Ok(CatSpend::new(
                        *cat,
                        alice_p2.spend_with_conditions(
                            ctx,
                            Conditions::new().create_coin(
                                alice.puzzle_hash,
                                cat.coin.amount,
                                memos,
                            ),
                        )?,
                    ))
                })
                .collect::<anyhow::Result<_>>()?;

            cats = Cat::spend_all(ctx, &cat_spends)?;
            sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;
        }

        Ok(())
    }

    #[test]
    fn test_different_cat_p2_puzzles() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        // This will just return the solution verbatim.
        let custom_p2 = ctx.alloc(&1)?;
        let custom_p2_puzzle_hash = ctx.tree_hash(custom_p2).into();

        let memos = ctx.hint(alice.puzzle_hash)?;
        let custom_memos = ctx.hint(custom_p2_puzzle_hash)?;
        let (issue_cat, cats) = Cat::issue_with_coin(
            ctx,
            alice.coin.coin_id(),
            2,
            Conditions::new()
                .create_coin(alice.puzzle_hash, 1, memos)
                .create_coin(custom_p2_puzzle_hash, 1, custom_memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let spends = [
            CatSpend::new(
                cats[0],
                alice_p2.spend_with_conditions(
                    ctx,
                    Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
                )?,
            ),
            CatSpend::new(
                cats[1],
                Spend::new(
                    custom_p2,
                    ctx.alloc(&[CreateCoin::new(custom_p2_puzzle_hash, 1, custom_memos)])?,
                ),
            ),
        ];

        Cat::spend_all(ctx, &spends)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_cat_melt() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(10000);
        let alice_p2 = StandardLayer::new(alice.pk);
        let hint = ctx.hint(alice.puzzle_hash)?;

        let conditions = Conditions::new().create_coin(alice.puzzle_hash, 10000, hint);

        let (issue_cat, cats) =
            Cat::issue_with_key(ctx, alice.coin.coin_id(), alice.pk, 10000, conditions)?;

        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;

        let cat_spend = CatSpend::new(
            cats[0],
            alice_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(alice.puzzle_hash, 7000, hint)
                    .run_cat_tail(tail, NodePtr::NIL),
            )?,
        );

        Cat::spend_all(ctx, &[cat_spend])?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[rstest]
    fn test_cat_tail_reveal(
        #[values(0, 1, 2)] tail_index: usize,
        #[values(true, false)] melt: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(15000);
        let alice_p2 = StandardLayer::new(alice.pk);
        let hint = ctx.hint(alice.puzzle_hash)?;

        let conditions = Conditions::new()
            .create_coin(alice.puzzle_hash, 3000, hint)
            .create_coin(alice.puzzle_hash, 6000, hint)
            .create_coin(alice.puzzle_hash, 1000, hint);

        let (issue_cat, cats) =
            Cat::issue_with_key(ctx, alice.coin.coin_id(), alice.pk, 10000, conditions)?;

        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;

        let cat_spends = cats
            .into_iter()
            .enumerate()
            .map(|(i, cat)| {
                let mut conditions = Conditions::new();

                // Add the TAIL reveal to the second spend, to ensure the order doesn't matter
                if i == tail_index {
                    conditions.push(RunCatTail::new(tail, NodePtr::NIL));

                    if !melt {
                        conditions.push(CreateCoin::new(alice.puzzle_hash, 15000, hint));
                    }
                }

                Ok(CatSpend::new(
                    cat,
                    alice_p2.spend_with_conditions(ctx, conditions)?,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Cat::spend_all(ctx, &cat_spends)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_revocable_cat() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(10);
        let alice_p2 = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);
        let bob_p2 = StandardLayer::new(bob.pk);

        let asset_id = EverythingWithSignatureTailArgs::curry_tree_hash(alice.pk).into();
        let hint = ctx.hint(bob.puzzle_hash)?;

        let (issue_cat, cats) = Cat::issue_revocable_with_key(
            &mut ctx,
            alice.coin.coin_id(),
            alice.pk,
            alice.puzzle_hash,
            10,
            Conditions::new().create_coin(bob.puzzle_hash, 10, hint),
        )?;
        alice_p2.spend(&mut ctx, alice.coin, issue_cat)?;

        // Bob can spend the CAT because he owns it
        let cat_spend = CatSpend::new(
            cats[0],
            bob_p2.spend_with_conditions(
                &mut ctx,
                Conditions::new().create_coin(bob.puzzle_hash, 10, hint),
            )?,
        );
        let cats = Cat::spend_all(&mut ctx, &[cat_spend])?;

        // But Alice can also spend (revoke) it because she owns the revocation key
        let hint = ctx.hint(alice.puzzle_hash)?;

        let revocable_puzzle_hash = RevocationArgs::new(alice.puzzle_hash, alice.puzzle_hash)
            .curry_tree_hash()
            .into();

        let cat_spend = CatSpend::revoke(
            cats[0],
            alice_p2.spend_with_conditions(
                &mut ctx,
                Conditions::new()
                    .create_coin(alice.puzzle_hash, 5, hint)
                    .create_coin(revocable_puzzle_hash, 5, hint),
            )?,
        );

        let cats = Cat::spend_all(&mut ctx, &[cat_spend])?;

        // Validate the transaction
        sim.spend_coins(ctx.take(), &[alice.sk.clone(), bob.sk.clone()])?;

        // The first coin should exist and not be revocable
        assert_ne!(sim.coin_state(cats[0].coin.coin_id()), None);
        assert_eq!(cats[0].info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cats[0].info.asset_id, asset_id);
        assert_eq!(cats[0].info.hidden_puzzle_hash, None);

        // The second coin should exist and be revocable
        assert_ne!(sim.coin_state(cats[1].coin.coin_id()), None);
        assert_eq!(cats[1].info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cats[1].info.asset_id, asset_id);
        assert_eq!(cats[1].info.hidden_puzzle_hash, Some(alice.puzzle_hash));

        let lineage_proof = cats[0].lineage_proof;

        let parent_spend = sim.coin_spend(cats[0].coin.parent_coin_info).unwrap();
        let parent_puzzle = ctx.alloc(&parent_spend.puzzle_reveal)?;
        let parent_puzzle = Puzzle::parse(&ctx, parent_puzzle);
        let parent_solution = ctx.alloc(&parent_spend.solution)?;

        let cats =
            Cat::parse_children(&mut ctx, parent_spend.coin, parent_puzzle, parent_solution)?
                .unwrap();

        // The first coin should exist and not be revocable
        assert_ne!(sim.coin_state(cats[0].coin.coin_id()), None);
        assert_eq!(cats[0].info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cats[0].info.asset_id, asset_id);
        assert_eq!(cats[0].info.hidden_puzzle_hash, None);

        // The second coin should exist and be revocable
        assert_ne!(sim.coin_state(cats[1].coin.coin_id()), None);
        assert_eq!(cats[1].info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(cats[1].info.asset_id, asset_id);
        assert_eq!(cats[1].info.hidden_puzzle_hash, Some(alice.puzzle_hash));

        assert_eq!(cats[0].lineage_proof, lineage_proof);

        let cat_spends = cats
            .into_iter()
            .map(|cat| {
                Ok(CatSpend::revoke(
                    cat,
                    alice_p2.spend_with_conditions(
                        &mut ctx,
                        Conditions::new().create_coin(alice.puzzle_hash, 5, hint),
                    )?,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        _ = Cat::spend_all(&mut ctx, &cat_spends)?;

        // Validate the transaction
        sim.spend_coins(ctx.take(), &[alice.sk, bob.sk])?;

        Ok(())
    }

    // ===== Fee Layer Test Infrastructure =====

    #[derive(Clone, Copy, Debug)]
    enum Flavor {
        Plain,
        Revocable,
        Fee,
        FeeRevocable,
    }

    #[derive(Clone, Copy, Debug)]
    enum Direction {
        BuyBase,
        SellBase,
    }

    fn fee_tpq(policy: &FeePolicy) -> FeeTradePriceFeePolicy {
        FeeTradePriceFeePolicy {
            issuer_fee_puzzle_hash: policy.issuer_fee_puzzle_hash,
            fee_basis_points: policy.fee_basis_points,
            min_fee: policy.min_fee,
            allow_zero_price: policy.allow_zero_price,
            allow_revoke_fee_bypass: policy.allow_revoke_fee_bypass,
        }
    }

    fn fee_tp_for_cat(cat: &Cat, amount: u64) -> FeeTradePrice {
        FeeTradePrice::cat_with_quote_layers(
            amount,
            cat.info.asset_id,
            cat.info.hidden_puzzle_hash,
            cat.info.fee_policy.as_ref().map(fee_tpq),
        )
    }

    fn calc_fee(quote: u64, basis_points: u16, min_fee: u64) -> u64 {
        (quote * u64::from(basis_points) / 10_000).max(min_fee)
    }

    fn issue_cat_flavor(
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        issuer: &BlsPairWithCoin,
        target_ph: Bytes32,
        flavor: Flavor,
        amount: u64,
        fee_policy: Option<FeePolicy>,
    ) -> anyhow::Result<Cat> {
        let issuer_p2 = StandardLayer::new(issuer.pk);
        let hint = ctx.hint(target_ph)?;
        let conditions = Conditions::new().create_coin(target_ph, amount, hint);
        let (issue, cats) = match flavor {
            Flavor::Plain => {
                Cat::issue_with_key(ctx, issuer.coin.coin_id(), issuer.pk, amount, conditions)?
            }
            Flavor::Revocable => Cat::issue_revocable_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                issuer.puzzle_hash,
                amount,
                conditions,
            )?,
            Flavor::Fee => Cat::issue_fee_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                fee_policy.expect("fee policy required for Fee"),
                amount,
                conditions,
            )?,
            Flavor::FeeRevocable => Cat::issue_revocable_fee_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                issuer.puzzle_hash,
                fee_policy.expect("fee policy required for FeeRevocable"),
                amount,
                conditions,
            )?,
        };
        issuer_p2.spend(ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;
        Ok(cats[0])
    }

    // ===== XCH-Quoted Fee Cat Scenario =====

    struct XchQuotedScenario {
        sim: Simulator,
        ctx: SpendContext,
        issuer: BlsPairWithCoin,
        trader: BlsPairWithCoin,
        trader_p2: StandardLayer,
        fee_cat: Cat,
        trade_nonce: Bytes32,
        quote_amount: u64,
        expected_fee: u64,
    }

    fn setup_xch_quoted(
        allow_zero_price: bool,
        allow_revoke_fee_bypass: bool,
    ) -> anyhow::Result<XchQuotedScenario> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let issuer = sim.bls(2);
        let trader = sim.bls(100);
        let issuer_p2 = StandardLayer::new(issuer.pk);
        let trader_p2 = StandardLayer::new(trader.pk);

        let fee_policy = FeePolicy::new(
            issuer.puzzle_hash,
            500,
            1,
            allow_zero_price,
            allow_revoke_fee_bypass,
        );
        let quote_amount = 1_000;
        let exp_fee = calc_fee(quote_amount, fee_policy.fee_basis_points, fee_policy.min_fee);

        let hint = ctx.hint(trader.puzzle_hash)?;
        let (issue, cats) = Cat::issue_fee_with_key(
            &mut ctx,
            issuer.coin.coin_id(),
            issuer.pk,
            fee_policy,
            1,
            Conditions::new().create_coin(trader.puzzle_hash, 1, hint),
        )?;
        issuer_p2.spend(&mut ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        let fee_cat = cats[0];
        let trade_nonce = Offer::nonce(vec![fee_cat.coin.coin_id(), trader.coin.coin_id()]);

        Ok(XchQuotedScenario {
            sim,
            ctx,
            issuer,
            trader,
            trader_p2,
            fee_cat,
            trade_nonce,
            quote_amount,
            expected_fee: exp_fee,
        })
    }

    fn execute_xch_quoted(
        mut s: XchQuotedScenario,
        trade_prices: Vec<FeeTradePrice>,
        settlement_nonce: Option<Bytes32>,
        settlement_fee_amount: u64,
        announcement: Option<Vec<u8>>,
        use_wrong_memos: bool,
    ) -> anyhow::Result<Vec<Cat>> {
        let hint = s.ctx.hint(s.trader.puzzle_hash)?;
        let mut conditions = Conditions::new()
            .create_coin(s.trader.puzzle_hash, s.fee_cat.coin.amount, hint)
            .set_cat_trade_context(s.trade_nonce, trade_prices);
        if let Some(msg) = announcement {
            conditions = conditions.create_puzzle_announcement(msg.into());
        }

        let fee_cat_spend = CatSpend::new(
            s.fee_cat,
            s.trader_p2.spend_with_conditions(&mut s.ctx, conditions)?,
        );

        if let Some(nonce) = settlement_nonce {
            let change_hint = s.ctx.hint(s.trader.puzzle_hash)?;
            let xch_spend = s.trader_p2.spend_with_conditions(
                &mut s.ctx,
                Conditions::new()
                    .create_coin(
                        SETTLEMENT_PAYMENT_HASH.into(),
                        settlement_fee_amount,
                        Memos::None,
                    )
                    .create_coin(
                        s.trader.puzzle_hash,
                        s.trader.coin.amount - settlement_fee_amount,
                        change_hint,
                    ),
            )?;
            s.ctx.spend(s.trader.coin, xch_spend)?;

            let fee_memos = if use_wrong_memos {
                s.ctx.hint(s.trader.puzzle_hash)?
            } else {
                Memos::Some(NodePtr::NIL)
            };
            let settlement_spend = SettlementLayer.construct_spend(
                &mut s.ctx,
                SettlementPaymentsSolution::new(vec![NotarizedPayment::new(
                    nonce,
                    vec![Payment::new(s.issuer.puzzle_hash, settlement_fee_amount, fee_memos)],
                )]),
            )?;
            let settlement_coin = Coin::new(
                s.trader.coin.coin_id(),
                SETTLEMENT_PAYMENT_HASH.into(),
                settlement_fee_amount,
            );
            s.ctx.spend(settlement_coin, settlement_spend)?;
        }

        let children = Cat::spend_all(&mut s.ctx, &[fee_cat_spend])?;
        s.sim
            .spend_coins(s.ctx.take(), &[s.trader.sk.clone()])?;
        Ok(children)
    }

    // ===== CAT-Quoted Fee Cat Scenario =====

    struct CatQuotedScenario {
        sim: Simulator,
        ctx: SpendContext,
        fee_issuer: BlsPairWithCoin,
        trader: BlsPairWithCoin,
        trader_p2: StandardLayer,
        fee_cat: Cat,
        quote_cat: Cat,
        settlement_quote_cat: Cat,
        trade_nonce: Bytes32,
        quote_amount: u64,
        expected_fee: u64,
    }

    fn setup_cat_quoted() -> anyhow::Result<CatQuotedScenario> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let fee_issuer = sim.bls(2);
        let quote_issuer = sim.bls(1_000);
        let trader = sim.bls(100);
        let trader_p2 = StandardLayer::new(trader.pk);

        let quote_amount = 1_000;
        let quote_cat = issue_cat_flavor(
            &mut sim,
            &mut ctx,
            &quote_issuer,
            trader.puzzle_hash,
            Flavor::FeeRevocable,
            quote_amount,
            Some(FeePolicy::new(quote_issuer.puzzle_hash, 0, 0, true, true)),
        )?;

        let fee_policy = FeePolicy::new(fee_issuer.puzzle_hash, 500, 1, false, false);
        let exp_fee = calc_fee(quote_amount, fee_policy.fee_basis_points, fee_policy.min_fee);
        let fee_cat = issue_cat_flavor(
            &mut sim,
            &mut ctx,
            &fee_issuer,
            trader.puzzle_hash,
            Flavor::Fee,
            1,
            Some(fee_policy),
        )?;

        let trade_nonce = Offer::nonce(vec![fee_cat.coin.coin_id(), quote_cat.coin.coin_id()]);
        let settlement_ph: Bytes32 = SETTLEMENT_PAYMENT_HASH.into();

        let change_hint = ctx.hint(trader.puzzle_hash)?;
        let quote_spend = CatSpend::new(
            quote_cat,
            trader_p2.spend_with_conditions(
                &mut ctx,
                Conditions::new()
                    .create_coin(settlement_ph, exp_fee, Memos::None)
                    .create_coin(trader.puzzle_hash, quote_amount - exp_fee, change_hint),
            )?,
        );
        let quote_children = Cat::spend_all(&mut ctx, &[quote_spend])?;
        let settlement_quote_cat = quote_children
            .into_iter()
            .find(|c| c.info.p2_puzzle_hash == settlement_ph)
            .expect("missing settlement quote CAT");

        Ok(CatQuotedScenario {
            sim,
            ctx,
            fee_issuer,
            trader,
            trader_p2,
            fee_cat,
            quote_cat,
            settlement_quote_cat,
            trade_nonce,
            quote_amount,
            expected_fee: exp_fee,
        })
    }

    fn execute_cat_quoted(
        mut s: CatQuotedScenario,
        trade_price: FeeTradePrice,
        settlement: Option<(Bytes32, u64)>,
        announcement: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let hint = s.ctx.hint(s.trader.puzzle_hash)?;
        let mut conditions = Conditions::new()
            .create_coin(s.trader.puzzle_hash, s.fee_cat.coin.amount, hint)
            .set_cat_trade_context(s.trade_nonce, vec![trade_price]);
        if let Some(msg) = announcement {
            conditions = conditions.create_puzzle_announcement(msg.into());
        }

        let fee_cat_spend = CatSpend::new(
            s.fee_cat,
            s.trader_p2.spend_with_conditions(&mut s.ctx, conditions)?,
        );

        if let Some((nonce, amount)) = settlement {
            let fee_memos = s.ctx.hint(s.fee_issuer.puzzle_hash)?;
            let settlement_spend = SettlementLayer.construct_spend(
                &mut s.ctx,
                SettlementPaymentsSolution::new(vec![NotarizedPayment::new(
                    nonce,
                    vec![Payment::new(s.fee_issuer.puzzle_hash, amount, fee_memos)],
                )]),
            )?;
            Cat::spend_all(
                &mut s.ctx,
                &[CatSpend::new(s.settlement_quote_cat, settlement_spend)],
            )?;
        }

        Cat::spend_all(&mut s.ctx, &[fee_cat_spend])?;
        s.sim
            .spend_coins(s.ctx.take(), &[s.trader.sk.clone()])?;
        Ok(())
    }

    // ===== Self-Announcement Helpers =====

    fn clvm_int_bytes(value: u64) -> Vec<u8> {
        if value == 0 {
            return Vec::new();
        }
        let mut bytes = value.to_be_bytes().to_vec();
        while bytes.first() == Some(&0) {
            bytes.remove(0);
        }
        if bytes.first().is_some_and(|b| (b & 0x80) != 0) {
            bytes.insert(0, 0);
        }
        bytes
    }

    fn fee_message_hash(
        trade_nonce: Bytes32,
        issuer_ph: Bytes32,
        fee_amount: u64,
        include_memo: bool,
    ) -> Bytes32 {
        let nil = tree_hash_atom(&[]);
        let nonce_h = tree_hash_atom(trade_nonce.as_ref());
        let issuer_h = tree_hash_atom(issuer_ph.as_ref());
        let amount_h = tree_hash_atom(&clvm_int_bytes(fee_amount));

        let tail = if include_memo {
            let memos = tree_hash_pair(issuer_h, nil);
            tree_hash_pair(amount_h, tree_hash_pair(memos, nil))
        } else {
            tree_hash_pair(amount_h, tree_hash_pair(nil, nil))
        };
        let payment = tree_hash_pair(issuer_h, tail);
        Bytes32::from(tree_hash_pair(nonce_h, tree_hash_pair(payment, nil)))
    }

    // ===== Descriptor Tamper Helpers =====

    #[derive(Clone, Copy)]
    struct TamperCase {
        name: &'static str,
        mutate: fn(&mut FeeTradePrice),
    }

    fn run_tamper_cases<T>(
        cases: &[TamperCase],
        mut build: impl FnMut() -> anyhow::Result<(T, FeeTradePrice)>,
        mut execute: impl FnMut(T, FeeTradePrice) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        for case in cases {
            let (ctx, mut tp) = build()?;
            (case.mutate)(&mut tp);
            assert!(
                execute(ctx, tp).is_err(),
                "expected failure for {}",
                case.name
            );
        }
        Ok(())
    }

    // ===== Offer Helper =====

    fn build_cat_trade_offer(
        trade_nonce: Bytes32,
        base_cat: &Cat,
        quote_cat: &Cat,
        offered_amount: u64,
        requested_amount: u64,
        requested_ph: Bytes32,
        base_fee_policy: Option<FeePolicy>,
        quote_fee_policy: Option<FeePolicy>,
        direction: Direction,
    ) -> anyhow::Result<Offer> {
        let settlement_ph: Bytes32 = SETTLEMENT_PAYMENT_HASH.into();
        let mut offered = OfferCoins::new();
        let mut requested = RequestedPayments::new();

        match direction {
            Direction::BuyBase => {
                offered.cats.insert(
                    quote_cat.info.asset_id,
                    vec![quote_cat.child(settlement_ph, offered_amount)],
                );
                requested.cats.insert(
                    base_cat.info.asset_id,
                    vec![NotarizedPayment::new(
                        trade_nonce,
                        vec![Payment::new(requested_ph, requested_amount, Memos::None)],
                    )],
                );
            }
            Direction::SellBase => {
                offered.cats.insert(
                    base_cat.info.asset_id,
                    vec![base_cat.child(settlement_ph, offered_amount)],
                );
                requested.cats.insert(
                    quote_cat.info.asset_id,
                    vec![NotarizedPayment::new(
                        trade_nonce,
                        vec![Payment::new(requested_ph, requested_amount, Memos::None)],
                    )],
                );
            }
        }

        let mut asset_info = AssetInfo::new();
        asset_info.insert_cat(
            base_cat.info.asset_id,
            CatAssetInfo::new(base_cat.info.hidden_puzzle_hash, base_fee_policy)
                .with_settlement_puzzle_hash(Some(settlement_ph)),
        )?;
        asset_info.insert_cat(
            quote_cat.info.asset_id,
            CatAssetInfo::new(quote_cat.info.hidden_puzzle_hash, quote_fee_policy)
                .with_settlement_puzzle_hash(Some(settlement_ph)),
        )?;

        Ok(Offer::new(
            SpendBundle::new(Vec::new(), Default::default()),
            offered,
            requested,
            asset_info,
        ))
    }

    // ===== CLVM Grinding Helpers =====

    #[derive(Clone, Copy, ToClvm)]
    #[clvm(list)]
    struct RawQuoteFeePolicy {
        issuer_fee_puzzle_hash: Bytes32,
        quote_fee_basis_points: u16,
        quote_fee_min_fee: u64,
        quote_fee_allow_zero_price: bool,
        quote_fee_allow_revoke_fee_bypass: bool,
    }

    #[derive(Clone, Copy, ToClvm)]
    #[clvm(list)]
    struct RawTradePrice {
        amount: i64,
        asset_id: Option<Bytes32>,
        quote_hidden_puzzle_hash: Option<Bytes32>,
        quote_fee_policy: Option<RawQuoteFeePolicy>,
    }

    #[derive(ToClvm)]
    #[clvm(list)]
    struct RawSetCatTradeContextCondition {
        opcode: i8,
        trade_nonce: Bytes32,
        trade_prices: Vec<RawTradePrice>,
    }

    struct PendingMutation {
        sim: Simulator,
        ctx: SpendContext,
        spends: Vec<CoinSpend>,
        fee_cat_coin_id: Bytes32,
        trader: BlsPairWithCoin,
    }

    fn build_pending_mutation(
        allow_zero_price: bool,
        allow_revoke_fee_bypass: bool,
        include_settlement: bool,
    ) -> anyhow::Result<PendingMutation> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let issuer = sim.bls(2);
        let trader = sim.bls(100);
        let issuer_p2 = StandardLayer::new(issuer.pk);
        let trader_p2 = StandardLayer::new(trader.pk);

        let fee_policy = FeePolicy::new(
            issuer.puzzle_hash,
            500,
            1,
            allow_zero_price,
            allow_revoke_fee_bypass,
        );
        let hint = ctx.hint(trader.puzzle_hash)?;
        let (issue, cats) = Cat::issue_fee_with_key(
            &mut ctx,
            issuer.coin.coin_id(),
            issuer.pk,
            fee_policy,
            1,
            Conditions::new().create_coin(trader.puzzle_hash, 1, hint),
        )?;
        issuer_p2.spend(&mut ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        let fee_cat = cats[0];
        let fee_cat_coin_id = fee_cat.coin.coin_id();
        let trade_nonce = Offer::nonce(vec![fee_cat_coin_id, trader.coin.coin_id()]);

        let cat_hint = ctx.hint(trader.puzzle_hash)?;
        let fee_cat_spend = CatSpend::new(
            fee_cat,
            trader_p2.spend_with_conditions(
                &mut ctx,
                Conditions::new()
                    .create_coin(trader.puzzle_hash, 1, cat_hint)
                    .set_cat_trade_context(trade_nonce, vec![FeeTradePrice::xch(1_000)]),
            )?,
        );

        if include_settlement {
            let change_hint = ctx.hint(trader.puzzle_hash)?;
            let xch_spend = trader_p2.spend_with_conditions(
                &mut ctx,
                Conditions::new()
                    .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 50, Memos::None)
                    .create_coin(
                        trader.puzzle_hash,
                        trader.coin.amount - 50,
                        change_hint,
                    ),
            )?;
            ctx.spend(trader.coin, xch_spend)?;

            let settlement_spend = SettlementLayer.construct_spend(
                &mut ctx,
                SettlementPaymentsSolution::new(vec![NotarizedPayment::new(
                    trade_nonce,
                    vec![Payment::new(
                        issuer.puzzle_hash,
                        50,
                        Memos::Some(NodePtr::NIL),
                    )],
                )]),
            )?;
            ctx.spend(
                Coin::new(trader.coin.coin_id(), SETTLEMENT_PAYMENT_HASH.into(), 50),
                settlement_spend,
            )?;
        }

        Cat::spend_all(&mut ctx, &[fee_cat_spend])?;
        let spends = ctx.take();

        Ok(PendingMutation {
            sim,
            ctx,
            spends,
            fee_cat_coin_id,
            trader,
        })
    }

    fn mutate_fee_layer_in_cat(
        ctx: &mut SpendContext,
        spends: &mut [CoinSpend],
        coin_id: Bytes32,
        mutate: impl FnOnce(&mut SpendContext, FeeLayerSolution<NodePtr>) -> anyhow::Result<NodePtr>,
    ) -> anyhow::Result<()> {
        let spend = spends
            .iter_mut()
            .find(|s| s.coin.coin_id() == coin_id)
            .expect("missing fee-cat spend");

        let sol_ptr = ctx.alloc(&spend.solution)?;
        let mut cat_sol = CatSolution::<NodePtr>::from_clvm(ctx, sol_ptr)?;
        let fee_sol =
            FeeLayerSolution::<NodePtr>::from_clvm(ctx, cat_sol.inner_puzzle_solution)?;
        cat_sol.inner_puzzle_solution = mutate(ctx, fee_sol)?;
        spend.solution = ctx.serialize(&cat_sol)?;
        Ok(())
    }

    fn mutate_first_trade_amount(
        ctx: &mut SpendContext,
        sol: FeeLayerSolution<NodePtr>,
        replacement: i64,
    ) -> anyhow::Result<NodePtr> {
        let mut standard_solution =
            StandardSolution::<NodePtr, NodePtr>::from_clvm(ctx, sol.inner_solution)?;
        let conditions =
            Vec::<Condition<NodePtr>>::from_clvm(ctx, standard_solution.delegated_puzzle)?;
        let mut condition_nodes = Vec::with_capacity(conditions.len());
        let mut patched = false;

        for condition in conditions {
            if let Some(context) = condition.as_set_cat_trade_context() {
                let mut first = true;
                let raw_prices: Vec<RawTradePrice> = context
                    .trade_prices
                    .iter()
                    .map(|tp| {
                        let amount = if first {
                            first = false;
                            replacement
                        } else {
                            i64::try_from(tp.amount).expect("amount overflow")
                        };
                        RawTradePrice {
                            amount,
                            asset_id: tp.asset_id,
                            quote_hidden_puzzle_hash: tp.quote_hidden_puzzle_hash,
                            quote_fee_policy: tp.quote_fee_policy.map(|p| RawQuoteFeePolicy {
                                issuer_fee_puzzle_hash: p.issuer_fee_puzzle_hash,
                                quote_fee_basis_points: p.fee_basis_points,
                                quote_fee_min_fee: p.min_fee,
                                quote_fee_allow_zero_price: p.allow_zero_price,
                                quote_fee_allow_revoke_fee_bypass: p.allow_revoke_fee_bypass,
                            }),
                        }
                    })
                    .collect();

                condition_nodes.push(ctx.alloc(&RawSetCatTradeContextCondition {
                    opcode: -26,
                    trade_nonce: context.trade_nonce,
                    trade_prices: raw_prices,
                })?);
                patched = true;
            } else {
                condition_nodes.push(condition.to_clvm(ctx)?);
            }
        }

        if !patched {
            anyhow::bail!("missing set_cat_trade_context condition");
        }

        standard_solution.delegated_puzzle = ctx.alloc(&condition_nodes)?;
        let inner_solution = ctx.alloc(&standard_solution)?;
        Ok(ctx.alloc(&FeeLayerSolution::new(inner_solution))?)
    }

    // ===== Fee Layer Tests =====

    #[rstest]
    fn test_fee_cat_parse_roundtrip(
        #[values(Flavor::Plain, Flavor::Revocable, Flavor::Fee, Flavor::FeeRevocable)]
        flavor: Flavor,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let issuer = sim.bls(10);
        let holder = sim.bls(0);
        let issuer_p2 = StandardLayer::new(issuer.pk);
        let hint = ctx.hint(holder.puzzle_hash)?;

        let fee_policy = FeePolicy::new(issuer.puzzle_hash, 500, 1, false, true);
        let (revocable, fee_enabled) = match flavor {
            Flavor::Plain => (false, false),
            Flavor::Revocable => (true, false),
            Flavor::Fee => (false, true),
            Flavor::FeeRevocable => (true, true),
        };
        let expected_hidden = revocable.then_some(issuer.puzzle_hash);
        let expected_fee = fee_enabled.then_some(fee_policy);

        let (issue, cats) = match flavor {
            Flavor::Plain => Cat::issue_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                10,
                Conditions::new().create_coin(holder.puzzle_hash, 10, hint),
            )?,
            Flavor::Revocable => Cat::issue_revocable_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                issuer.puzzle_hash,
                10,
                Conditions::new().create_coin(holder.puzzle_hash, 10, hint),
            )?,
            Flavor::Fee => Cat::issue_fee_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                fee_policy,
                10,
                Conditions::new().create_coin(holder.puzzle_hash, 10, hint),
            )?,
            Flavor::FeeRevocable => Cat::issue_revocable_fee_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                issuer.puzzle_hash,
                fee_policy,
                10,
                Conditions::new().create_coin(holder.puzzle_hash, 10, hint),
            )?,
        };
        issuer_p2.spend(ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        let child = cats[0];
        let parent_spend = sim.coin_spend(child.coin.parent_coin_info).unwrap();
        let parent_puzzle = ctx.alloc(&parent_spend.puzzle_reveal)?;
        let parent_solution = ctx.alloc(&parent_spend.solution)?;

        let (parsed, _, _) = Cat::parse(
            ctx,
            parent_spend.coin,
            Puzzle::parse(ctx, parent_puzzle),
            parent_solution,
        )?
        .unwrap();
        assert_eq!(parsed.info.hidden_puzzle_hash, expected_hidden);
        assert_eq!(parsed.info.fee_policy, expected_fee);

        let parsed_puzzle = Puzzle::parse(ctx, parent_puzzle);
        let children = Cat::parse_children(
            ctx,
            parent_spend.coin,
            parsed_puzzle,
            parent_solution,
        )?
        .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].coin.coin_id(), child.coin.coin_id());
        assert_eq!(children[0].info.hidden_puzzle_hash, expected_hidden);
        assert_eq!(children[0].info.fee_policy, expected_fee);

        Ok(())
    }

    #[rstest]
    fn test_fee_cat_multi_issuance(
        #[values(false, true)] revocable: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let issuer = sim.bls(10);
        let holder = sim.bls(0);
        let issuer_p2 = StandardLayer::new(issuer.pk);
        let hint = ctx.hint(holder.puzzle_hash)?;

        let fee_policy = FeePolicy::new(issuer.puzzle_hash, 500, 1, false, true);
        let expected_hidden = revocable.then_some(issuer.puzzle_hash);

        let conditions = Conditions::new()
            .create_coin(holder.puzzle_hash, 5, hint.clone())
            .create_coin(holder.puzzle_hash, 3, hint.clone())
            .create_coin(holder.puzzle_hash, 2, hint);

        let (issue, cats) = if revocable {
            Cat::issue_revocable_fee_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                issuer.puzzle_hash,
                fee_policy,
                10,
                conditions,
            )?
        } else {
            Cat::issue_fee_with_key(
                ctx,
                issuer.coin.coin_id(),
                issuer.pk,
                fee_policy,
                10,
                conditions,
            )?
        };
        issuer_p2.spend(ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        assert_eq!(cats.len(), 3);
        for cat in &cats {
            assert_eq!(cat.info.hidden_puzzle_hash, expected_hidden);
            assert_eq!(cat.info.fee_policy, Some(fee_policy));
        }

        let parent_spend = sim.coin_spend(cats[0].coin.parent_coin_info).unwrap();
        let puzzle = ctx.alloc(&parent_spend.puzzle_reveal)?;
        let solution = ctx.alloc(&parent_spend.solution)?;
        let parsed_puzzle = Puzzle::parse(ctx, puzzle);
        let parsed = Cat::parse_children(
            ctx,
            parent_spend.coin,
            parsed_puzzle,
            solution,
        )?
        .unwrap();

        assert_eq!(parsed.len(), 3);
        for child in &parsed {
            assert_eq!(child.info.hidden_puzzle_hash, expected_hidden);
            assert_eq!(child.info.fee_policy, Some(fee_policy));
        }
        for issued in &cats {
            assert!(parsed
                .iter()
                .any(|p| p.coin.coin_id() == issued.coin.coin_id()));
        }

        Ok(())
    }

    #[rstest]
    fn test_revocable_fee_cat_bypass(
        #[values(true, false)] allow_revoke_fee_bypass: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let issuer = sim.bls(10);
        let holder = sim.bls(0);
        let issuer_p2 = StandardLayer::new(issuer.pk);

        let policy = FeePolicy::new(issuer.puzzle_hash, 500, 1, false, allow_revoke_fee_bypass);
        let hint = ctx.hint(holder.puzzle_hash)?;
        let (issue, cats) = Cat::issue_revocable_fee_with_key(
            ctx,
            issuer.coin.coin_id(),
            issuer.pk,
            issuer.puzzle_hash,
            policy,
            10,
            Conditions::new().create_coin(holder.puzzle_hash, 10, hint),
        )?;
        issuer_p2.spend(ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        let issuer_hint = ctx.hint(issuer.puzzle_hash)?;
        let revoke_spend = issuer_p2.spend_with_conditions(
            ctx,
            Conditions::new().create_coin(issuer.puzzle_hash, 10, issuer_hint),
        )?;
        let revoked = Cat::spend_all(ctx, &[CatSpend::revoke(cats[0], revoke_spend)])?;
        let result = sim.spend_coins(ctx.take(), &[issuer.sk.clone()]);

        if allow_revoke_fee_bypass {
            result?;
            assert_eq!(revoked[0].coin.amount, 10);
            assert_eq!(revoked[0].info.hidden_puzzle_hash, None);
            assert!(revoked[0].info.fee_policy.is_some());
        } else {
            assert!(result.is_err());
        }

        Ok(())
    }

    #[test]
    fn test_fee_cat_melt_by_issuer() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let issuer = sim.bls(20_000);
        let issuer_p2 = StandardLayer::new(issuer.pk);

        let fee_policy = FeePolicy::new(issuer.puzzle_hash, 500, 1, false, true);
        let hint = ctx.hint(issuer.puzzle_hash)?;
        let (issue, cats) = Cat::issue_fee_with_key(
            ctx,
            issuer.coin.coin_id(),
            issuer.pk,
            fee_policy,
            10_000,
            Conditions::new().create_coin(issuer.puzzle_hash, 10_000, hint),
        )?;
        issuer_p2.spend(ctx, issuer.coin, issue)?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(issuer.pk))?;
        let melt_hint = ctx.hint(issuer.puzzle_hash)?;
        let melt_spend = CatSpend::new(
            cats[0],
            issuer_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(issuer.puzzle_hash, 7_000, melt_hint)
                    .run_cat_tail(tail, NodePtr::NIL),
            )?,
        );
        let melted = Cat::spend_all(ctx, &[melt_spend])?;
        sim.spend_coins(ctx.take(), &[issuer.sk.clone()])?;

        assert_eq!(melted[0].coin.amount, 7_000);
        assert!(melted[0].info.fee_policy.is_some());

        Ok(())
    }

    #[rstest]
    fn test_xch_quoted_fee_transfer(
        #[values(
            "success",
            "no_payment",
            "wrong_memos",
            "wrong_nonce",
            "underpay",
            "overpay",
            "two_prices_one_payment",
            "extra_announcement",
            "self_announcement_bypass",
            "zero_price_allowed",
            "zero_price_rejected",
            "non_revocable_enforces"
        )]
        case: &str,
    ) -> anyhow::Result<()> {
        let (allow_zero, non_revocable) = match case {
            "zero_price_allowed" => (true, false),
            "non_revocable_enforces" => (false, true),
            _ => (false, false),
        };
        let s = setup_xch_quoted(allow_zero, non_revocable)?;
        let q = s.quote_amount;
        let f = s.expected_fee;
        let n = s.trade_nonce;
        let issuer_ph = s.issuer.puzzle_hash;
        let fee_cat_asset_id = s.fee_cat.info.asset_id;
        let fee_cat_hidden = s.fee_cat.info.hidden_puzzle_hash;
        let fee_cat_policy = s.fee_cat.info.fee_policy;

        let (prices, nonce, fee_amt, ann, wrong_memos, should_succeed) = match case {
            "success" => (vec![FeeTradePrice::xch(q)], Some(n), f, None, false, true),
            "no_payment" => (vec![FeeTradePrice::xch(q)], None, 0, None, false, false),
            "wrong_memos" => (vec![FeeTradePrice::xch(q)], Some(n), f, None, true, false),
            "wrong_nonce" => (
                vec![FeeTradePrice::xch(q)],
                Some(Bytes32::new([9; 32])),
                f,
                None,
                false,
                false,
            ),
            "underpay" => (
                vec![FeeTradePrice::xch(q)],
                Some(n),
                f - 1,
                None,
                false,
                false,
            ),
            "overpay" => (
                vec![FeeTradePrice::xch(q)],
                Some(n),
                f + 1,
                None,
                false,
                false,
            ),
            "two_prices_one_payment" => (
                vec![FeeTradePrice::xch(600), FeeTradePrice::xch(400)],
                Some(n),
                calc_fee(600, 500, 1),
                None,
                false,
                false,
            ),
            "extra_announcement" => {
                let mut msg = vec![0xcf_u8];
                msg.extend_from_slice(&[0x42; 32]);
                (vec![FeeTradePrice::xch(q)], Some(n), f, Some(msg), false, true)
            }
            "self_announcement_bypass" => {
                let mut tp = FeeTradePrice::xch(q);
                tp.asset_id = Some(fee_cat_asset_id);
                tp.quote_hidden_puzzle_hash = fee_cat_hidden;
                tp.quote_fee_policy = fee_cat_policy.as_ref().map(fee_tpq);
                let ann =
                    fee_message_hash(n, issuer_ph, f, false).as_ref().to_vec();
                (vec![tp], None, 0, Some(ann), false, false)
            }
            "zero_price_allowed" => (vec![FeeTradePrice::xch(0)], None, 0, None, false, true),
            "zero_price_rejected" => (vec![FeeTradePrice::xch(0)], None, 0, None, false, false),
            "non_revocable_enforces" => (
                vec![FeeTradePrice::xch(q)],
                None,
                0,
                None,
                false,
                false,
            ),
            _ => unreachable!(),
        };

        let result = execute_xch_quoted(s, prices, nonce, fee_amt, ann, wrong_memos);
        if should_succeed {
            let children = result?;
            assert_eq!(children[0].coin.amount, 1);
            assert!(children[0].info.fee_policy.is_some());
        } else {
            assert!(result.is_err(), "expected failure for {case}");
        }

        Ok(())
    }

    #[test]
    fn test_plain_cat_quoted_fee_payment() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let fee_issuer = sim.bls(2);
        let quote_issuer = sim.bls(1_000);
        let trader = sim.bls(100);

        let fee_issuer_p2 = StandardLayer::new(fee_issuer.pk);
        let quote_issuer_p2 = StandardLayer::new(quote_issuer.pk);
        let trader_p2 = StandardLayer::new(trader.pk);

        let hint = ctx.hint(trader.puzzle_hash)?;
        let (issue_quote, quote_cats) = Cat::issue_with_key(
            ctx,
            quote_issuer.coin.coin_id(),
            quote_issuer.pk,
            1_000,
            Conditions::new().create_coin(trader.puzzle_hash, 1_000, hint),
        )?;
        quote_issuer_p2.spend(ctx, quote_issuer.coin, issue_quote)?;
        sim.spend_coins(ctx.take(), &[quote_issuer.sk.clone()])?;
        let quote_cat = quote_cats[0];

        let fee_policy = FeePolicy::new(fee_issuer.puzzle_hash, 500, 1, false, false);
        let hint = ctx.hint(trader.puzzle_hash)?;
        let (issue_fee, fee_cats) = Cat::issue_fee_with_key(
            ctx,
            fee_issuer.coin.coin_id(),
            fee_issuer.pk,
            fee_policy,
            1,
            Conditions::new().create_coin(trader.puzzle_hash, 1, hint),
        )?;
        fee_issuer_p2.spend(ctx, fee_issuer.coin, issue_fee)?;
        sim.spend_coins(ctx.take(), &[fee_issuer.sk.clone()])?;
        let fee_cat = fee_cats[0];

        let trade_nonce =
            Offer::nonce(vec![fee_cat.coin.coin_id(), quote_cat.coin.coin_id()]);
        let settlement_ph: Bytes32 = SETTLEMENT_PAYMENT_HASH.into();
        let fee_amount = 50;

        let fee_hint = ctx.hint(trader.puzzle_hash)?;
        let fee_cat_spend = CatSpend::new(
            fee_cat,
            trader_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(trader.puzzle_hash, 1, fee_hint)
                    .set_cat_trade_context(trade_nonce, vec![fee_tp_for_cat(&quote_cat, 1_000)]),
            )?,
        );

        let change_hint = ctx.hint(trader.puzzle_hash)?;
        let quote_spend = CatSpend::new(
            quote_cat,
            trader_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(settlement_ph, fee_amount, Memos::None)
                    .create_coin(trader.puzzle_hash, 1_000 - fee_amount, change_hint),
            )?,
        );
        let quote_children = Cat::spend_all(ctx, &[quote_spend])?;
        let settlement_cat = quote_children
            .into_iter()
            .find(|c| c.info.p2_puzzle_hash == settlement_ph)
            .unwrap();

        let fee_memos = ctx.hint(fee_issuer.puzzle_hash)?;
        let settlement_spend = SettlementLayer.construct_spend(
            ctx,
            SettlementPaymentsSolution::new(vec![NotarizedPayment::new(
                trade_nonce,
                vec![Payment::new(fee_issuer.puzzle_hash, fee_amount, fee_memos)],
            )]),
        )?;
        Cat::spend_all(ctx, &[CatSpend::new(settlement_cat, settlement_spend)])?;

        let children = Cat::spend_all(ctx, &[fee_cat_spend])?;
        sim.spend_coins(ctx.take(), &[trader.sk.clone()])?;

        assert_eq!(children[0].coin.amount, 1);
        assert!(children[0].info.fee_policy.is_some());

        Ok(())
    }

    #[rstest]
    fn test_descriptor_tampering(
        #[values("cat_quote", "xch_quote")] quote_type: &str,
    ) -> anyhow::Result<()> {
        let mut cases: Vec<TamperCase> = vec![
            TamperCase { name: "asset_id", mutate: |tp| { tp.asset_id = Some(Bytes32::new([0x22; 32])); } },
            TamperCase { name: "hidden_puzzle_hash", mutate: |tp| { tp.quote_hidden_puzzle_hash = Some(Bytes32::new([0x34; 32])); } },
        ];

        match quote_type {
            "cat_quote" => {
                cases.extend([
                    TamperCase { name: "asset_id_nil", mutate: |tp| { tp.asset_id = None; } },
                    TamperCase { name: "hidden_puzzle_hash_removed", mutate: |tp| { tp.quote_hidden_puzzle_hash = None; } },
                    TamperCase { name: "fee_policy_removed", mutate: |tp| { tp.quote_fee_policy = None; } },
                    TamperCase { name: "issuer_fee_puzzle_hash", mutate: |tp| { tp.quote_fee_policy = tp.quote_fee_policy.map(|mut p| { p.issuer_fee_puzzle_hash = Bytes32::new([0x35; 32]); p }); } },
                    TamperCase { name: "basis_points", mutate: |tp| { tp.quote_fee_policy = tp.quote_fee_policy.map(|mut p| { p.fee_basis_points += 1; p }); } },
                    TamperCase { name: "min_fee", mutate: |tp| { tp.quote_fee_policy = tp.quote_fee_policy.map(|mut p| { p.min_fee += 1; p }); } },
                    TamperCase { name: "allow_zero_price", mutate: |tp| { tp.quote_fee_policy = tp.quote_fee_policy.map(|mut p| { p.allow_zero_price = !p.allow_zero_price; p }); } },
                    TamperCase { name: "allow_revoke_fee_bypass", mutate: |tp| { tp.quote_fee_policy = tp.quote_fee_policy.map(|mut p| { p.allow_revoke_fee_bypass = !p.allow_revoke_fee_bypass; p }); } },
                ]);
                run_tamper_cases(
                    &cases,
                    || {
                        let s = setup_cat_quoted()?;
                        let tp = fee_tp_for_cat(&s.quote_cat, s.quote_amount);
                        Ok((s, tp))
                    },
                    |s, tp| {
                        let nonce = s.trade_nonce;
                        let fee = s.expected_fee;
                        execute_cat_quoted(s, tp, Some((nonce, fee)), None)
                    },
                )
            }
            "xch_quote" => {
                cases.push(TamperCase { name: "fee_policy", mutate: |tp| { tp.quote_fee_policy = Some(FeeTradePriceFeePolicy::default()); } });
                run_tamper_cases(
                    &cases,
                    || {
                        let s = setup_xch_quoted(false, false)?;
                        let tp = FeeTradePrice::xch(s.quote_amount);
                        Ok((s, tp))
                    },
                    |s, tp| {
                        let n = s.trade_nonce;
                        let f = s.expected_fee;
                        execute_xch_quoted(s, vec![tp], Some(n), f, None, false).map(|_| ())
                    },
                )
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_cat_quote_self_announcement_bypass() -> anyhow::Result<()> {
        let s = setup_cat_quoted()?;
        let n = s.trade_nonce;
        let f = s.expected_fee;
        let issuer_ph = s.fee_issuer.puzzle_hash;

        let mut tp = fee_tp_for_cat(&s.quote_cat, s.quote_amount);
        tp.asset_id = Some(s.fee_cat.info.asset_id);
        tp.quote_hidden_puzzle_hash = s.fee_cat.info.hidden_puzzle_hash;
        tp.quote_fee_policy = s.fee_cat.info.fee_policy.as_ref().map(fee_tpq);

        let ann = fee_message_hash(n, issuer_ph, f, true).as_ref().to_vec();

        assert!(execute_cat_quoted(s, tp, None, Some(ann)).is_err());
        Ok(())
    }

    #[rstest]
    fn test_offer_with_transfer_fees(
        #[values(Flavor::Plain, Flavor::Fee, Flavor::FeeRevocable)] base_flavor: Flavor,
        #[values(Flavor::Plain, Flavor::Revocable, Flavor::Fee, Flavor::FeeRevocable)]
        quote_flavor: Flavor,
        #[values(Direction::BuyBase, Direction::SellBase)] direction: Direction,
    ) -> anyhow::Result<()> {
        let fee_on_quote = matches!(base_flavor, Flavor::Plain);

        if fee_on_quote
            && (!matches!(quote_flavor, Flavor::Fee | Flavor::FeeRevocable)
                || !matches!(direction, Direction::BuyBase))
        {
            return Ok(());
        }

        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let base_issuer = sim.bls(1_000);
        let quote_issuer = sim.bls(1_000);
        let taker = sim.bls(if fee_on_quote { 200 } else { 500 });

        let quote_amount = 1_000u64;
        let base_amount = 200u64;

        let (base_fee_policy, quote_fee_policy) = if fee_on_quote {
            (
                None,
                Some(FeePolicy::new(quote_issuer.puzzle_hash, 500, 1, false, true)),
            )
        } else {
            (
                Some(FeePolicy::new(base_issuer.puzzle_hash, 500, 1, false, true)),
                Some(FeePolicy::new(quote_issuer.puzzle_hash, 0, 0, true, true)),
            )
        };

        let quote_cat = issue_cat_flavor(
            &mut sim,
            ctx,
            &quote_issuer,
            taker.puzzle_hash,
            quote_flavor,
            quote_amount,
            quote_fee_policy,
        )?;

        let base_cat = issue_cat_flavor(
            &mut sim,
            ctx,
            &base_issuer,
            taker.puzzle_hash,
            base_flavor,
            base_amount,
            base_fee_policy,
        )?;

        let trade_nonce = if fee_on_quote {
            Offer::nonce(vec![quote_cat.coin.coin_id(), base_cat.coin.coin_id()])
        } else {
            Offer::nonce(vec![base_cat.coin.coin_id(), quote_cat.coin.coin_id()])
        };

        let (offered_amount, requested_ph) = if fee_on_quote {
            (quote_amount, quote_issuer.puzzle_hash)
        } else {
            match direction {
                Direction::BuyBase => (quote_amount, base_issuer.puzzle_hash),
                Direction::SellBase => (base_amount, quote_issuer.puzzle_hash),
            }
        };

        let offer = build_cat_trade_offer(
            trade_nonce,
            &base_cat,
            &quote_cat,
            offered_amount,
            100,
            requested_ph,
            base_cat.info.fee_policy,
            quote_cat.info.fee_policy,
            if fee_on_quote { Direction::BuyBase } else { direction },
        )?;

        let mut spends = Spends::new(taker.puzzle_hash);
        spends.add(quote_cat);
        spends.add(base_cat);
        offer.apply_transfer_fee_trade_context(&mut spends)?;
        let actions = offer.take_actions_with_transfer_fees(ctx)?;
        let deltas = spends.apply(ctx, &actions)?;
        spends.finish_with_keys(
            ctx,
            &deltas,
            Relation::None,
            &indexmap! { taker.puzzle_hash => taker.pk },
        )?;

        sim.spend_coins(ctx.take(), &[taker.sk.clone()])?;
        Ok(())
    }

    #[rstest]
    fn test_clvm_grinding_attack(
        #[values("negative", "zero", "malformed_inner")] case: &str,
    ) -> anyhow::Result<()> {
        let (include_settlement, allow_zero, allow_bypass) = match case {
            "negative" | "zero" => (true, false, false),
            "malformed_inner" => (false, false, true),
            _ => unreachable!(),
        };
        let mut s = build_pending_mutation(allow_zero, allow_bypass, include_settlement)?;

        mutate_fee_layer_in_cat(
            &mut s.ctx,
            &mut s.spends,
            s.fee_cat_coin_id,
            |ctx, sol| match case {
                "negative" => mutate_first_trade_amount(ctx, sol, -1),
                "zero" => mutate_first_trade_amount(ctx, sol, 0),
                "malformed_inner" => {
                    let malformed = ctx.alloc(&(1_i64, NodePtr::NIL))?;
                    let mutated = FeeLayerSolution::new(malformed);
                    Ok(ctx.alloc(&mutated)?)
                }
                _ => unreachable!(),
            },
        )?;

        assert!(s.sim.spend_coins(s.spends, &[s.trader.sk]).is_err());
        Ok(())
    }
}
