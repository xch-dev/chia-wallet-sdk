/// How a multi-coin `Spends` bundle binds its inputs together so that no
/// single coin spend can be replayed independently of its sibling spends.
///
/// The binding mechanism is also load-bearing for CHIP-0057 silent-payment
/// scanner detection: scanners reconstruct multi-input SP spend groups by
/// computing strongly connected components over the directed graph of
/// `ASSERT_CONCURRENT_SPEND` (opcode 64) references in each removal's solution.
/// The SP-side enforcement lives in the `sp_finish_branch` helper in
/// `action_system/spends.rs` (reached via `Spends::finish_with_keys`); the
/// CHIP-0057 spec §"Scanner Grouping Strategies" covers the receiver side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relation {
    /// No cross-coin binding emitted. Each coin's spend stands alone (signature
    /// aggregation still prevents partial-bundle replay at the BLS layer).
    /// Suitable for single-coin bundles and for multi-coin bundles that don't
    /// require atomic-replay protection beyond the signature aggregate.
    None,

    /// Emit a closed cycle of `ASSERT_CONCURRENT_SPEND` (opcode 64) conditions
    /// across every non-ephemeral coin spend in the bundle: coin 0 asserts
    /// coin N-1's `coin_id`, and coin i (for i ≥ 1) asserts coin i-1's `coin_id`.
    /// This forms a single strongly connected component spanning all bound
    /// coins, defending against third-party "pollution" assertions that point
    /// at a victim's coin (the polluter's coin sits in its own trivial SCC).
    ///
    /// **Load-bearing for CHIP-0057 multi-input silent payment detection.**
    /// Wallets sending to a silent-payment destination across 2+ XCH inputs MUST
    /// pass this variant to `Spends::finish_with_keys`; the call returns
    /// `Err(DriverError::SilentPaymentRequiresInputBinding)` otherwise.
    /// Refactoring this emission shape away from the closed cycle
    /// will silently break SP scanner detection for cross-derivation-index
    /// multi-input sends — the pinning test in `action_system/spends.rs`
    /// guards against accidental drift.
    AssertConcurrent,
}
