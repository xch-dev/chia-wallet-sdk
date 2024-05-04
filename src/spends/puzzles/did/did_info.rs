use chia_protocol::{Bytes32, Coin};
use chia_wallet::Proof;

#[derive(Debug, Clone)]
pub struct DidInfo<T> {
    pub launcher_id: Bytes32,
    pub coin: Coin,
    pub proof: Proof,
    pub recovery_did_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: T,
}
