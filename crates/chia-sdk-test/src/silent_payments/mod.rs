//! Test-side helpers for CHIP-0057 silent payments.
//!
//! These helpers are NOT production code — they exist so that the simulator
//! and binding test suites can construct `TweakData` from on-chain state
//! without going through a future transport protocol (not currently supported).
//!
//! Convenience re-exports of the wallet-side address/key types from
//! `chia-sdk-utils::silent_payments` so that downstream simulator round-trip
//! tests can reach `SilentPaymentAddress` and `SilentPaymentKeys` through one
//! import path.

mod tweak_data;

pub use chia_sdk_utils::silent_payments::{
    LabelRegistry, SilentPaymentAddress, SilentPaymentKeys, SilentPaymentNetwork,
};
pub use tweak_data::tweak_data_from_simulator_block;
