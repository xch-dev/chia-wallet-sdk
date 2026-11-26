//! The scanner's input ([`TweakData`], [`OutputMeta`]) and output
//! ([`DetectedSilentPaymentCoin`]), and the outputs recorded on `Spends` until it is prepared.

use std::fmt;

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_types::silent_payments::ScalarField;
use clvmr::NodePtr;

use super::layers::{Wrapper, wrap_puzzle_hash};
use super::protocol::derive_onetime_sk;
use crate::{DriverError, Id, SettlementPayer};

/// The scanner's input for one block: its tweak points, the coins created in it and its wrapper
/// list (CHIP-0057 "Tweak Points", "Wrapper List of a Block"). With an empty wrapper list a CAT
/// output is missed.
#[derive(Clone, Debug, Default)]
pub struct TweakData {
    /// The tweak point of each spend group of the block, `input_hash * A_sum`.
    pub tweak_points: Vec<PublicKey>,
    /// The coins created in the block, which the scanner matches against.
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

    /// Tweak data with the block's wrapper list, in any order.
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

/// A coin created in the block, as the scanner needs it.
#[derive(Clone, Copy, Debug)]
pub struct OutputMeta {
    /// The coin's puzzle hash, which the scanner matches against.
    pub puzzle_hash: Bytes32,
    /// The coin's ID.
    pub coin_id: Bytes32,
    /// The coin's amount.
    pub amount: u64,
    /// The ID of the coin's parent.
    pub parent_coin_id: Bytes32,
}

/// A silent payment coin found by [`super::scan_from_tweaks`]. It holds no spendable key: the
/// one-time key is derived from `tweak` with [`DetectedSilentPaymentCoin::onetime_sk`]
/// (CHIP-0057 "Spending").
#[derive(Clone)]
pub struct DetectedSilentPaymentCoin {
    /// The ID of the matched coin.
    pub coin_id: Bytes32,
    /// The bare one-time puzzle hash (the inner puzzle hash of a CAT output).
    pub puzzle_hash: Bytes32,
    /// The amount of the matched coin.
    pub amount: u64,
    /// The ID of the matched coin's parent.
    pub parent_coin_id: Bytes32,
    /// The `k` counter at which this output was detected within its spend group.
    pub k: u32,
    /// `None` for unlabeled detections; `Some(m)` for label index `m`.
    pub label: Option<u32>,
    /// The combined tweak: `t_k`, or `(t_k + label_scalar) mod r` for a labeled detection. Keep
    /// it private: with the address it links the coin to the recipient.
    pub tweak: ScalarField,
    /// The wrapper under which the coin matched: `None` for an XCH output.
    pub wrapper: Option<Wrapper>,
}

/// The tweak is not printed, since it links the coin to the recipient.
impl fmt::Debug for DetectedSilentPaymentCoin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DetectedSilentPaymentCoin")
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

impl DetectedSilentPaymentCoin {
    /// The detected coin as it is on chain, with puzzle hash `wrap(wrapper, puzzle_hash)`.
    #[must_use]
    pub fn coin(&self) -> Coin {
        Coin::new(
            self.parent_coin_id,
            wrap_puzzle_hash(self.wrapper.as_ref(), self.puzzle_hash),
            self.amount,
        )
    }

    /// The one-time secret key of the detected coin, `(b_spend + tweak) mod r`. The coin is
    /// spent with the synthetic key of the result (`derive_synthetic()`). Fails only for a
    /// `tweak` not below `r`, which a detection from `scan_from_tweaks` never has.
    pub fn onetime_sk(&self, spend_sk: &SecretKey) -> Result<SecretKey, DriverError> {
        derive_onetime_sk(spend_sk, &self.tweak)
    }
}

/// A silent payment output recorded when its action is applied. Its one-time puzzle hash is
/// derived when the transaction is prepared.
#[derive(Debug, Clone)]
pub(crate) struct SilentPaymentPending {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    /// The spend that creates the output.
    pub source: SilentPaymentSource,
    /// The coin that creates the output, or `None` for a payment created by the counterparty's
    /// settlement coin, which this transaction does not know.
    pub parent_coin: Option<Coin>,
    pub k: u32,
    pub amount: u64,
    pub memos: Memos<NodePtr>,
}

/// A requested payment to an address that is not a silent payment address, listed after the
/// silent payments of its asset in the party's notarized payment.
#[derive(Debug, Clone)]
pub(crate) struct SettledOtherPayment {
    pub asset: Option<Wrapper>,
    pub nonce: Bytes32,
    pub payer: SettlementPayer,
    pub settlement_index: Option<usize>,
    pub payment: chia_puzzle_types::offer::Payment,
}

/// The spend that creates a silent payment output, recorded at apply time. Indexes are positions
/// in an asset's items, which only grow while actions are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SilentPaymentSource {
    /// An XCH output, created by `spends.xch.items[index]`.
    Xch(usize),
    /// A CAT output, created by `spends.cats[id].items[index]`, the asset's first conditions spend.
    Cat(Id, usize),
    /// A requested payment created by a settlement coin of `asset`, listed in the party's
    /// notarized payment under `nonce`. `settlement_index` is the position of a spent
    /// settlement coin in its asset's items.
    Settled {
        asset: Option<Wrapper>,
        nonce: Bytes32,
        payer: SettlementPayer,
        settlement_index: Option<usize>,
    },
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

    /// A detection built by hand can carry a tweak that is not below `r`;
    /// `onetime_sk` reports it instead of panicking.
    #[test]
    fn detection_onetime_sk_rejects_scalar_not_below_r() {
        use super::DetectedSilentPaymentCoin;
        use chia_sdk_types::silent_payments::ScalarField;

        let detection = DetectedSilentPaymentCoin {
            coin_id: [1u8; 32].into(),
            puzzle_hash: [2u8; 32].into(),
            amount: 42,
            parent_coin_id: [3u8; 32].into(),
            k: 0,
            label: None,
            tweak: ScalarField::from_bytes_raw([0xff; 32]),
            wrapper: None,
        };
        let spend_sk = chia_bls::SecretKey::from_seed(&[7u8; 32]);
        assert!(matches!(
            detection.onetime_sk(&spend_sk),
            Err(crate::DriverError::SilentPaymentScalarOutOfRange)
        ));
    }

    /// `Debug` on a detection shows the coin but not the tweak.
    #[test]
    fn detection_debug_redacts_the_tweak() {
        use super::DetectedSilentPaymentCoin;
        use chia_sdk_types::silent_payments::ScalarField;

        let detection = DetectedSilentPaymentCoin {
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
        let cat_detection = DetectedSilentPaymentCoin {
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
        use super::{DetectedSilentPaymentCoin, Wrapper};
        use chia_protocol::{Bytes32, Coin};
        use chia_sdk_types::silent_payments::ScalarField;

        let bare: Bytes32 = [2u8; 32].into();
        let parent: Bytes32 = [3u8; 32].into();
        let w = Wrapper::cat([0x33u8; 32].into());
        let wrapped = Coin::new(parent, w.wrap(bare), 42);
        let detection = DetectedSilentPaymentCoin {
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
        let xch = DetectedSilentPaymentCoin {
            wrapper: None,
            ..detection
        };
        assert_eq!(xch.coin(), Coin::new(parent, bare, 42));
    }
}
