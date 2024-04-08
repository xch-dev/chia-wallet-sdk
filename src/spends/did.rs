use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::{did::DidSolution, singleton::SingletonSolution, Proof};
use clvm_traits::ToClvm;
use clvmr::NodePtr;

use crate::{standard_solution, SpendContext, SpendError};

/// Spend a standard DID coin (a DID singleton with the standard transaction inner puzzle).
pub fn spend_did<T>(
    ctx: &mut SpendContext,
    coin: Coin,
    puzzle_reveal: Program,
    proof: Proof,
    conditions: T,
) -> Result<CoinSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    let p2_solution = standard_solution(conditions);
    let did_solution = DidSolution::InnerSpend(p2_solution);

    let solution = ctx.serialize(SingletonSolution {
        proof,
        amount: coin.amount,
        inner_solution: did_solution,
    })?;

    Ok(CoinSpend::new(coin, puzzle_reveal, solution))
}
