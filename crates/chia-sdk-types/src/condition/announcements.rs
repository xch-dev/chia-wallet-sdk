use chia_protocol::Bytes32;
use chia_sha2::Sha256;

/// Computes the id of an announcement, which is the SHA-256 hash of `coin_info` followed by
/// `message`.
///
/// For a [`CreateCoinAnnouncement`](crate::conditions::CreateCoinAnnouncement), `coin_info` is the
/// announcing coin's id, and the result can be asserted with
/// [`AssertCoinAnnouncement`](crate::conditions::AssertCoinAnnouncement). For a
/// [`CreatePuzzleAnnouncement`](crate::conditions::CreatePuzzleAnnouncement), `coin_info` is the
/// announcing coin's puzzle hash, and the result can be asserted with
/// [`AssertPuzzleAnnouncement`](crate::conditions::AssertPuzzleAnnouncement).
///
/// ## Example
///
/// ```rust
/// # use chia_protocol::{Bytes32, Coin};
/// # use chia_sdk_types::{Condition, announcement_id};
/// let coin = Coin::new(Bytes32::new([1; 32]), Bytes32::new([2; 32]), 1000);
///
/// // The coin creates an announcement in its spend.
/// let announce: Condition = Condition::create_coin_announcement(b"hello".to_vec().into());
///
/// // Another spend in the same block asserts it.
/// let assert: Condition =
///     Condition::assert_coin_announcement(announcement_id(coin.coin_id(), b"hello"));
/// # let _ = (announce, assert);
/// ```
pub fn announcement_id(coin_info: Bytes32, message: impl AsRef<[u8]>) -> Bytes32 {
    let mut hasher = Sha256::new();
    hasher.update(coin_info.as_ref());
    hasher.update(message.as_ref());
    Bytes32::from(hasher.finalize())
}
