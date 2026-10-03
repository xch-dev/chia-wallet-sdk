//! Wire-protocol types for the silent-payment scanner.
//!
//! These three structs are the transport-agnostic surface a wallet sees:
//!
//! - [`TweakData`] is the scanner's input — pre-computed per-spend-group tweak
//!   points + candidate output metadata. Constructed by an indexer adapter (a
//!   a future transport protocol, the simulator helper, etc.) but
//!   carries no transport fields.
//! - [`OutputMeta`] is the metadata for one candidate coin — the puzzle hash
//!   the scanner matches against, plus the bookkeeping fields the wallet needs
//!   to act on a detected coin without re-parsing the block.
//! - [`DetectedSpCoin`] is the scanner's output — one entry per detected coin,
//!   carrying the coin and the tweak from which the holder of the spend secret
//!   key derives the one-time key.

use std::fmt;

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::silent_payments::ScalarField;
use clvmr::NodePtr;

use super::protocol::derive_onetime_sk;

/// Transport-agnostic input to the silent-payment scanner.
///
/// `tweak_points` are the pre-computed per-spend-group ECDH multipliers
/// `tweak_point[i] = input_hash[i] * A_sum[i]` (the indexer or sender computes
/// these on the send/index side; the scanner consumes them as-is).
/// `outputs` are the candidate coin metadata to scan against — typically all
/// outputs in a single block, but the primitive does not care about block
/// boundaries.
///
/// No transport fields: no `height`, no `block_hash`, no JSON envelope. A
/// future transport client constructs `TweakData` from its wire
/// messages without breaking this struct's shape.
#[derive(Clone, Debug)]
pub struct TweakData {
    pub tweak_points: Vec<PublicKey>,
    pub outputs: Vec<OutputMeta>,
}

/// Coin metadata the scanner needs to identify and report a detected
/// silent-payment coin.
///
/// `puzzle_hash` is the field the scanner matches against; the remaining fields
/// flow through to the returned [`DetectedSpCoin`] so the wallet can act on the
/// coin without re-parsing the block.
#[derive(Clone, Copy, Debug)]
pub struct OutputMeta {
    pub puzzle_hash: Bytes32,
    pub coin_id: Bytes32,
    pub amount: u64,
    pub parent_coin_id: Bytes32,
}

/// A silent-payment coin detected by `scan_from_tweaks`.
///
/// A detection is produced from the scan secret key and the spend public key
/// alone, so it does not contain a spendable key. It carries the output coin,
/// the index `k`, the label (if any), and the combined tweak that the holder of
/// the spend secret key needs to derive the one-time key (CHIP-0057,
/// "Spending"). Use [`DetectedSpCoin::onetime_sk`] for that step.
#[derive(Clone)]
pub struct DetectedSpCoin {
    pub coin_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub parent_coin_id: Bytes32,
    /// The `k` counter at which this output was detected within its spend group.
    pub k: u32,
    /// `None` for unlabeled detections; `Some(m)` for label index `m`.
    pub label: Option<u32>,
    /// The combined tweak: `t_k` for an unlabeled detection, and
    /// `(t_k + label_scalar) mod r` for a labeled one. The one-time secret key
    /// is `(b_spend + tweak) mod r`.
    ///
    /// The tweak cannot spend the coin without the spend secret key, but
    /// together with the address it links the coin to the recipient, so it must
    /// be kept private.
    pub tweak: ScalarField,
}

/// The tweak is not printed: together with the recipient's address it links
/// the coin to the recipient.
impl fmt::Debug for DetectedSpCoin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DetectedSpCoin")
            .field("coin_id", &self.coin_id)
            .field("puzzle_hash", &self.puzzle_hash)
            .field("amount", &self.amount)
            .field("parent_coin_id", &self.parent_coin_id)
            .field("k", &self.k)
            .field("label", &self.label)
            .field("tweak", &"<redacted>")
            .finish()
    }
}

impl DetectedSpCoin {
    /// The detected output coin.
    #[must_use]
    pub fn coin(&self) -> Coin {
        Coin::new(self.parent_coin_id, self.puzzle_hash, self.amount)
    }

    /// Derive the one-time secret key of the detected coin from the recipient's
    /// spend secret key: `(b_spend + tweak) mod r`.
    ///
    /// This is the only step of receiving a silent payment that needs the spend
    /// secret key. The coin is locked to the standard puzzle, so it is spent
    /// with the synthetic key of the result (`derive_synthetic()`).
    #[must_use]
    pub fn onetime_sk(&self, spend_sk: &SecretKey) -> SecretKey {
        derive_onetime_sk(spend_sk, &self.tweak)
    }
}

/// Per-output deterministic state recorded at apply time, consumed at finish
/// time by the chip-0057 SP branch of [`crate::Spends::finish_with_keys`] to
/// compute the recipient's one-time puzzle hash and emit the on-chain
/// `CreateCoin`.
///
/// The struct is `pub(crate)` — external callers never construct it directly;
/// they go through [`crate::Action::silent_payment_send`].
#[derive(Debug, Clone)]
pub(crate) struct SilentPaymentPending {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    pub parent_xch_index: usize,
    pub parent_coin: Coin,
    pub k: u32,
    pub amount: u64,
    pub memos: Memos<NodePtr>,
}

#[cfg(test)]
mod tests {
    use chia_bls::PublicKey;

    /// Defensive test: malformed 48-byte pubkey bytes are rejected at
    /// `PublicKey::from_bytes`, not inside the scanner. Documents the
    /// deserialization boundary so a future change to scanner-internal
    /// pubkey parsing doesn't silently swallow malformed bytes.
    #[test]
    fn malformed_pubkey_caught_at_deserialization() {
        let result = PublicKey::from_bytes(&[0xff; 48]);
        assert!(
            result.is_err(),
            "PublicKey::from_bytes(&[0xff; 48]) must reject"
        );
    }

    /// `Debug` on a detection shows the coin but not the tweak.
    #[test]
    fn detection_debug_redacts_the_tweak() {
        use super::DetectedSpCoin;
        use chia_sdk_types::silent_payments::ScalarField;

        let detection = DetectedSpCoin {
            coin_id: [1u8; 32].into(),
            puzzle_hash: [2u8; 32].into(),
            amount: 42,
            parent_coin_id: [3u8; 32].into(),
            k: 7,
            label: Some(1),
            tweak: ScalarField::from_bytes_raw([0xab; 32]),
        };
        for rendered in [format!("{detection:?}"), format!("{detection:#?}")] {
            assert!(rendered.contains("<redacted>"));
            assert!(!rendered.contains("abab"));
            assert!(rendered.contains("amount: 42"));
        }
    }
}
