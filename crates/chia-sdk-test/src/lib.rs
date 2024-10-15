mod announcements;
mod error;
mod events;
mod keys;
mod peer_simulator;
mod simulator;
mod transaction;

pub use announcements::*;
pub use error::*;
pub use events::*;
pub use keys::*;
pub use peer_simulator::*;
pub use simulator::*;
pub use transaction::*;

use chia_protocol::{Bytes32, Program};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::Allocator;

pub fn to_program(value: impl ToClvm<Allocator>) -> anyhow::Result<Program> {
    let mut allocator = Allocator::new();
    let ptr = value.to_clvm(&mut allocator)?;
    Ok(Program::from_clvm(&allocator, ptr)?)
}

pub fn to_puzzle(value: impl ToClvm<Allocator>) -> anyhow::Result<(Bytes32, Program)> {
    let mut allocator = Allocator::new();
    let ptr = value.to_clvm(&mut allocator)?;
    let puzzle_reveal = Program::from_clvm(&allocator, ptr)?;
    let puzzle_hash = tree_hash(&allocator, ptr);
    Ok((puzzle_hash.into(), puzzle_reveal))
}
