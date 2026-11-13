//! Construction-backed verification and aggregate-context signing for Internal Custody.
//!
//! Typed requests reconstruct supported prefarm operations before executing them
//! with a shared cost budget. Verified effects and signing capabilities bind review
//! facts and participant signatures to those exact spends and the chosen network.

mod effects;
mod execution;
mod operations;
mod reconstruction;

pub use effects::*;
pub use operations::*;

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod tests;
