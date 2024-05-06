use chia_protocol::{Bytes32, Coin};
use chia_wallet::Proof;

#[derive(Debug, Clone)]
pub struct DidInfo<M> {
    pub launcher_id: Bytes32,
    pub coin: Coin,
    pub did_inner_puzzle_hash: Bytes32,
    pub owner_puzzle_hash: Bytes32,
    pub proof: Proof,
    pub recovery_did_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
}
