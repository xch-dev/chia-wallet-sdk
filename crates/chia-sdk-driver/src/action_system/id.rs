use chia_protocol::Bytes32;

/// Represents either XCH, an existing CAT or singleton, or a new CAT or singleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id {
    /// XCH does not have an asset id on-chain, so we need a special id for it.
    Xch,

    /// An id that already exists on the blockchain.
    Existing(Bytes32),

    /// A unique index for an asset that doesn't exist on the blockchain yet.
    New(usize),
}

/// A spendable singleton identity used to mint or own an NFT.
///
/// The contained [`Id`] resolves to an asset in the corresponding action-system
/// collection. The identity stored on-chain is the asset's singleton launcher id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NftIdentity {
    Did(Id),
    Nft(Id),
}

impl NftIdentity {
    pub fn id(self) -> Id {
        match self {
            Self::Did(id) | Self::Nft(id) => id,
        }
    }
}
