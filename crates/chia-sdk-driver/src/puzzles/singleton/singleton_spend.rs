use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    singleton::{
        SingletonArgs, SingletonSolution, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    Proof,
};
use clvm_utils::CurriedProgram;

use crate::{spend_builder::InnerSpend, SpendContext, SpendError};

pub fn spend_singleton(
    ctx: &mut SpendContext,
    coin: Coin,
    launcher_id: Bytes32,
    lineage_proof: Proof,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError> {
    let singleton_puzzle = ctx.singleton_top_layer()?;

    let puzzle_reveal = ctx.serialize(CurriedProgram {
        program: singleton_puzzle,
        args: SingletonArgs {
            singleton_struct: SingletonStruct {
                mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                launcher_id,
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            },
            inner_puzzle: inner_spend.puzzle(),
        },
    })?;

    let solution = ctx.serialize(SingletonSolution {
        lineage_proof,
        amount: coin.amount,
        inner_solution: inner_spend.solution(),
    })?;

    Ok(CoinSpend::new(coin, puzzle_reveal, solution))
}
