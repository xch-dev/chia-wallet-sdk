//! Limits shared by the sending and the scanning side of CHIP-0057.

/// `K_max`: the maximum number of silent-payment outputs for a single scan key
/// in one spend group (CHIP-0057 "Kmax: Maximum Outputs Per Spend Group").
///
/// The limit is shared by both sides, so that no valid payment lies beyond the
/// point where a scanner stops: a sender must not create more than `K_MAX`
/// outputs for one scan key in one spend group, and a scanner detects outputs
/// at every index below `K_MAX` and stops iterating a spend group when `k`
/// reaches it.
///
/// The value is derived from the mempool's spend bundle cost limit: about
/// 2,409 `CREATE_COIN` outputs fit in a single spend bundle.
pub const K_MAX: u32 = 2400;
