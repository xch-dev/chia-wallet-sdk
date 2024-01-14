use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::standard::{StandardArgs, StandardSolution};
use clvm_traits::{clvm_quote, ToClvmError};
use clvm_utils::CurriedProgram;
use clvmr::{allocator::NodePtr, Allocator, FromNodePtr, ToNodePtr};

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
    .to_node_ptr(a)?;

    let solution = StandardSolution {
        original_public_key: None,
        delegated_puzzle: clvm_quote!(conditions),
        solution: (),
    }
    .to_node_ptr(a)?;

    let puzzle = Program::from_node_ptr(a, puzzle).unwrap();
    let solution = Program::from_node_ptr(a, solution).unwrap();

    Ok(CoinSpend::new(coin, puzzle, solution))
}
