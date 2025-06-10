use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    cat::{CatSolution, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs},
    CoinProof, LineageProof, Memos,
};
use chia_sdk_types::{
    conditions::CreateCoin, puzzles::RevocationSolution, run_puzzle, Condition, Conditions,
};
use clvm_traits::{clvm_quote, FromClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Puzzle, RevocationLayer, Spend, SpendContext};

mod cat_info;
mod cat_spend;
mod single_cat_spend;

pub use cat_info::*;
pub use cat_spend::*;
pub use single_cat_spend::*;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
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

    pub fn single_issuance_eve(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Cat), DriverError> {
        let tail = ctx.curry(GenesisByCoinIdTailArgs::new(parent_coin_id))?;

        Self::create_and_spend_eve(
            ctx,
            parent_coin_id,
            ctx.tree_hash(tail).into(),
            amount,
            extra_conditions.run_cat_tail(tail, NodePtr::NIL),
        )
    }

    pub fn multi_issuance_eve(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        public_key: PublicKey,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Cat), DriverError> {
        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(public_key))?;

        Self::create_and_spend_eve(
            ctx,
            parent_coin_id,
            ctx.tree_hash(tail).into(),
            amount,
            extra_conditions.run_cat_tail(tail, NodePtr::NIL),
        )
    }

    pub fn create_and_spend_eve(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        asset_id: Bytes32,
        amount: u64,
        conditions: Conditions,
    ) -> Result<(Conditions, Cat), DriverError> {
        let p2_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let p2_puzzle_hash = ctx.tree_hash(p2_puzzle).into();
        let puzzle = CatLayer::new(asset_id, p2_puzzle).construct_puzzle(ctx)?;
        let puzzle_hash = ctx.tree_hash(puzzle).into();

        let eve = Cat::new(
            Coin::new(parent_coin_id, puzzle_hash, amount),
            None,
            CatInfo::new(asset_id, None, p2_puzzle_hash),
        );

        eve.spend_eve(ctx, Spend::new(p2_puzzle, NodePtr::NIL))?;

        Ok((
            Conditions::new().create_coin(puzzle_hash, amount, Memos::None),
            eve,
        ))
    }

    pub fn spend_all(ctx: &mut SpendContext, cat_spends: &[CatSpend]) -> Result<(), DriverError> {
        let len = cat_spends.len();

        let mut total_delta = 0;

        for (index, cat_spend) in cat_spends.iter().enumerate() {
            let CatSpend {
                cat,
                inner_spend,
                extra_delta,
                revoke,
            } = cat_spend;

            // Calculate the delta and add it to the subtotal.
            let output = ctx.run(inner_spend.puzzle, inner_spend.solution)?;
            let conditions: Vec<NodePtr> = ctx.extract(output)?;

            let create_coins = conditions
                .into_iter()
                .filter_map(|ptr| ctx.extract::<CreateCoin<NodePtr>>(ptr).ok());

            let delta = create_coins.fold(
                i128::from(cat.coin.amount) - i128::from(*extra_delta),
                |delta, create_coin| delta - i128::from(create_coin.amount),
            );

            let prev_subtotal = total_delta;
            total_delta += delta;

            // Find information of neighboring coins on the ring.
            let prev = &cat_spends[if index == 0 { len - 1 } else { index - 1 }];
            let next = &cat_spends[if index == len - 1 { 0 } else { index + 1 }];

            cat.spend(
                ctx,
                SingleCatSpend {
                    inner_spend: *inner_spend,
                    prev_coin_id: prev.cat.coin.coin_id(),
                    next_coin_proof: CoinProof {
                        parent_coin_info: next.cat.coin.parent_coin_info,
                        inner_puzzle_hash: ctx.tree_hash(next.inner_spend.puzzle).into(),
                        amount: next.cat.coin.amount,
                    },
                    prev_subtotal: prev_subtotal.try_into()?,
                    extra_delta: *extra_delta,
                    revoke: *revoke,
                },
            )?;
        }

        Ok(())
    }

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

    pub fn spend_eve(&self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<(), DriverError> {
        self.spend(
            ctx,
            SingleCatSpend::eve(self.coin, self.info.inner_puzzle_hash(), inner_spend),
        )
    }

    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash(),
            parent_amount: self.coin.amount,
        }
    }

    pub fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        let info = CatInfo {
            p2_puzzle_hash,
            ..self.info
        };
        Self {
            coin: Coin::new(self.coin.coin_id(), info.puzzle_hash(), amount),
            lineage_proof: Some(self.child_lineage_proof()),
            info,
        }
    }
}

impl Cat {
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

        if let Some(revocation_layer) =
            RevocationLayer::parse_puzzle(allocator, parent_layer.inner_puzzle)?
        {
            hidden_puzzle_hash = Some(revocation_layer.hidden_puzzle_hash);

            let revocation_solution =
                RevocationLayer::parse_solution(allocator, parent_solution.inner_puzzle_solution)?;

            inner_spend = Spend::new(revocation_solution.puzzle, revocation_solution.solution);
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
            .map(|create_coin| cat.child(create_coin.puzzle_hash, create_coin.amount))
            .collect();

        Ok(Some(outputs))
    }
}

#[cfg(test)]
mod tests {
    use chia_consensus::validation_error::ErrorCode;
    use chia_puzzle_types::cat::EverythingWithSignatureTailArgs;
    use chia_sdk_test::{Simulator, SimulatorError};
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
        let (issue_cat, cat) = Cat::single_issuance_eve(
            ctx,
            alice.coin.coin_id(),
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = cat.child(alice.puzzle_hash, 1);
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
        let (issue_cat, cat) = Cat::multi_issuance_eve(
            ctx,
            alice.coin.coin_id(),
            alice.pk,
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let cat = cat.child(alice.puzzle_hash, 1);
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
        let (issue_cat, cat) = Cat::single_issuance_eve(
            ctx,
            alice.coin.coin_id(),
            0,
            Conditions::new().create_coin(alice.puzzle_hash, 0, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let cat = cat.child(alice.puzzle_hash, 0);
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

        let (issue_cat, _cat) =
            Cat::single_issuance_eve(ctx, alice.coin.coin_id(), 1, Conditions::new())?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        assert!(matches!(
            sim.spend_coins(ctx.take(), &[alice.sk]).unwrap_err(),
            SimulatorError::Validation(ErrorCode::AssertCoinAnnouncementFailed)
        ));

        Ok(())
    }

    #[test]
    fn test_exceeded_cat_issuance_output() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, _cat) = Cat::single_issuance_eve(
            ctx,
            alice.coin.coin_id(),
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 2, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        assert!(matches!(
            sim.spend_coins(ctx.take(), &[alice.sk]).unwrap_err(),
            SimulatorError::Validation(ErrorCode::AssertCoinAnnouncementFailed)
        ));

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

        let (issue_cat, cat) =
            Cat::single_issuance_eve(ctx, alice.coin.coin_id(), sum, conditions)?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let mut cats: Vec<Cat> = amounts
            .into_iter()
            .map(|amount| cat.child(alice.puzzle_hash, amount))
            .collect();

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

            Cat::spend_all(ctx, &cat_spends)?;
            sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

            // Update the cats to the children.
            cats = cats
                .into_iter()
                .map(|cat| cat.child(alice.puzzle_hash, cat.coin.amount))
                .collect();
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
        let (issue_cat, cat) = Cat::single_issuance_eve(
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
                cat.child(alice.puzzle_hash, 1),
                alice_p2.spend_with_conditions(
                    ctx,
                    Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
                )?,
            ),
            CatSpend::new(
                cat.child(custom_p2_puzzle_hash, 1),
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
        let (issue_cat, cat) =
            Cat::multi_issuance_eve(ctx, alice.coin.coin_id(), alice.pk, 10000, conditions)?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        let tail = ctx.curry(EverythingWithSignatureTailArgs::new(alice.pk))?;

        let cat_spend = CatSpend::with_extra_delta(
            cat.child(alice.puzzle_hash, 10000),
            alice_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(alice.puzzle_hash, 7000, memos)
                    .run_cat_tail(tail, NodePtr::NIL),
            )?,
            -3000,
        );

        Cat::spend_all(ctx, &[cat_spend])?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
