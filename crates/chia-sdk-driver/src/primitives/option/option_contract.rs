use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    singleton::{LauncherSolution, SingletonArgs, SingletonSolution},
    LineageProof, Proof,
};
use chia_sdk_types::{
    puzzles::{OptionContractArgs, OptionContractSolution},
    run_puzzle, Condition, Conditions, Mod,
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext, SpendWithConditions};

use super::{OptionContractLayers, OptionInfo, OptionMetadata};

#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct OptionContract {
    pub coin: Coin,
    pub proof: Proof,
    pub info: OptionInfo,
}

impl OptionContract {
    pub fn new(coin: Coin, proof: Proof, info: OptionInfo) -> Self {
        Self { coin, proof, info }
    }

    pub fn parse_child(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<(Self, Puzzle)>, DriverError> {
        let Some(singleton) =
            OptionContractLayers::<Puzzle>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let solution = OptionContractLayers::<Puzzle>::parse_solution(allocator, parent_solution)?;
        let output = run_puzzle(
            allocator,
            singleton.inner_puzzle.inner_puzzle.ptr(),
            solution.inner_solution.inner_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let Some(create_coin) = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .find(|cond| cond.amount % 2 == 1)
        else {
            return Err(DriverError::MissingChild);
        };

        let puzzle_hash = SingletonArgs::curry_tree_hash(
            singleton.launcher_id,
            OptionContractArgs::new(
                singleton.inner_puzzle.underlying_coin_id,
                singleton.inner_puzzle.underlying_delegated_puzzle_hash,
                TreeHash::from(create_coin.puzzle_hash),
            )
            .curry_tree_hash(),
        );

        let option = Self {
            coin: Coin::new(
                parent_coin.coin_id(),
                puzzle_hash.into(),
                create_coin.amount,
            ),
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: singleton.inner_puzzle.tree_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info: OptionInfo {
                launcher_id: singleton.launcher_id,
                underlying_coin_id: singleton.inner_puzzle.underlying_coin_id,
                underlying_delegated_puzzle_hash: singleton
                    .inner_puzzle
                    .underlying_delegated_puzzle_hash,
                p2_puzzle_hash: create_coin.puzzle_hash,
            },
        };

        Ok(Some((option, singleton.inner_puzzle.inner_puzzle)))
    }

    pub fn parse_metadata(
        allocator: &mut Allocator,
        launcher_solution: NodePtr,
    ) -> Result<OptionMetadata, DriverError> {
        let solution = LauncherSolution::<OptionMetadata>::from_clvm(allocator, launcher_solution)?;
        Ok(solution.key_value_list)
    }

    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    pub fn spend(&self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<(), DriverError> {
        let layers = self.info.into_layers(inner_spend.puzzle);

        let puzzle = layers.construct_puzzle(ctx)?;
        let solution = layers.construct_solution(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: OptionContractSolution::new(inner_spend.solution),
            },
        )?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))?;

        Ok(())
    }

    pub fn spend_with<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let inner_spend = inner.spend_with_conditions(ctx, conditions)?;
        self.spend(ctx, inner_spend)
    }

    pub fn transfer<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<Self, DriverError>
    where
        I: SpendWithConditions,
    {
        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions.create_coin(p2_puzzle_hash, self.coin.amount, Some(memos)),
        )?;

        Ok(self.wrapped_child(p2_puzzle_hash))
    }

    pub fn exercise<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        extra_conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let data = ctx.alloc(&self.info.underlying_coin_id)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions
                .send_message(
                    23,
                    self.info.underlying_delegated_puzzle_hash.into(),
                    vec![data],
                )
                .melt_singleton(),
        )?;

        Ok(())
    }

    pub fn wrapped_child(&self, p2_puzzle_hash: Bytes32) -> Self {
        let info = self.info.with_p2_puzzle_hash(p2_puzzle_hash);

        let inner_puzzle_hash = info.inner_puzzle_hash();

        Self {
            coin: Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id, inner_puzzle_hash).into(),
                self.coin.amount,
            ),
            proof: Proof::Lineage(self.child_lineage_proof()),
            info,
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::{expect_spend, Simulator};
    use rstest::rstest;

    use crate::{OptionLauncher, StandardLayer};

    use super::*;

    #[test]
    fn test_transfer_option() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let parent_coin = sim.new_coin(alice.puzzle_hash, 1);

        let launcher = OptionLauncher::new(
            ctx,
            alice.coin.coin_id(),
            alice.puzzle_hash,
            alice.puzzle_hash,
            10,
        )?;

        let (lock, launcher) = launcher.lock_underlying(parent_coin.coin_id(), None, 1);
        alice_p2.spend(ctx, parent_coin, lock)?;

        let (mint_option, mut option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        for _ in 0..5 {
            option = option.transfer(ctx, &alice_p2, alice.puzzle_hash, Conditions::new())?;
        }

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[rstest]
    fn test_clawback_option(#[values(true, false)] expired: bool) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        if expired {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let parent_coin = sim.new_coin(alice.puzzle_hash, 1);

        let launcher = OptionLauncher::new(
            ctx,
            alice.coin.coin_id(),
            alice.puzzle_hash,
            alice.puzzle_hash,
            10,
        )?;

        let (lock, launcher) = launcher.lock_underlying(parent_coin.coin_id(), None, 1);
        alice_p2.spend(ctx, parent_coin, lock)?;

        let underlying_coin = launcher.underlying_coin();
        let underlying = launcher.underlying();

        let (mint_option, _option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let clawback_spend =
            alice_p2.spend_with_conditions(ctx, Conditions::new().reserve_fee(1))?;
        underlying.clawback_coin_spend(ctx, underlying_coin, clawback_spend)?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), expired);

        Ok(())
    }

    #[rstest]
    fn test_exercise_option(#[values(true, false)] expired: bool) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        if expired {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let parent_coin = sim.new_coin(alice.puzzle_hash, 1);

        let launcher = OptionLauncher::new(
            ctx,
            alice.coin.coin_id(),
            alice.puzzle_hash,
            alice.puzzle_hash,
            10,
        )?;

        let (lock, launcher) = launcher.lock_underlying(parent_coin.coin_id(), None, 1);
        alice_p2.spend(ctx, parent_coin, lock)?;

        let underlying_coin = launcher.underlying_coin();
        let underlying = launcher.underlying();

        let (mint_option, option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        option.exercise(ctx, &alice_p2, Conditions::new())?;
        underlying.exercise_coin_spend(
            ctx,
            underlying_coin,
            option.info.inner_puzzle_hash().into(),
            option.coin.amount,
        )?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !expired);

        Ok(())
    }
}
