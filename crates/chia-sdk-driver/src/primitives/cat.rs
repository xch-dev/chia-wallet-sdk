use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    cat::{CatSolution, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs},
    CoinProof, LineageProof, Memos,
};
use chia_sdk_types::{
    conditions::{CreateCoin, RunCatTail},
    puzzles::{RevocationArgs, RevocationSolution},
    run_puzzle, Condition, Conditions, Mod,
};
use clvm_traits::{clvm_quote, FromClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Puzzle, RevocationLayer, Spend, SpendContext};

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
            ctx.tree_hash(tail).into(),
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
            ctx.tree_hash(tail).into(),
            amount,
            RunCatTail::new(tail, NodePtr::NIL),
            extra_conditions,
        )
    }

    pub fn issue(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        asset_id: Bytes32,
        amount: u64,
        run_tail: RunCatTail<NodePtr, NodePtr>,
        conditions: Conditions,
    ) -> Result<(Conditions, Vec<Cat>), DriverError> {
        let p2_puzzle = ctx.alloc_hashed(&clvm_quote!(conditions.with(run_tail)))?;
        let puzzle_hash = CatLayer::new(asset_id, p2_puzzle).tree_hash().into();

        let eve = Cat::new(
            Coin::new(parent_coin_id, puzzle_hash, amount),
            None,
            CatInfo::new(asset_id, None, p2_puzzle.tree_hash().into()),
        );

        let children = Cat::spend_all(
            ctx,
            &[CatSpend::new(
                eve,
                Spend::new(p2_puzzle.ptr(), NodePtr::NIL),
            )],
        )?;

        Ok((
            Conditions::new().create_coin(puzzle_hash, amount, Memos::None),
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

        for (index, &item) in cat_spends.iter().enumerate() {
            // Calculate the delta and add it to the subtotal.
            let output = ctx.run(item.inner_spend.puzzle, item.inner_spend.solution)?;
            let conditions: Vec<Condition> = ctx.extract(output)?;

            if conditions.iter().any(Condition::is_run_cat_tail) {
                run_tail_index = Some(index);
            }

            let create_coins: Vec<CreateCoin<NodePtr>> = conditions
                .into_iter()
                .filter_map(Condition::into_create_coin)
                .collect();

            let delta = create_coins
                .iter()
                .fold(i128::from(item.cat.coin.amount), |delta, create_coin| {
                    delta - i128::from(create_coin.amount)
                });

            let prev_subtotal = total_delta;
            total_delta += delta;

            prev_subtotals.push(prev_subtotal);

            for create_coin in create_coins {
                children.push(
                    item.cat
                        .child_from_p2_create_coin(ctx, create_coin, item.revoke),
                );
            }
        }

        for (index, item) in cat_spends.iter().enumerate() {
            // Find information of neighboring coins on the ring.
            let prev = &cat_spends[if index == 0 { len - 1 } else { index - 1 }];
            let next = &cat_spends[if index == len - 1 { 0 } else { index + 1 }];

            let next_p2_puzzle_hash = ctx.tree_hash(next.inner_spend.puzzle).into();

            item.cat.spend(
                ctx,
                SingleCatSpend {
                    inner_spend: item.inner_spend,
                    prev_coin_id: prev.cat.coin.coin_id(),
                    next_coin_proof: CoinProof {
                        parent_coin_info: next.cat.coin.parent_coin_info,
                        inner_puzzle_hash: if let Some(hidden_puzzle_hash) =
                            item.cat.info.hidden_puzzle_hash
                        {
                            RevocationArgs::new(hidden_puzzle_hash, next_p2_puzzle_hash)
                                .curry_tree_hash()
                                .into()
                        } else {
                            next_p2_puzzle_hash
                        },
                        amount: next.cat.coin.amount,
                    },
                    prev_subtotal: prev_subtotals[index].try_into()?,
                    extra_delta: if run_tail_index.is_some_and(|i| i == index) {
                        -total_delta.try_into()?
                    } else {
                        0
                    },
                    revoke: item.revoke,
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
        let mut spend = info.inner_spend;

        if let Some(hidden_puzzle_hash) = self.info.hidden_puzzle_hash {
            spend = RevocationLayer::new(hidden_puzzle_hash, self.info.p2_puzzle_hash)
                .construct_spend(
                    ctx,
                    RevocationSolution::new(info.revoke, spend.puzzle, spend.solution),
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
        self.child_with(
            CatInfo {
                p2_puzzle_hash,
                ..self.info
            },
            amount,
        )
    }

    /// Creates a new [`Cat`] that represents a child of this one.
    /// The child will not have a revocation layer.
    ///
    /// If you need to construct a child with the same revocation layer, use [`Cat::child`].
    pub fn unrevocable_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        self.child_with(
            CatInfo {
                p2_puzzle_hash,
                hidden_puzzle_hash: None,
                ..self.info
            },
            amount,
        )
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
}

impl Cat {
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
    ) -> Result<Option<Vec<Self>>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_layer) = CatLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)? else {
            return Ok(None);
        };
        let parent_solution = CatLayer::<Puzzle>::parse_solution(allocator, parent_solution)?;

        let mut hidden_puzzle_hash = None;
        let mut inner_spend = Spend::new(
            parent_layer.inner_puzzle.ptr(),
            parent_solution.inner_puzzle_solution,
        );
        let mut revoke = false;

        if let Some(revocation_layer) =
            RevocationLayer::parse_puzzle(allocator, parent_layer.inner_puzzle)?
        {
            hidden_puzzle_hash = Some(revocation_layer.hidden_puzzle_hash);

            let revocation_solution =
                RevocationLayer::parse_solution(allocator, parent_solution.inner_puzzle_solution)?;

            inner_spend = Spend::new(revocation_solution.puzzle, revocation_solution.solution);
            revoke = revocation_solution.hidden;
        }

        let cat = Cat::new(
            parent_coin,
            parent_solution.lineage_proof,
            CatInfo::new(
                parent_layer.asset_id,
                hidden_puzzle_hash,
                tree_hash(allocator, inner_spend.puzzle).into(),
            ),
        );

        let output = run_puzzle(allocator, inner_spend.puzzle, inner_spend.solution)?;
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
        if hint
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
    use chia_puzzle_types::cat::EverythingWithSignatureTailArgs;
    use chia_sdk_test::Simulator;
    use rstest::rstest;

    use crate::{SpendWithConditions, StandardLayer};

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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

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
            "Signer error: Eval error: Error at NodePtr(SmallAtom, 0): clvm raise"
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
            "Signer error: Eval error: Error at NodePtr(SmallAtom, 0): clvm raise"
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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

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
            sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;
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
        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

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

        let memos = ctx.hint(alice.puzzle_hash)?;
        let conditions = Conditions::new().create_coin(alice.puzzle_hash, 10000, memos);
        let (issue_cat, cats) =
            Cat::issue_with_key(ctx, alice.coin.coin_id(), alice.pk, 10000, conditions)?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;

        let cat_spend = CatSpend::new(
            cats[0],
            alice_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(alice.puzzle_hash, 7000, memos)
                    .run_cat_tail(tail, NodePtr::NIL),
            )?,
        );

        Cat::spend_all(ctx, &[cat_spend])?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
