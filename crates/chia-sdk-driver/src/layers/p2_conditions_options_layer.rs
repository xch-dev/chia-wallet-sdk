use chia_protocol::Coin;
use chia_sdk_types::{Conditions, Mod};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The p2 conditions options [`Layer`] allows a predetermined set of conditions to be chosen at spend time.
/// To do so, a list of conditions lists are provided when the coin are created. Then, one of the conditions
/// is selected when the coin is spent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2ConditionsOptionsLayer<T = NodePtr> {
    pub options: Vec<Conditions<T>>,
}

impl<T> P2ConditionsOptionsLayer<T> {
    pub fn new(options: Vec<Conditions<T>>) -> Self {
        Self { options }
    }

    pub fn spend(&self, ctx: &mut SpendContext, coin: Coin, option: u16) -> Result<(), DriverError>
    where
        T: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let spend = self.inner_spend(ctx, option)?;
        ctx.spend(coin, spend)
    }

    pub fn inner_spend(&self, ctx: &mut SpendContext, option: u16) -> Result<Spend, DriverError>
    where
        T: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, P2ConditionsOptionsSolution { option })?;
        Ok(Spend { puzzle, solution })
    }
}

impl<T> Layer for P2ConditionsOptionsLayer<T>
where
    T: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    type Solution = P2ConditionsOptionsSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_CONDITIONS_OPTIONS_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2ConditionsOptionsArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            options: args.options,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2ConditionsOptionsSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2ConditionsOptionsArgs {
            options: self.options.clone(),
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::Simulator;
    use chia_sdk_types::run_puzzle;
    use rstest::rstest;

    use crate::StandardLayer;

    use super::*;

    #[test]
    fn test_conditions_options_layer() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let layer = P2ConditionsOptionsLayer::new(vec![
            Conditions::new().remark(NodePtr::NIL),
            Conditions::new().create_coin_announcement(b"hello".to_vec().into()),
        ]);

        let ptr = layer.construct_puzzle(&mut ctx)?;
        let puzzle = Puzzle::parse(&ctx.allocator, ptr);
        let roundtrip = P2ConditionsOptionsLayer::<NodePtr>::parse_puzzle(&ctx.allocator, puzzle)?
            .expect("invalid P2 conditions options layer");

        assert_eq!(roundtrip.options, layer.options);

        Ok(())
    }

    #[rstest]
    fn test_conditions_options_spend(#[values(0, 1)] option: u16) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();
        let (sk, pk, _puzzle_hash, coin) = sim.new_p2(1)?;
        let p2 = StandardLayer::new(pk);

        let conditions0 = Conditions::new().create_coin_announcement(b"hello".to_vec().into());
        let conditions1 = Conditions::new().create_coin_announcement(b"goodbye".to_vec().into());
        let layer = P2ConditionsOptionsLayer::new(vec![conditions0.clone(), conditions1.clone()]);

        let ptr = layer.construct_puzzle(ctx)?;
        let p2_puzzle_hash = ctx.tree_hash(ptr);

        p2.spend(
            ctx,
            coin,
            Conditions::new().create_coin(p2_puzzle_hash.into(), 1, None),
        )?;

        let option_coin = Coin::new(coin.coin_id(), p2_puzzle_hash.into(), 1);
        layer.spend(ctx, option_coin, option)?;
        sim.spend_coins(ctx.take(), &[sk])?;

        let solution = layer.construct_solution(ctx, P2ConditionsOptionsSolution { option })?;

        let output = run_puzzle(&mut ctx.allocator, ptr, solution)?;
        let output = ctx.extract::<Conditions>(output)?;

        if option == 0 {
            assert_eq!(output, conditions0);
        } else {
            assert_eq!(output, conditions1);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2ConditionsOptionsArgs<T = NodePtr> {
    pub options: Vec<Conditions<T>>,
}

impl<T> P2ConditionsOptionsArgs<T> {
    pub fn new(options: Vec<Conditions<T>>) -> Self {
        Self { options }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct P2ConditionsOptionsSolution {
    pub option: u16,
}

impl<T> Mod for P2ConditionsOptionsArgs<T> {
    const MOD_REVEAL: &[u8] = &P2_CONDITIONS_OPTIONS_PUZZLE;
    const MOD_HASH: TreeHash = P2_CONDITIONS_OPTIONS_PUZZLE_HASH;
}

/*
; this puzzle takes a list of conditions lists and lets the spender select which one to use
(mod (CONDITIONS_OPTIONS option_index)
    ; helper to pick an item from a list
    (defun select (items idx)
        (if items
            (if idx
                (select (r items) (- idx 1)) ; continue to get the selected index
                (f items) ; idx 0 returns first element
            )
            (x) ; no items to choose from
        )
    )

    ; entry point
    (select CONDITIONS_OPTIONS option_index)
)
*/

pub const P2_CONDITIONS_OPTIONS_PUZZLE: [u8; 111] = hex!(
    "
    ff02ffff01ff02ff02ffff04ff02ffff04ff05ffff04ff0bff8080808080ffff
    04ffff01ff02ffff03ff05ffff01ff02ffff03ff0bffff01ff02ff02ffff04ff
    02ffff04ff0dffff04ffff11ff0bffff010180ff8080808080ffff010980ff01
    80ffff01ff088080ff0180ff018080
    "
);

pub const P2_CONDITIONS_OPTIONS_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "e82e42b272a903ddd9c279d291487655e4e08883829dbb8086af91bd9b8afc3e"
));
