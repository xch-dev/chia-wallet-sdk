use chia_protocol::{Bytes32, Coin};
use chia_wallet::Proof;

#[derive(Debug, Clone)]
pub struct NftInfo<M> {
    pub launcher_id: Bytes32,
    pub coin: Coin,
    pub proof: Proof,
    pub metadata: M,
    pub metadata_updater_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
}
