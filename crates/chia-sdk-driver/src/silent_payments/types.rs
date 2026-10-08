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
//!
//! The Extension to CATs of CHIP-0057 adds the block's wrapper list to
//! [`TweakData`] ([`TweakData::wrappers`]) and the wrapper under which a coin
//! matched to [`DetectedSpCoin`] ([`DetectedSpCoin::wrapper`]). XCH is the
//! `None` case of the same fields.

use std::fmt;

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::silent_payments::ScalarField;
use clvmr::NodePtr;

use super::layers::{Wrapper, wrap_puzzle_hash};
use super::protocol::derive_onetime_sk;
use crate::Id;

/// Transport-agnostic input to the silent-payment scanner.
///
/// `tweak_points` are the pre-computed per-spend-group ECDH multipliers
/// `tweak_point[i] = input_hash[i] * A_sum[i]` (the indexer or sender computes
/// these on the send/index side; the scanner consumes them as-is).
/// `outputs` are the candidate coin metadata to scan against — typically all
/// outputs in a single block, but the primitive does not care about block
/// boundaries.
///
/// `wrappers` is the wrapper list of the block (CHIP-0057 "Wrapper List of a
/// Block"): the distinct wrappers of every CAT coin spent in it. The scanner
/// tests every candidate one-time puzzle hash bare and under every wrapper of
/// the list ("Matching Outputs under Wrappers"). An empty list means bare
/// candidates only, which is the whole scan of a block in which no CAT coin is
/// spent; a CAT output is then missed. [`TweakData::new`] leaves the list
/// empty, and [`TweakData::with_wrappers`] sets it.
///
/// No transport fields: no `height`, no `block_hash`, no JSON envelope. A
/// future transport client constructs `TweakData` from its wire
/// messages without breaking this struct's shape.
#[derive(Clone, Debug, Default)]
pub struct TweakData {
    pub tweak_points: Vec<PublicKey>,
    pub outputs: Vec<OutputMeta>,
    /// The wrapper list of the block; empty when no CAT coin is spent in it.
    pub wrappers: Vec<Wrapper>,
}

impl TweakData {
    /// Tweak data with an empty wrapper list: candidates are tested bare only.
    pub fn new(tweak_points: Vec<PublicKey>, outputs: Vec<OutputMeta>) -> Self {
        Self {
            tweak_points,
            outputs,
            wrappers: Vec::new(),
        }
    }

    /// Tweak data with the block's wrapper list.
    ///
    /// The list is kept as given. The scanner tests every entry, so its order
    /// does not change which coins are found; [`super::tweak_data_from_block_spends`]
    /// gives it in the CHIP's order ([`super::sort_wrappers`]).
    pub fn with_wrappers(
        tweak_points: Vec<PublicKey>,
        outputs: Vec<OutputMeta>,
        wrappers: Vec<Wrapper>,
    ) -> Self {
        Self {
            tweak_points,
            outputs,
            wrappers,
        }
    }
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
///
/// `puzzle_hash` is the BARE one-time puzzle hash, the tree hash of the
/// standard puzzle of the one-time key. A coin that matched under a wrapper
/// (a CAT output, CHIP-0057 "Matching Outputs under Wrappers") has puzzle hash
/// `wrap(wrapper, puzzle_hash)`; [`DetectedSpCoin::coin`] returns that coin.
/// For an XCH output (`wrapper` is `None`) the two are equal.
#[derive(Clone)]
pub struct DetectedSpCoin {
    /// The ID of the matched coin.
    pub coin_id: Bytes32,
    /// The bare one-time puzzle hash (the inner puzzle hash of a CAT output).
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
    /// The wrapper under which the coin matched: `None` for an XCH output,
    /// the CAT's wrapper for a CAT output.
    pub wrapper: Option<Wrapper>,
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
            .field("wrapper", &self.wrapper)
            .finish()
    }
}

impl DetectedSpCoin {
    /// The detected output coin, as it is on chain: its puzzle hash is
    /// `wrap(wrapper, puzzle_hash)`, the bare one-time puzzle hash for an XCH
    /// output.
    #[must_use]
    pub fn coin(&self) -> Coin {
        Coin::new(
            self.parent_coin_id,
            wrap_puzzle_hash(self.wrapper.as_ref(), self.puzzle_hash),
            self.amount,
        )
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
/// they go through [`crate::Action::silent_payment_send`] or
/// [`crate::Action::silent_payment_send_asset`].
#[derive(Debug, Clone)]
pub(crate) struct SilentPaymentPending {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    /// The spend that creates the output.
    pub source: SilentPaymentSource,
    pub parent_coin: Coin,
    pub k: u32,
    pub amount: u64,
    pub memos: Memos<NodePtr>,
}

/// The spend that creates a silent payment output, recorded at apply time.
///
/// The index is the position of the spend in its asset's items, which only
/// grow while actions are applied, so it stays valid until
/// [`crate::Spends::prepare`] emits the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SilentPaymentSource {
    /// An XCH output, created by `spends.xch.items[index]`.
    Xch(usize),
    /// A CAT output, created by `spends.cats[id].items[index]`: the first
    /// conditions spend of that asset, the one the spend group's cycle binds.
    Cat(Id, usize),
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
            wrapper: None,
        };
        // A CAT detection: the wrapper is printed, the tweak is not.
        let cat_detection = DetectedSpCoin {
            wrapper: Some(super::Wrapper::revocable(
                [0x44u8; 32].into(),
                [0x55u8; 32].into(),
            )),
            ..detection.clone()
        };
        for rendered in [
            format!("{detection:?}"),
            format!("{detection:#?}"),
            format!("{cat_detection:?}"),
            format!("{cat_detection:#?}"),
        ] {
            assert!(rendered.contains("<redacted>"));
            assert!(!rendered.contains("abab"));
            assert!(rendered.contains("amount: 42"));
            assert!(rendered.contains("wrapper"));
        }
        assert!(format!("{cat_detection:?}").contains("4444"));
        assert!(format!("{cat_detection:?}").contains("5555"));
    }

    /// `coin()` is the coin as it is on chain: the bare puzzle hash for an XCH
    /// detection, `wrap(W, puzzle_hash)` for a CAT detection; the coin ID is
    /// the matched coin's.
    #[test]
    fn detected_coin_is_the_wrapped_coin() {
        use super::{DetectedSpCoin, Wrapper};
        use chia_protocol::{Bytes32, Coin};
        use chia_sdk_types::silent_payments::ScalarField;

        let bare: Bytes32 = [2u8; 32].into();
        let parent: Bytes32 = [3u8; 32].into();
        let w = Wrapper::cat([0x33u8; 32].into());
        let wrapped = Coin::new(parent, w.wrap(bare), 42);
        let detection = DetectedSpCoin {
            coin_id: wrapped.coin_id(),
            puzzle_hash: bare,
            amount: 42,
            parent_coin_id: parent,
            k: 0,
            label: None,
            tweak: ScalarField::from_bytes_raw([1u8; 32]),
            wrapper: Some(w),
        };
        assert_eq!(detection.coin(), wrapped);
        assert_eq!(detection.coin().coin_id(), detection.coin_id);
        let xch = DetectedSpCoin {
            wrapper: None,
            ..detection
        };
        assert_eq!(xch.coin(), Coin::new(parent, bare, 42));
    }
}
