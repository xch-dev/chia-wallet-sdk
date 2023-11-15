use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::standard::{StandardArgs, StandardSolution};
use clvm_traits::{clvm_quote, ToClvmError, ToPtr};
use clvm_utils::CurriedProgram;
use clvmr::{allocator::NodePtr, serde::node_to_bytes, Allocator};

use crate::Condition;

pub fn spend_standard_coin(
    a: &mut Allocator,
    standard_puzzle_ptr: NodePtr,
    coin: Coin,
    synthetic_key: PublicKey,
    conditions: &[Condition<NodePtr>],
) -> Result<CoinSpend, ToClvmError> {
    let puzzle = CurriedProgram {
        program: standard_puzzle_ptr,
        args: StandardArgs { synthetic_key },
    }
    .to_ptr(a)?;

    let solution = StandardSolution {
        original_public_key: None,
        delegated_puzzle: clvm_quote!(conditions),
        solution: (),
    }
    .to_ptr(a)?;

    let puzzle_bytes = node_to_bytes(a, puzzle).unwrap();
    let solution_bytes = node_to_bytes(a, solution).unwrap();

    let puzzle = Program::new(puzzle_bytes.into());
    let solution = Program::new(solution_bytes.into());
    Ok(CoinSpend::new(coin, puzzle, solution))
}
