use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    cat::{CatArgs, CatSolution, EverythingWithSignatureTailArgs, GenesisByCoinIdTailArgs},
    CoinProof, LineageProof,
};
use chia_sdk_types::{run_puzzle, Condition, Conditions, CreateCoin};
use clvm_traits::{clvm_quote, FromClvm};
use clvm_utils::CurriedProgram;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Primitive, Puzzle, Spend, SpendContext};

use super::{CatSpend, RawCatSpend};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub asset_id: Bytes32,
    pub p2_puzzle_hash: Bytes32,
}

impl Cat {
    pub fn new(
        coin: Coin,
        lineage_proof: Option<LineageProof>,
        asset_id: Bytes32,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            coin,
            lineage_proof,
            asset_id,
            p2_puzzle_hash,
        }
    }

    pub fn single_issuance_eve(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        amount: u64,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Cat), DriverError> {
        let genesis_by_coin_id_ptr = ctx.genesis_by_coin_id_tail_puzzle()?;

        let tail = ctx.alloc(&CurriedProgram {
            program: genesis_by_coin_id_ptr,
            args: GenesisByCoinIdTailArgs::new(parent_coin_id),
        })?;

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
        let everything_with_signature_ptr = ctx.everything_with_signature_tail_puzzle()?;

        let tail = ctx.alloc(&CurriedProgram {
            program: everything_with_signature_ptr,
            args: EverythingWithSignatureTailArgs::new(public_key),
        })?;

        Self::create_and_spend_eve(
            ctx,
            parent_coin_id,
            ctx.tree_hash(tail).into(),
            amount,
            extra_conditions.run_cat_tail(tail, NodePtr::NIL),
        )
    }

    /// Creates and spends an eve CAT with the provided conditions.
    /// To issue the CAT, you will need to reveal the TAIL puzzle and solution.
    /// This can be done with the [`RunCatTail`] condition.
    pub fn create_and_spend_eve(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        asset_id: Bytes32,
        amount: u64,
        conditions: Conditions,
    ) -> Result<(Conditions, Cat), DriverError> {
        let inner_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let eve_layer = CatLayer::new(asset_id, inner_puzzle);
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle).into();
        let puzzle_ptr = eve_layer.construct_puzzle(ctx)?;
        let puzzle_hash = ctx.tree_hash(puzzle_ptr).into();

        let eve = Cat::new(
            Coin::new(parent_coin_id, puzzle_hash, amount),
            None,
            asset_id,
            inner_puzzle_hash,
        );

        eve.spend(
            ctx,
            RawCatSpend {
                prev_coin_id: eve.coin.coin_id(),
                next_coin_proof: CoinProof {
                    parent_coin_info: parent_coin_id,
                    inner_puzzle_hash,
                    amount,
                },
                prev_subtotal: 0,
                extra_delta: 0,
                inner_spend: Spend::new(inner_puzzle, NodePtr::NIL),
            },
        )?;

        Ok((
            Conditions::new().create_coin(puzzle_hash, amount, Vec::new()),
            eve,
        ))
    }

    /// Creates coin spends for one or more CATs in a ring.
    /// Without the ring announcements, CAT spends cannot share inputs and outputs.
    ///
    /// Each item is a CAT and the inner spend for that CAT.
    pub fn spend_all(ctx: &mut SpendContext, cat_spends: &[CatSpend]) -> Result<(), DriverError> {
        let cat_puzzle_ptr = ctx.cat_puzzle()?;
        let len = cat_spends.len();

        let mut total_delta = 0;

        for (index, cat_spend) in cat_spends.iter().enumerate() {
            let CatSpend {
                cat,
                inner_spend,
                extra_delta,
            } = cat_spend;

            // Calculate the delta and add it to the subtotal.
            let output = ctx.run(inner_spend.puzzle, inner_spend.solution)?;
            let conditions: Vec<NodePtr> = ctx.extract(output)?;

            let create_coins = conditions
                .into_iter()
                .filter_map(|ptr| ctx.extract::<CreateCoin>(ptr).ok());

            let delta = create_coins.fold(
                i128::from(cat.coin.amount) - i128::from(*extra_delta),
                |delta, create_coin| delta - i128::from(create_coin.amount),
            );

            let prev_subtotal = total_delta;
            total_delta += delta;

            // Find information of neighboring coins on the ring.
            let prev = &cat_spends[if index == 0 { len - 1 } else { index - 1 }];
            let next = &cat_spends[if index == len - 1 { 0 } else { index + 1 }];

            let puzzle = ctx.alloc(&CurriedProgram {
                program: cat_puzzle_ptr,
                args: CatArgs::new(cat.asset_id, cat_spend.inner_spend.puzzle),
            })?;

            let solution = ctx.alloc(&CatSolution {
                inner_puzzle_solution: inner_spend.solution,
                lineage_proof: cat.lineage_proof,
                prev_coin_id: prev.cat.coin.coin_id(),
                this_coin_info: cat.coin,
                next_coin_proof: CoinProof {
                    parent_coin_info: next.cat.coin.parent_coin_info,
                    inner_puzzle_hash: ctx.tree_hash(inner_spend.puzzle).into(),
                    amount: next.cat.coin.amount,
                },
                prev_subtotal: prev_subtotal.try_into()?,
                extra_delta: *extra_delta,
            })?;

            ctx.spend(cat.coin, Spend::new(puzzle, solution))?;
        }

        Ok(())
    }

    /// Creates a coin spend for this CAT.
    pub fn spend(&self, ctx: &mut SpendContext, spend: RawCatSpend) -> Result<(), DriverError> {
        let cat_layer = CatLayer::new(self.asset_id, spend.inner_spend.puzzle);

        let puzzle = cat_layer.construct_puzzle(ctx)?;
        let solution = cat_layer.construct_solution(
            ctx,
            CatSolution {
                lineage_proof: self.lineage_proof,
                prev_coin_id: spend.prev_coin_id,
                this_coin_info: self.coin,
                next_coin_proof: spend.next_coin_proof,
                prev_subtotal: spend.prev_subtotal,
                extra_delta: spend.extra_delta,
                inner_puzzle_solution: spend.inner_spend.solution,
            },
        )?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))
    }

    /// Returns the lineage proof that would be used by each child.
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.p2_puzzle_hash,
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a wrapped spendable CAT for a given output.
    #[must_use]
    pub fn wrapped_child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        let puzzle_hash = CatArgs::curry_tree_hash(self.asset_id, p2_puzzle_hash.into());
        Self {
            coin: Coin::new(self.coin.coin_id(), puzzle_hash.into(), amount),
            lineage_proof: Some(self.child_lineage_proof()),
            asset_id: self.asset_id,
            p2_puzzle_hash,
        }
    }
}

impl Primitive for Cat {
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent_layer) = CatLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)? else {
            return Ok(None);
        };
        let parent_solution = CatLayer::<Puzzle>::parse_solution(allocator, parent_solution)?;

        let output = run_puzzle(
            allocator,
            parent_layer.inner_puzzle.ptr(),
            parent_solution.inner_puzzle_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let p2_puzzle_hash = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .find_map(|create_coin| {
                // This is an optimization to skip calculating the hash.
                if create_coin.amount != coin.amount {
                    return None;
                }

                // Calculate what the wrapped puzzle hash would be for the created coin.
                // This is because we're running the inner layer.
                let wrapped_puzzle_hash =
                    CatArgs::curry_tree_hash(parent_layer.asset_id, create_coin.puzzle_hash.into());

                // If the puzzle hash doesn't match the coin, this isn't the correct p2 puzzle hash.
                if wrapped_puzzle_hash != coin.puzzle_hash.into() {
                    return None;
                }

                // We've found the p2 puzzle hash of the coin we're looking for.
                Some(create_coin.puzzle_hash)
            });

        let Some(p2_puzzle_hash) = p2_puzzle_hash else {
            return Err(DriverError::MissingChild);
        };

        Ok(Some(Self {
            coin,
            lineage_proof: Some(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: parent_layer.inner_puzzle.curried_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            asset_id: parent_layer.asset_id,
            p2_puzzle_hash,
        }))
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzles::{cat::EverythingWithSignatureTailArgs, standard::StandardArgs};
    use chia_sdk_test::{test_secret_key, Simulator};
    use chia_sdk_types::MAINNET_CONSTANTS;

    use crate::StandardLayer;

    use super::*;

    #[test]
    fn test_single_issuance_cat() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let p2_puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(p2_puzzle_hash, 1);

        let conditions =
            Conditions::new().create_coin(p2_puzzle_hash, 1, vec![p2_puzzle_hash.into()]);
        let (issue_cat, _cat) = Cat::single_issuance_eve(ctx, coin.coin_id(), 1, conditions)?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;
        sim.spend_coins(ctx.take(), &[sk], &MAINNET_CONSTANTS)?;

        Ok(())
    }

    #[test]
    fn test_multi_issuance_cat() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let p2_puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(p2_puzzle_hash, 1);

        let conditions =
            Conditions::new().create_coin(p2_puzzle_hash, 1, vec![p2_puzzle_hash.into()]);
        let (issue_cat, _cat) = Cat::multi_issuance_eve(ctx, coin.coin_id(), pk, 1, conditions)?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        sim.spend_coins(ctx.take(), &[sk], &MAINNET_CONSTANTS)?;

        Ok(())
    }

    #[test]
    fn test_cat_spend_multi() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();
        let (sk, pk, p2, coin) = sim.new_p2(6)?;

        let (issue_cat, cat) = Cat::single_issuance_eve(
            ctx,
            coin.coin_id(),
            6,
            Conditions::new()
                .create_coin(p2, 1, vec![p2.into()])
                .create_coin(p2, 2, vec![p2.into()])
                .create_coin(p2, 3, vec![p2.into()]),
        )?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        let cat_spends = [
            CatSpend::new(
                cat.wrapped_child(p2, 1),
                StandardLayer::new(pk)
                    .spend(ctx, Conditions::new().create_coin(p2, 1, vec![p2.into()]))?,
            ),
            CatSpend::new(
                cat.wrapped_child(p2, 2),
                StandardLayer::new(pk)
                    .spend(ctx, Conditions::new().create_coin(p2, 2, vec![p2.into()]))?,
            ),
            CatSpend::new(
                cat.wrapped_child(p2, 3),
                StandardLayer::new(pk)
                    .spend(ctx, Conditions::new().create_coin(p2, 3, vec![p2.into()]))?,
            ),
        ];

        Cat::spend_all(ctx, &cat_spends)?;

        sim.spend_coins(ctx.take(), &[sk], &MAINNET_CONSTANTS)?;

        Ok(())
    }

    #[test]
    fn test_cat_spend() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let p2_puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(p2_puzzle_hash, 1);

        let conditions =
            Conditions::new().create_coin(p2_puzzle_hash, 1, vec![p2_puzzle_hash.into()]);
        let (issue_cat, cat) = Cat::single_issuance_eve(ctx, coin.coin_id(), 1, conditions)?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        let cat_spends = [CatSpend::new(
            cat.wrapped_child(p2_puzzle_hash, 1),
            StandardLayer::new(pk).spend(
                ctx,
                Conditions::new().create_coin(p2_puzzle_hash, 1, vec![p2_puzzle_hash.into()]),
            )?,
        )];

        Cat::spend_all(ctx, &cat_spends)?;

        sim.spend_coins(ctx.take(), &[sk], &MAINNET_CONSTANTS)?;

        Ok(())
    }

    #[test]
    fn test_cat_melt() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let p2_puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(p2_puzzle_hash, 10000);

        let conditions =
            Conditions::new().create_coin(p2_puzzle_hash, 10000, vec![p2_puzzle_hash.into()]);
        let (issue_cat, cat) = Cat::multi_issuance_eve(ctx, coin.coin_id(), pk, 10000, conditions)?;

        ctx.spend_p2_coin(coin, pk, issue_cat)?;

        let everything_with_signature_ptr = ctx.everything_with_signature_tail_puzzle()?;

        let tail = ctx.alloc(&CurriedProgram {
            program: everything_with_signature_ptr,
            args: EverythingWithSignatureTailArgs::new(pk),
        })?;

        let cat_spend = CatSpend::with_extra_delta(
            cat.wrapped_child(p2_puzzle_hash, 10000),
            StandardLayer::new(pk).spend(
                ctx,
                Conditions::new()
                    .create_coin(p2_puzzle_hash, 7000, vec![p2_puzzle_hash.into()])
                    .run_cat_tail(tail, NodePtr::NIL),
            )?,
            -3000,
        );

        Cat::spend_all(ctx, &[cat_spend])?;

        sim.spend_coins(ctx.take(), &[sk], &MAINNET_CONSTANTS)?;

        Ok(())
    }
}
