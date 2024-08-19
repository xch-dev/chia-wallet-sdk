use chia_protocol::Coin;
use chia_puzzles::Proof;

use super::{DataStoreInfo, DataStoreMetadata};

/// Everything that is required to spend a ``DataStore`` coin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStore<M = DataStoreMetadata> {
    /// The coin that holds this ``DataStore``.
    pub coin: Coin,
    /// The lineage proof for the singletonlayer.
    pub proof: Proof,
    /// The info associated with the ``DataStore``, including the metadata.
    pub info: DataStoreInfo<M>,
}
