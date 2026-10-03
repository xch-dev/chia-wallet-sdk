//! Silent-payments (chip-0057) binding facade.
//!
//! chip-0057 is enabled unconditionally on this crate's chia-sdk-{driver,utils,
//! types,test} dependencies, so the facade carries no cargo feature of its own
//! and no `#[cfg(feature = "chip-0057")]` attributes. The free-function
//! primitives (`scan_from_tweaks`, `derive_onetime_sk`,
//! `derive_one_time_puzzle_hash`, `compute_input_hash`, `aggregate_sender_sks`,
//! `tweak_data_from_block_spends`) are surfaced as static methods
//! on a zero-field `SilentPayments` namespace class — the same shape used by
//! the `Constants` and `Clvm` facades elsewhere in this crate. `ScalarField`
//! is exposed as its own bindy class (not type-grouped to `{bytes}`) so the
//! unsigned mod-r reduction invariant survives the FFI boundary and cannot be
//! bypassed by callers handing in a raw 32-byte buffer.
//!
//! Privacy warning: silent-payment memos and scan keys carry chip-0057 hazards.
//! Any party holding the recipient's scan secret key can detect every payment
//! to that address, and memos attached to silent-payment outputs land on chain
//! in plaintext.

use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin, CoinSpend};

use crate::Mnemonic;

// ─── SilentPaymentNetwork (unit-variant enum) ────────────────────────────

/// Network discriminator for silent-payment addresses (mainnet `spxch` /
/// testnet `tspxch`).
#[derive(Clone, Copy, Debug)]
pub enum SilentPaymentNetwork {
    Mainnet,
    Testnet,
}

impl From<chia_sdk_utils::silent_payments::SilentPaymentNetwork> for SilentPaymentNetwork {
    fn from(value: chia_sdk_utils::silent_payments::SilentPaymentNetwork) -> Self {
        match value {
            chia_sdk_utils::silent_payments::SilentPaymentNetwork::Mainnet => Self::Mainnet,
            chia_sdk_utils::silent_payments::SilentPaymentNetwork::Testnet => Self::Testnet,
        }
    }
}

impl From<SilentPaymentNetwork> for chia_sdk_utils::silent_payments::SilentPaymentNetwork {
    fn from(value: SilentPaymentNetwork) -> Self {
        match value {
            SilentPaymentNetwork::Mainnet => Self::Mainnet,
            SilentPaymentNetwork::Testnet => Self::Testnet,
        }
    }
}

// ─── SilentPaymentAddress (3-field class + encode/decode) ────────────────

/// CHIP-0057 silent-payment bech32m address.
#[derive(Clone)]
pub struct SilentPaymentAddress {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    pub network: SilentPaymentNetwork,
}

impl SilentPaymentAddress {
    pub fn encode(&self) -> Result<String> {
        let inner = chia_sdk_utils::silent_payments::SilentPaymentAddress::new(
            self.scan_pk,
            self.spend_pk,
            self.network.into(),
        );
        Ok(inner.encode()?)
    }

    pub fn decode(address: String) -> Result<Self> {
        let inner = chia_sdk_utils::silent_payments::SilentPaymentAddress::decode(&address)?;
        Ok(Self {
            scan_pk: inner.scan_pk,
            spend_pk: inner.spend_pk,
            network: inner.network.into(),
        })
    }
}

impl From<chia_sdk_utils::silent_payments::SilentPaymentAddress> for SilentPaymentAddress {
    fn from(value: chia_sdk_utils::silent_payments::SilentPaymentAddress) -> Self {
        Self {
            scan_pk: value.scan_pk,
            spend_pk: value.spend_pk,
            network: value.network.into(),
        }
    }
}

impl From<SilentPaymentAddress> for chia_sdk_utils::silent_payments::SilentPaymentAddress {
    fn from(value: SilentPaymentAddress) -> Self {
        Self::new(value.scan_pk, value.spend_pk, value.network.into())
    }
}

// ─── SilentPaymentKeys (opaque wrapper with getters + factories) ─────────

/// CHIP-0057 scan + spend key bundle.
///
/// Privacy warning: `scan_sk` lets the holder see every payment to the
/// associated address. Treat as the more sensitive key for at-rest storage.
#[derive(Clone)]
pub struct SilentPaymentKeys(chia_sdk_utils::silent_payments::SilentPaymentKeys);

impl SilentPaymentKeys {
    pub fn from_mnemonic(mnemonic: Mnemonic) -> Result<Self> {
        Ok(Self(
            chia_sdk_utils::silent_payments::SilentPaymentKeys::from_mnemonic(mnemonic.inner()),
        ))
    }

    pub fn from_secret_keys(scan_sk: SecretKey, spend_sk: SecretKey) -> Result<Self> {
        Ok(Self(
            chia_sdk_utils::silent_payments::SilentPaymentKeys::from_secret_keys(scan_sk, spend_sk),
        ))
    }

    pub fn scan_sk(&self) -> Result<SecretKey> {
        Ok(self.0.scan_sk().clone())
    }

    pub fn spend_sk(&self) -> Result<SecretKey> {
        Ok(self.0.spend_sk().clone())
    }

    pub fn scan_pk(&self) -> Result<PublicKey> {
        Ok(*self.0.scan_pk())
    }

    pub fn spend_pk(&self) -> Result<PublicKey> {
        Ok(*self.0.spend_pk())
    }

    pub fn unlabeled_address(&self, network: SilentPaymentNetwork) -> Result<SilentPaymentAddress> {
        Ok(self.0.unlabeled_address(network.into()).into())
    }

    pub fn labeled_address(
        &self,
        network: SilentPaymentNetwork,
        m: u32,
    ) -> Result<SilentPaymentAddress> {
        Ok(self.0.labeled_address(network.into(), m)?.into())
    }

    /// The wallet's own change address (reserved label `m = 0`).
    ///
    /// Never share this address: anyone who knows it can create payments that
    /// the wallet identifies as its own change. The scanner always checks the
    /// change label and reports it as label 0.
    pub fn change_address(&self, network: SilentPaymentNetwork) -> Result<SilentPaymentAddress> {
        Ok(self.0.change_address(network.into()).into())
    }
}

// ─── LabelRegistry (full register/forward/lookup/len/is_empty API) ───────
//
// Wrapped in Arc<Mutex<_>> so the bindy-generated `&self` dispatch can mutate
// the underlying registry — bindy methods are always `&self` on the wrapper,
// matching the Spends/FinishedSpends precedent in action_system.rs.

#[derive(Clone)]
pub struct LabelRegistry(Arc<Mutex<chia_sdk_utils::silent_payments::LabelRegistry>>);

impl LabelRegistry {
    pub fn new() -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(
            chia_sdk_utils::silent_payments::LabelRegistry::new(),
        ))))
    }

    /// Register label index `m` against scan secret key `scan_sk`.
    pub fn register(&self, scan_sk: SecretKey, m: u32) -> Result<()> {
        self.0.lock().unwrap().register(&scan_sk, m);
        Ok(())
    }

    pub fn forward(&self, m: u32) -> Result<Option<PublicKey>> {
        Ok(self.0.lock().unwrap().forward(m).copied())
    }

    pub fn lookup(&self, label_pk: PublicKey) -> Result<Option<u32>> {
        Ok(self.0.lock().unwrap().lookup(&label_pk))
    }

    pub fn len(&self) -> Result<u32> {
        u32::try_from(self.0.lock().unwrap().len())
            .map_err(|_| bindy::Error::Custom("LabelRegistry length overflows u32".into()))
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.0.lock().unwrap().is_empty())
    }
}

impl From<chia_sdk_utils::silent_payments::LabelRegistry> for LabelRegistry {
    fn from(value: chia_sdk_utils::silent_payments::LabelRegistry) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }
}

impl From<LabelRegistry> for chia_sdk_utils::silent_payments::LabelRegistry {
    fn from(value: LabelRegistry) -> Self {
        // bindy passes `LabelRegistry` by value into static methods like
        // `SilentPayments::scan_from_tweaks`. Unwrap the Arc<Mutex<_>>;
        // try_unwrap is the cheap path, fall back to cloning the inner if
        // another handle is alive.
        match Arc::try_unwrap(value.0) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        }
    }
}

// ─── OutputMeta (4-field class with auto-generated new) ──────────────────

#[derive(Clone)]
pub struct OutputMeta {
    pub puzzle_hash: Bytes32,
    pub coin_id: Bytes32,
    pub amount: u64,
    pub parent_coin_id: Bytes32,
}

impl From<chia_sdk_driver::OutputMeta> for OutputMeta {
    fn from(value: chia_sdk_driver::OutputMeta) -> Self {
        Self {
            puzzle_hash: value.puzzle_hash,
            coin_id: value.coin_id,
            amount: value.amount,
            parent_coin_id: value.parent_coin_id,
        }
    }
}

impl From<OutputMeta> for chia_sdk_driver::OutputMeta {
    fn from(value: OutputMeta) -> Self {
        Self {
            puzzle_hash: value.puzzle_hash,
            coin_id: value.coin_id,
            amount: value.amount,
            parent_coin_id: value.parent_coin_id,
        }
    }
}

// ─── TweakData (2-field class with auto-generated new) ───────────────────

#[derive(Clone)]
pub struct TweakData {
    pub tweak_points: Vec<PublicKey>,
    pub outputs: Vec<OutputMeta>,
}

impl From<chia_sdk_driver::TweakData> for TweakData {
    fn from(value: chia_sdk_driver::TweakData) -> Self {
        Self {
            tweak_points: value.tweak_points,
            outputs: value.outputs.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<TweakData> for chia_sdk_driver::TweakData {
    fn from(value: TweakData) -> Self {
        Self {
            tweak_points: value.tweak_points,
            outputs: value.outputs.into_iter().map(Into::into).collect(),
        }
    }
}

// ─── ScalarField (own class, NOT type-grouped to {bytes}) ────────────────

/// CHIP-0057 mod-r scalar with unsigned reduction at construction.
///
/// Use `ScalarField.fromBytes(bytes)` (TS) / `ScalarField.from_bytes(bytes)`
/// (py) to construct from any 32-byte input — the factory reduces mod r so
/// wallet authors cannot accidentally pass an unreduced value into
/// `SilentPayments.deriveOneTimePuzzleHash`. Exposing this as a dedicated
/// class (rather than collapsing to a raw `{bytes}` type group) is what makes
/// the unsigned-vs-signed reduction choice survive the FFI boundary.
#[derive(Clone)]
pub struct ScalarField(chia_sdk_types::silent_payments::ScalarField);

impl ScalarField {
    pub fn from_bytes(bytes: Bytes32) -> Result<Self> {
        // MUST use the unsigned-reducing factory. The unchecked / no-reduction
        // sibling on `ScalarField` is deliberately not surfaced through this
        // facade — exposing it would let a caller hand in a value above r,
        // producing silently-undetectable on-chain payments.
        Ok(Self(
            chia_sdk_types::silent_payments::ScalarField::from_bytes_unsigned(bytes.into()),
        ))
    }

    pub fn to_bytes(&self) -> Result<Bytes32> {
        Ok(Bytes32::new(self.0.to_bytes()))
    }
}

impl From<chia_sdk_types::silent_payments::ScalarField> for ScalarField {
    fn from(value: chia_sdk_types::silent_payments::ScalarField) -> Self {
        Self(value)
    }
}

impl From<ScalarField> for chia_sdk_types::silent_payments::ScalarField {
    fn from(value: ScalarField) -> Self {
        value.0
    }
}

// ─── DetectedSpCoin (return type of scan_from_tweaks) ────────────────────

/// A detected silent-payment coin. Produced from the scan secret key and the
/// spend public key alone: it carries the combined tweak
/// `(t_k + label_scalar) mod r`, not a spendable key. Call `onetime_sk` with
/// the spend secret key to obtain the one-time key.
#[derive(Clone)]
pub struct DetectedSpCoin {
    pub coin_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub parent_coin_id: Bytes32,
    pub k: u32,
    pub label: Option<u32>,
    pub tweak: ScalarField,
}

impl DetectedSpCoin {
    /// The detected output coin.
    pub fn coin(&self) -> Result<Coin> {
        Ok(Coin::new(
            self.parent_coin_id,
            self.puzzle_hash,
            self.amount,
        ))
    }

    /// The one-time secret key of the coin: `(spend_sk + tweak) mod r`. The
    /// coin is spent with the synthetic key of the result.
    pub fn onetime_sk(&self, spend_sk: SecretKey) -> Result<SecretKey> {
        Ok(chia_sdk_driver::derive_onetime_sk(&spend_sk, &self.tweak.0))
    }
}

impl From<chia_sdk_driver::DetectedSpCoin> for DetectedSpCoin {
    fn from(value: chia_sdk_driver::DetectedSpCoin) -> Self {
        Self {
            coin_id: value.coin_id,
            puzzle_hash: value.puzzle_hash,
            amount: value.amount,
            parent_coin_id: value.parent_coin_id,
            k: value.k,
            label: value.label,
            tweak: value.tweak.into(),
        }
    }
}

// ─── SilentPaymentLabel (return type of generate_label) ──────────────────

/// A CHIP-0057 label: `scalar = int(tagged_hash("Chia_SP/Label", ser256(b_scan)
/// || ser32(m))) mod r` and `public_key = scalar * G`. The labeled spend key is
/// `B_m = B_spend + public_key`.
///
/// The scalar is derived from the scan secret key and links a labeled address
/// to the wallet's other addresses, so it must be kept private.
#[derive(Clone)]
pub struct SilentPaymentLabel {
    pub scalar: ScalarField,
    pub public_key: PublicKey,
}

// ─── SilentPayments (zero-field namespace of statics) ────────────────────

/// Static-functions namespace. Hosts the free-fn protocol primitives under one
/// class name. Mirrors the namespace shape used by `Constants` and `Clvm`
/// elsewhere in the facade.
#[derive(Clone)]
pub struct SilentPayments;

// ─── SP key registration wrappers (Spends::with_silent_payment_keys) ─────
//
// bindy does not natively marshal `Vec<(K, V)>` tuple types across the FFI
// boundary, so the two registration maps that `Spends::with_silent_payment_keys`
// consumes are surfaced as `Vec<SilentPaymentRegisteredKey>` and
// `Vec<SilentPaymentRegisteredSecretKey>` respectively. The facade converts to
// `IndexMap<Bytes32, _>` internally before delegating to the driver.

/// One `(p2_puzzle_hash, raw public_key)` entry used to register the chip-0057
/// silent-payment key bundle on `Spends` before `prepare`.
///
/// `public_key` is the RAW wallet public key; `Spends::with_silent_payment_keys`
/// synthesizes the synthetic key internally via the default hidden puzzle (see
/// that method's docs for the custom-hidden / synthetic-key fail-loud contract).
#[derive(Clone)]
pub struct SilentPaymentRegisteredKey {
    pub p2_puzzle_hash: Bytes32,
    pub public_key: PublicKey,
}

/// One `(p2_puzzle_hash, raw secret_key)` entry used to register the chip-0057
/// silent-payment key bundle on `Spends` before `prepare`.
///
/// `secret_key` is the RAW wallet secret key; `Spends::with_silent_payment_keys`
/// synthesizes the synthetic key internally via the default hidden puzzle (see
/// that method's docs for the custom-hidden / synthetic-key fail-loud contract).
///
/// Privacy warning: `secret_key` carries sensitive secret-key material —
/// wallets must treat the wrapping vec like the SKs themselves (zeroize on
/// drop, do not log).
#[derive(Clone)]
pub struct SilentPaymentRegisteredSecretKey {
    pub p2_puzzle_hash: Bytes32,
    pub secret_key: SecretKey,
}

impl SilentPayments {
    /// Detect silent-payment outputs in a `TweakData` blob.
    ///
    /// The change label `m = 0` is always checked, whether or not it is in
    /// `labels`, and a match is reported as label 0.
    ///
    /// Needs only the scan secret key and the spend public key, so it can run
    /// on a watch-only device. Each detection carries the combined tweak; the
    /// holder of the spend secret key turns it into the one-time key with
    /// `DetectedSpCoin.onetime_sk` or `SilentPayments.derive_onetime_sk`.
    ///
    /// Privacy warning: anyone with the scan secret key sees every payment to
    /// the wallet.
    pub fn scan_from_tweaks(
        b_scan: SecretKey,
        b_spend_pub: PublicKey,
        data: TweakData,
        labels: LabelRegistry,
        k_max: u32,
    ) -> Result<Vec<DetectedSpCoin>> {
        let driver_data: chia_sdk_driver::TweakData = data.into();
        let driver_labels: chia_sdk_utils::silent_payments::LabelRegistry = labels.into();
        let detections = chia_sdk_driver::scan_from_tweaks(
            &b_scan,
            &b_spend_pub,
            &driver_data,
            Some(&driver_labels),
            k_max as usize,
        );
        Ok(detections.into_iter().map(Into::into).collect())
    }

    /// Compute the label scalar and label public key for label index `m`
    /// (CHIP-0057 "Label Generation"). `m = 0` is the reserved change label.
    pub fn generate_label(scan_sk: SecretKey, m: u32) -> Result<SilentPaymentLabel> {
        let (scalar, public_key) = chia_sdk_utils::silent_payments::generate_label(&scan_sk, m);
        Ok(SilentPaymentLabel {
            scalar: scalar.into(),
            public_key,
        })
    }

    /// The one-time secret key for a detection's combined tweak:
    /// `(spend_sk + tweak) mod r`. This is the only step of receiving a silent
    /// payment that needs the spend secret key.
    pub fn derive_onetime_sk(spend_sk: SecretKey, tweak: ScalarField) -> Result<SecretKey> {
        Ok(chia_sdk_driver::derive_onetime_sk(&spend_sk, &tweak.0))
    }

    pub fn derive_one_time_puzzle_hash(
        b_scan_pub: PublicKey,
        b_spend_pub: PublicKey,
        aggregated_sender_sk: ScalarField,
        input_hash: ScalarField,
        k: u32,
    ) -> Result<Bytes32> {
        let agg: chia_sdk_types::silent_payments::ScalarField = aggregated_sender_sk.into();
        let ih: chia_sdk_types::silent_payments::ScalarField = input_hash.into();
        Ok(chia_sdk_driver::derive_one_time_puzzle_hash(
            &b_scan_pub,
            &b_spend_pub,
            &agg,
            &ih,
            k,
        )?)
    }

    pub fn compute_input_hash(
        coin_ids: Vec<Bytes32>,
        aggregated_sender_pk: PublicKey,
    ) -> Result<ScalarField> {
        if coin_ids.is_empty() {
            return Err(chia_sdk_driver::DriverError::SilentPaymentNoXchInputs.into());
        }
        Ok(chia_sdk_driver::compute_input_hash(&coin_ids, &aggregated_sender_pk).into())
    }

    pub fn aggregate_sender_sks(sks: Vec<SecretKey>) -> Result<ScalarField> {
        Ok(chia_sdk_driver::aggregate_sender_sks(&sks)?.into())
    }

    /// Build a `TweakData` from a real-block `Vec<CoinSpend>` + `Vec<Coin>`
    /// (post-decompression). The canonical entry point for any wallet
    /// processing real blocks (testnet11, mainnet) — not just the in-process
    /// simulator — and for transport clients that materialise block data from
    /// upstream RPC.
    ///
    /// Delegates to the driver-side
    /// `chia_sdk_driver::silent_payments::tweak_data_from_block_spends`, which
    /// implements same-puzzle-hash bucketing and iterative Tarjan SCC over
    /// opcode-64 `AssertConcurrentSpend` edges to recover transaction-group
    /// shape from a flat list of spends. Non-standard puzzles (CAT, NFT,
    /// arbitrary mod hashes) skip silently; BLS12-381 identity-element tweak
    /// points are suppressed (CHIP §459). See the driver-side module docs for
    /// the full grouping algorithm.
    pub fn tweak_data_from_block_spends(
        coin_spends: Vec<CoinSpend>,
        additions: Vec<Coin>,
    ) -> Result<TweakData> {
        let driver_td = chia_sdk_driver::silent_payments::tweak_data_from_block_spends(
            &coin_spends,
            &additions,
        )?;
        Ok(driver_td.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chia_bls::SecretKey;

    /// ISSUE-1 boundary proof: an empty `coin_ids` list passed to the
    /// FFI-reachable `compute_input_hash` facade returns `Err` (a typed
    /// `DriverError::SilentPaymentNoXchInputs`) instead of panicking across the
    /// FFI boundary. The test process must NOT abort — `is_err()` is the proof
    /// that the guard intercepts the empty slice before the driver `assert!`.
    #[test]
    fn empty_input_returns_err_not_panic() {
        // `PublicKey::default()` is the identity point; the guard fires before
        // the aggregated PK is ever read, so any value is fine here.
        let result = SilentPayments::compute_input_hash(Vec::new(), PublicKey::default());

        // Pin the error to SilentPaymentNoXchInputs, both by variant and by its
        // display string, so a future refactor cannot silently change the
        // boundary contract. Matching on `&result` avoids requiring `Debug` on
        // the `Ok` payload (`ScalarField` does not derive it).
        let Err(err) = &result else {
            panic!("empty coin_ids must return Err, not panic across the FFI boundary");
        };
        assert!(
            matches!(
                err,
                bindy::Error::Driver(chia_sdk_driver::DriverError::SilentPaymentNoXchInputs)
            ),
            "expected DriverError::SilentPaymentNoXchInputs, got a different bindy::Error variant"
        );
        assert!(
            err.to_string()
                .contains("silent payment requires an xch input"),
            "error display must carry the SilentPaymentNoXchInputs message, got {err}"
        );
    }

    /// Happy-path regression guard: a single-element `coin_ids` list still
    /// delegates correctly to the driver fn and returns `Ok`.
    #[test]
    fn single_input_returns_ok() {
        let sender_pk = SecretKey::from_seed(&[7u8; 32]).public_key();
        let coin_ids = vec![Bytes32::new([0x11; 32])];

        let result = SilentPayments::compute_input_hash(coin_ids, sender_pk);

        assert!(
            result.is_ok(),
            "non-empty coin_ids must delegate to the driver fn and return Ok"
        );
    }
}
