use chia_protocol::Bytes32;

/// Represents either an asset id for a CAT or a launcher id for a singleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id {
    /// An id that already exists on the blockchain.
    Existing(Bytes32),

    /// A unique index for an asset that doesn't exist on the blockchain yet.
    New(usize),
}
