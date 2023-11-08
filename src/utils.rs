use clvm_traits::{FromClvm, Result};
use clvmr::{allocator::NodePtr, run_program, Allocator, ChiaDialect};

use crate::Condition;

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
) -> Result<Vec<Condition>> {
    let dialect = ChiaDialect::new(0);
    let output = run_program(allocator, &dialect, puzzle, solution, u64::MAX)?.1;
    Vec::<Condition>::from_clvm(allocator, output)
}
