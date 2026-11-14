/// A capability that peers advertise in their [`Handshake`](chia_protocol::Handshake).
///
/// Each capability is sent as a `(id, "1")` pair in [`Handshake::capabilities`], and a
/// feature that depends on a capability is only used when both peers advertise it. The ids
/// match the `Capability` enum in `chia-blockchain`.
///
/// `connect_peer` always advertises [`Capability::Base`],
/// [`Capability::BlockHeaders`] and [`Capability::RateLimitsV2`], and advertises
/// [`Capability::MempoolUpdates`] when [`PeerOptions::mempool_updates`](crate::PeerOptions::mempool_updates)
/// is enabled.
///
/// # Example
///
/// ```
/// use chia_sdk_client::Capability;
///
/// assert_eq!(u16::from(Capability::MempoolUpdates), 5);
/// ```
///
/// [`Handshake::capabilities`]: chia_protocol::Handshake::capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum Capability {
    /// Supports the Chia protocol as used on mainnet.
    Base = 1,
    /// Supports `RequestBlockHeaders`, a faster way to fetch header blocks.
    BlockHeaders = 2,
    /// Supports version 2 of the rate limits, such as [`V2_RATE_LIMITS`](crate::V2_RATE_LIMITS).
    RateLimitsV2 = 3,
    /// Can handle a `None` response instead of waiting for the full timeout. Peers no longer
    /// advertise this, but still support it.
    NoneResponse = 4,
    /// Opts in to `MempoolItemsAdded` and `MempoolItemsRemoved` messages for mempool
    /// transactions that match the wallet's subscriptions, as defined in
    /// [CHIP-26](https://github.com/Chia-Network/chips/blob/main/CHIPs/chip-0026.md).
    MempoolUpdates = 5,
    /// Supports the second hard fork.
    HardFork2 = 6,
    /// Supports version 3 of the rate limits, which this crate doesn't implement.
    RateLimitsV3 = 7,
}

impl From<Capability> for u16 {
    fn from(capability: Capability) -> Self {
        capability as u16
    }
}
