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

/// A singleton that's spent in the same transaction to mint an NFT, or to have an NFT assigned to
/// it. The on-chain owner of an NFT is the launcher id of this singleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NftIdentity {
    /// The id of a DID in the [`Spends`](crate::Spends).
    Did(Id),
    /// The id of an NFT in the [`Spends`](crate::Spends).
    Nft(Id),
}

impl NftIdentity {
    pub fn id(self) -> Id {
        match self {
            Self::Did(id) | Self::Nft(id) => id,
        }
    }
}
