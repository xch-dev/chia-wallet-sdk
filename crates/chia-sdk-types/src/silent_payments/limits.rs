//! Limits shared by senders and scanners.

/// The most outputs for one scan key in one spend group (CHIP-0057 "Kmax: Maximum Outputs Per
/// Spend Group"). A sender creates no more, and a scanner checks every index below it.
pub const K_MAX: u32 = 2400;
