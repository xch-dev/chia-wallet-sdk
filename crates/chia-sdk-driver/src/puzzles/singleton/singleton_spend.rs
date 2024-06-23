use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    singleton::{SingletonArgs, SingletonSolution},
    Proof,
};
use clvm_utils::CurriedProgram;

use crate::{Spend, SpendContext, SpendError};

pub fn spend_singleton(
    ctx: &mut SpendContext,
    coin: Coin,
    launcher_id: Bytes32,
    lineage_proof: Proof,
    inner_spend: Spend,
) -> Result<CoinSpend, SpendError> {
    let singleton_puzzle = ctx.singleton_top_layer()?;

    let puzzle_reveal = ctx.serialize(&CurriedProgram {
        program: singleton_puzzle,
        args: SingletonArgs::new(launcher_id, inner_spend.puzzle()),
    })?;

    let solution = ctx.serialize(&SingletonSolution {
        lineage_proof,
        amount: coin.amount,
        inner_solution: inner_spend.solution(),
    })?;

    Ok(CoinSpend::new(coin, puzzle_reveal, solution))
}
