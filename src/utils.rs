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
