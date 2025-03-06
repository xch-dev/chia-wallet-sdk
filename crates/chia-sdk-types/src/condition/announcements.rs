use chia_protocol::Bytes32;
use chia_sha2::Sha256;

/// Creates announcement ids compatible with [`AssertCoinAnnouncement`](crate::conditions::AssertCoinAnnouncement)
/// and [`AssertPuzzleAnnouncement`](crate::conditions::AssertPuzzleAnnouncement).
pub fn announcement_id(coin_info: Bytes32, message: impl AsRef<[u8]>) -> Bytes32 {
    let mut hasher = Sha256::new();
    hasher.update(coin_info.as_ref());
    hasher.update(message.as_ref());
    Bytes32::from(hasher.finalize())
}
