use std::io;

use chia_client::Peer;
use chia_protocol::{Coin, RejectPuzzleSolution, RequestPuzzleSolution, RespondPuzzleSolution};
use clvm_traits::{FromClvm, FromClvmError};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{
    allocator::NodePtr, reduction::EvalErr, run_program, serde::node_from_bytes, Allocator,
    ChiaDialect, FromNodePtr,
};
use thiserror::Error;

use crate::Condition;

#[derive(Error, Debug)]
pub enum EvaluateConditionsError {
    #[error("{0}")]
    Eval(#[from] EvalErr),

    #[error("{0}")]
    Clvm(#[from] FromClvmError),
}

#[derive(Error, Debug)]
pub enum RequestPuzzleError {
    #[error("peer error: {0}")]
    Peer(#[from] chia_client::Error<RejectPuzzleSolution>),

    #[error("clvm error: {0}")]
    Clvm(#[from] FromClvmError),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("wrong mod hash")]
    WrongModHash([u8; 32]),
}

pub fn u64_to_bytes(amount: u64) -> Vec<u8> {
    let bytes: Vec<u8> = amount.to_be_bytes().into();
    let mut slice = bytes.as_slice();

    // Remove leading zeros.
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }

    slice.into()
}

pub fn evaluate_conditions(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<Vec<Condition<NodePtr>>, EvaluateConditionsError> {
    let dialect = ChiaDialect::new(0);
    let output = run_program(allocator, &dialect, puzzle, solution, u64::MAX)?.1;
    Ok(Vec::<Condition<NodePtr>>::from_node_ptr(allocator, output)?)
}

pub async fn request_puzzle_args<T>(
    a: &mut Allocator,
    peer: &Peer,
    coin: &Coin,
    expected_mod_hash: [u8; 32],
    height: u32,
) -> Result<T, RequestPuzzleError>
where
    T: FromClvm<NodePtr>,
{
    let response: RespondPuzzleSolution = peer
        .request(RequestPuzzleSolution::new(coin.parent_coin_info, height))
        .await
        .unwrap();

    let response = response.response;

    let ptr = node_from_bytes(a, response.puzzle.as_slice())?;
    let puzzle: CurriedProgram<NodePtr, T> = FromClvm::from_clvm(a, ptr)?;

    let mod_hash = tree_hash(a, puzzle.program);
    if mod_hash != expected_mod_hash {
        return Err(RequestPuzzleError::WrongModHash(mod_hash));
    }

    Ok(puzzle.args)
}
