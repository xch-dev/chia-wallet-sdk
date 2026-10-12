use chia_protocol::Bytes32;

/// Identifies the asset that an [`Action`](crate::Action) applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id {
    /// XCH, which doesn't have an asset id on-chain.
    Xch,

    /// The asset id of a CAT, or the launcher id of a singleton, that was added to the
    /// [`Spends`](crate::Spends).
    Existing(Bytes32),

    /// An asset created in the same transaction, by the action at this index in the list passed to
    /// [`Spends::apply`](crate::Spends::apply).
    New(usize),
}
