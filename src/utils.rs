use std::io;

use chia_client::Peer;
use chia_protocol::{Coin, RejectPuzzleSolution};
use clvm_traits::{FromClvm, FromClvmError};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};
use thiserror::Error;

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
    let puzzle = peer
        .request_puzzle_and_solution(coin.parent_coin_info, height)
        .await?
        .puzzle;

    let ptr = node_from_bytes(a, puzzle.as_slice())?;
    let puzzle: CurriedProgram<NodePtr, T> = FromClvm::from_clvm(a, ptr)?;

    let mod_hash = tree_hash(a, puzzle.program);
    if mod_hash != expected_mod_hash {
        return Err(RequestPuzzleError::WrongModHash(mod_hash));
    }

    Ok(puzzle.args)
}
