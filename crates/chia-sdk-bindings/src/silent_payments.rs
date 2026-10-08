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
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::offer::Payment;
use chia_sdk_driver::SpendContext;
use clvmr::serde::{node_from_bytes, node_to_bytes};

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

// ─── SilentPaymentWrapper (the asset of a CAT output) ────────────────────

/// The wrapper of a CAT (CHIP-0057 "Extension to CATs"): the asset ID and, for
/// a revocable CAT, the revocation layer's hidden puzzle hash. XCH has no
/// wrapper (`None` wherever a wrapper is optional).
///
/// Named `SilentPaymentWrapper` because the bindings' namespace is flat and
/// already has `WrapperMemo` and `ParsedWrapper`.
#[derive(Clone)]
pub struct SilentPaymentWrapper {
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Option<Bytes32>,
}

impl SilentPaymentWrapper {
    /// `wrap(W, h)`: the puzzle hash of a coin of this asset whose inner
    /// standard puzzle has tree hash `puzzle_hash` (the CAT layer, and the
    /// revocation layer for a revocable CAT, around it).
    pub fn wrap(&self, puzzle_hash: Bytes32) -> Result<Bytes32> {
        Ok(chia_sdk_driver::Wrapper::from(self.clone()).wrap(puzzle_hash))
    }
}

impl From<chia_sdk_driver::Wrapper> for SilentPaymentWrapper {
    fn from(value: chia_sdk_driver::Wrapper) -> Self {
        Self {
            asset_id: value.asset_id,
            hidden_puzzle_hash: value.hidden_puzzle_hash,
        }
    }
}

impl From<SilentPaymentWrapper> for chia_sdk_driver::Wrapper {
    fn from(value: SilentPaymentWrapper) -> Self {
        Self {
            asset_id: value.asset_id,
            hidden_puzzle_hash: value.hidden_puzzle_hash,
        }
    }
}

// ─── TweakData (3 fields, explicit two-argument constructor) ─────────────
//
// The constructor is explicit, not bindy's `"new": true` (which takes every
// field), so `TweakData(tweak_points, outputs)` keeps its two arguments after
// `wrappers` was added. `with_wrappers` builds tweak data with the block's
// wrapper list.

#[derive(Clone)]
pub struct TweakData {
    pub tweak_points: Vec<PublicKey>,
    pub outputs: Vec<OutputMeta>,
    /// The block's wrapper list; empty when no CAT coin is spent in it. The
    /// scanner tests every output bare and under each wrapper.
    pub wrappers: Vec<SilentPaymentWrapper>,
}

impl TweakData {
    /// Tweak data with an empty wrapper list: outputs are tested bare only.
    pub fn new(tweak_points: Vec<PublicKey>, outputs: Vec<OutputMeta>) -> Result<Self> {
        Ok(Self {
            tweak_points,
            outputs,
            wrappers: Vec::new(),
        })
    }

    /// Tweak data with the block's wrapper list.
    pub fn with_wrappers(
        tweak_points: Vec<PublicKey>,
        outputs: Vec<OutputMeta>,
        wrappers: Vec<SilentPaymentWrapper>,
    ) -> Result<Self> {
        Ok(Self {
            tweak_points,
            outputs,
            wrappers,
        })
    }
}

impl From<chia_sdk_driver::TweakData> for TweakData {
    fn from(value: chia_sdk_driver::TweakData) -> Self {
        Self {
            tweak_points: value.tweak_points,
            outputs: value.outputs.into_iter().map(Into::into).collect(),
            wrappers: value.wrappers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<TweakData> for chia_sdk_driver::TweakData {
    fn from(value: TweakData) -> Self {
        Self::with_wrappers(
            value.tweak_points,
            value.outputs.into_iter().map(Into::into).collect(),
            value.wrappers.into_iter().map(Into::into).collect(),
        )
    }
}

// ─── ScalarField (own class, NOT type-grouped to {bytes}) ────────────────

/// CHIP-0057 mod-r scalar with unsigned reduction at construction.
///
/// Scalars such as a detection's tweak and a label scalar are sensitive. The
/// underlying value is zeroized when the object is dropped, but bytes returned
/// by `to_bytes` are the caller's to protect.
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
///
/// `puzzle_hash` is the bare one-time puzzle hash (the inner puzzle hash of a
/// CAT output). `wrapper` is the asset under which the coin matched: `None`
/// for XCH. `coin()` is the coin as it is on chain, with puzzle hash
/// `wrap(wrapper, puzzle_hash)`.
#[derive(Clone)]
pub struct DetectedSpCoin {
    pub coin_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub parent_coin_id: Bytes32,
    pub k: u32,
    pub label: Option<u32>,
    pub tweak: ScalarField,
    pub wrapper: Option<SilentPaymentWrapper>,
}

impl DetectedSpCoin {
    /// The detected output coin as it is on chain: for a CAT output its puzzle
    /// hash is the wrapped one, `wrap(wrapper, puzzle_hash)`; for XCH it is
    /// the bare one-time puzzle hash.
    pub fn coin(&self) -> Result<Coin> {
        let wrapper = self.wrapper.clone().map(chia_sdk_driver::Wrapper::from);
        Ok(Coin::new(
            self.parent_coin_id,
            chia_sdk_driver::wrap_puzzle_hash(wrapper.as_ref(), self.puzzle_hash),
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
            wrapper: value.wrapper.map(Into::into),
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

// ─── Settlement coins (derive_settlement_outputs, without tuples) ─────────
//
// bindy cannot marshal tuples or `Vec<Option<T>>`, so the spend group, the
// recipient entries and the results of
// `chia_sdk_driver::derive_settlement_outputs` cross the FFI as small classes.
// CLVM values (payments, notarized payments) cross as their serialized bytes,
// as an offer lists them.

/// One coin of a party's spend group: its coin ID, its synthetic secret key
/// and its asset (`None` for XCH).
///
/// Privacy warning: `secret_key` is the coin's spending key.
#[derive(Clone)]
pub struct SilentPaymentGroupCoin {
    pub coin_id: Bytes32,
    pub secret_key: SecretKey,
    pub wrapper: Option<SilentPaymentWrapper>,
}

/// A recipient entry of `derive_settlement_outputs`: `amount` of the asset
/// `wrapper` to the silent payment address `(scan_pk, spend_pk)`, where
/// `spend_pk` is the spend key of the address paid (`B_m` for a labeled
/// address; the change is label 0). `settled` is true for a payment made by
/// the settlement coin of the asset, false for an output of the party's own
/// spend group.
#[derive(Clone)]
pub struct SilentPaymentSettlementRecipient {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    pub wrapper: Option<SilentPaymentWrapper>,
    pub settled: bool,
    pub amount: u64,
}

/// A requested payment of the party to an address that is not a silent
/// payment address, in the asset `wrapper`: `payment` is the serialized CLVM
/// of the payment as the party lists it in its offer, `(puzzle_hash amount
/// memos)`.
#[derive(Clone)]
pub struct SilentPaymentOtherPayment {
    pub wrapper: Option<SilentPaymentWrapper>,
    pub payment: Bytes,
}

/// An output of the party's own spend group (a recipient entry that is not
/// settled): the bare one-time puzzle hash and `wrap(wrapper, puzzle_hash)`,
/// the puzzle hash of the coin a coin of the group creates.
#[derive(Clone)]
pub struct SilentPaymentGroupOutput {
    pub k: u32,
    pub wrapper: Option<SilentPaymentWrapper>,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub wrapped_puzzle_hash: Bytes32,
}

/// The result of `SilentPayments.derive_settlement_outputs`.
///
/// - `settlement_outputs`: `wrap(W, p)` of each settled entry, in the order of
///   the recipients, the coins the settlement coins must create.
/// - `settled_payments`: the serialized payment of each settled entry, XCH
///   `(p amount ())` and CAT `(p amount (p))`.
/// - `notarized_payments`: one serialized notarized payment
///   `(nonce . payments)` per asset, in order of first appearance.
/// - `announcements`: the puzzle announcement each notarized payment's
///   settlement coin makes, aligned with `notarized_payments`; one coin of the
///   party's group asserts each.
/// - `group_outputs`: the outputs of the entries that are not settled.
#[derive(Clone)]
pub struct SilentPaymentSettlement {
    pub settlement_outputs: Vec<Bytes32>,
    pub announcements: Vec<Bytes32>,
    pub settled_payments: Vec<Bytes>,
    pub notarized_payments: Vec<Bytes>,
    pub group_outputs: Vec<SilentPaymentGroupOutput>,
}

/// The serialized CLVM of `value`, allocated in `ctx`.
fn serialized<T>(ctx: &mut SpendContext, value: &T) -> Result<Bytes>
where
    T: clvm_traits::ToClvm<clvmr::Allocator>,
{
    let ptr = ctx.alloc(value)?;
    Ok(node_to_bytes(ctx, ptr)?.into())
}

impl SilentPayments {
    /// `K_max` (2,400): the maximum number of silent-payment outputs for one
    /// scan key in one spend group (CHIP-0057 "Kmax: Maximum Outputs Per Spend
    /// Group"). A sender must not exceed it, and it is the iteration cap to
    /// pass to `scan_from_tweaks`.
    ///
    /// Exposed as a zero-argument static, the way `Constants` exposes its
    /// values.
    pub fn k_max() -> Result<u32> {
        Ok(chia_sdk_types::silent_payments::K_MAX)
    }

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

    /// The one-time puzzle hash of the sender's `k`-th output to a scan key.
    /// `aggregated_sender_sk` is the result of `aggregate_sender_sks`.
    pub fn derive_one_time_puzzle_hash(
        b_scan_pub: PublicKey,
        b_spend_pub: PublicKey,
        aggregated_sender_sk: SecretKey,
        input_hash: ScalarField,
        k: u32,
    ) -> Result<Bytes32> {
        let ih: chia_sdk_types::silent_payments::ScalarField = input_hash.into();
        Ok(chia_sdk_driver::derive_one_time_puzzle_hash(
            &b_scan_pub,
            &b_spend_pub,
            &aggregated_sender_sk,
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

    /// The tweak point of a spend group, `T = input_hash * A_sum` (CHIP-0057
    /// "Tweak Points"), from the coin ids of the group and the sum of its
    /// synthetic public keys. This is what a server computes for light
    /// clients, one per spend group.
    ///
    /// Returns nothing for a group that is left out of a block's tweak points:
    /// one whose key sum is the identity element, or whose input hash is zero.
    /// An empty `coin_ids` is an error.
    pub fn compute_tweak_point(
        coin_ids: Vec<Bytes32>,
        aggregated_sender_pk: PublicKey,
    ) -> Result<Option<PublicKey>> {
        if coin_ids.is_empty() {
            return Err(chia_sdk_driver::DriverError::SilentPaymentNoXchInputs.into());
        }
        Ok(chia_sdk_driver::compute_tweak_point(
            &coin_ids,
            &aggregated_sender_pk,
        ))
    }

    /// The sum of the sender's synthetic secret keys mod r, one term per coin
    /// of the spend group. Fails if the sum is zero.
    pub fn aggregate_sender_sks(sks: Vec<SecretKey>) -> Result<SecretKey> {
        Ok(chia_sdk_driver::aggregate_sender_sks(&sks)?)
    }

    /// Build a `TweakData` from a real-block `Vec<CoinSpend>` + `Vec<Coin>`
    /// (post-decompression). The canonical entry point for any wallet
    /// processing real blocks (testnet11, mainnet) — not just the in-process
    /// simulator — and for transport clients that materialise block data from
    /// upstream RPC.
    ///
    /// Delegates to the driver-side
    /// `chia_sdk_driver::silent_payments::tweak_data_from_block_spends`, which
    /// forms the block's spend groups as in CHIP-0057 "Scanning a Block":
    /// every standard-puzzle spend on its own, plus every strongly connected
    /// component of two or more standard-puzzle spends in the
    /// `ASSERT_CONCURRENT_SPEND` graph. CAT and revocable-CAT spends on the
    /// holder's path join the groups with the key of their inner standard
    /// puzzle, and give the block's wrapper list (`TweakData.wrappers`, in the
    /// CHIP's order). Spends of other puzzles (NFT, DID, ...) are ignored, and
    /// groups whose keys sum to the identity element yield no tweak point.
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

    /// CHIP-0057 `DeriveSettlementOutputs`: the payments a party is paid
    /// through settlement coins in an offer, derived from its own spend group.
    ///
    /// `group` is the party's spend group (coin ID, synthetic secret key and
    /// asset per coin), `recipients` its recipient entries (settled payments
    /// and the outputs of its own group, such as its change, in one `k`
    /// sequence per scan key), `other_payments` its requested payments to
    /// addresses that are not silent payment addresses, and `nonce` the
    /// offer's nonce of its requested payments. Runs in its own CLVM
    /// allocator and returns the CLVM values serialized.
    ///
    /// Fails, without panicking, on an empty group, a malformed other payment,
    /// an output of the group in an asset no coin of the group holds, more
    /// than `K_max` entries to one scan key, or a zero key sum.
    pub fn derive_settlement_outputs(
        group: Vec<SilentPaymentGroupCoin>,
        recipients: Vec<SilentPaymentSettlementRecipient>,
        other_payments: Vec<SilentPaymentOtherPayment>,
        nonce: Bytes32,
    ) -> Result<SilentPaymentSettlement> {
        let mut ctx = SpendContext::new();

        let sender_sks: Vec<SecretKey> = group.iter().map(|c| c.secret_key.clone()).collect();
        let coin_ids: Vec<Bytes32> = group.iter().map(|c| c.coin_id).collect();
        let coin_wrappers: Vec<Option<chia_sdk_driver::Wrapper>> = group
            .into_iter()
            .map(|c| c.wrapper.map(Into::into))
            .collect();
        let recipients: Vec<chia_sdk_driver::SettlementRecipient> = recipients
            .into_iter()
            .map(|r| chia_sdk_driver::SettlementRecipient {
                scan_pk: r.scan_pk,
                spend_pk: r.spend_pk,
                wrapper: r.wrapper.map(Into::into),
                settled: r.settled,
                amount: r.amount,
            })
            .collect();
        let mut others = Vec::with_capacity(other_payments.len());
        for other in other_payments {
            let ptr = node_from_bytes(&mut ctx, &other.payment)?;
            others.push(chia_sdk_driver::OtherPayment {
                wrapper: other.wrapper.map(Into::into),
                payment: ctx.extract::<Payment>(ptr)?,
            });
        }

        let derivation = chia_sdk_driver::derive_settlement_outputs(
            &mut ctx,
            &sender_sks,
            &coin_ids,
            &coin_wrappers,
            &recipients,
            &others,
            nonce,
        )?;

        let settled_payments = derivation
            .settled_payments
            .iter()
            .map(|payment| serialized(&mut ctx, payment))
            .collect::<Result<Vec<_>>>()?;
        let notarized_payments = derivation
            .notarized_payments
            .iter()
            .map(|np| serialized(&mut ctx, &np.notarized_payment))
            .collect::<Result<Vec<_>>>()?;

        Ok(SilentPaymentSettlement {
            settlement_outputs: derivation.settlement_outputs,
            announcements: derivation
                .notarized_payments
                .iter()
                .map(|np| np.announcement)
                .collect(),
            settled_payments,
            notarized_payments,
            group_outputs: derivation
                .group_outputs
                .into_iter()
                .map(|output| SilentPaymentGroupOutput {
                    k: output.k,
                    wrapper: output.wrapper.map(Into::into),
                    puzzle_hash: output.puzzle_hash,
                    amount: output.amount,
                    wrapped_puzzle_hash: output.wrapped_puzzle_hash,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chia_bls::SecretKey;

    /// An empty `coin_ids` list passed to the FFI-reachable
    /// `compute_input_hash` facade returns `Err` (a typed
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

    /// A zero key sum is rejected through the facade as well: keys `a` and
    /// `r - a` (here `1` and `r - 1`).
    #[test]
    fn zero_key_sum_returns_err() {
        let mut one = [0u8; 32];
        one[31] = 1;
        let mut r_minus_one = chia_sdk_types::silent_payments::GROUP_ORDER;
        r_minus_one[31] = 0;

        let result = SilentPayments::aggregate_sender_sks(vec![
            SecretKey::from_bytes(&one).unwrap(),
            SecretKey::from_bytes(&r_minus_one).unwrap(),
        ]);
        assert!(matches!(
            result,
            Err(bindy::Error::Driver(
                chia_sdk_driver::DriverError::SilentPaymentZeroKeySum
            ))
        ));
    }

    fn b32(hex_str: &str) -> Bytes32 {
        Bytes32::new(hex::decode(hex_str).unwrap().try_into().unwrap())
    }

    /// `SilentPaymentWrapper.wrap` is the driver's `wrap(W, h)`: Vector 9
    /// (plain CAT) and Vector 11 (revocable CAT) of the CHIP's vectors.
    #[test]
    fn wrapper_wrap_matches_the_vectors() {
        let v9 = SilentPaymentWrapper {
            asset_id: b32("a031232e72df4aa270c253a530b9b0d782d46ac7b5dad63c2aa37690bbb37f6d"),
            hidden_puzzle_hash: None,
        };
        assert_eq!(
            v9.wrap(b32(
                "df83709ebd837a46717cfb59cc3c75de70606ffb5e3499c202339cf9a5a20dd4"
            ))
            .unwrap(),
            b32("cf9f1b5dbf2bd67fa0bb69a554e38c0f2d53877fe565f34f16a16df9f6fa18e5")
        );

        let v11 = SilentPaymentWrapper {
            asset_id: b32("c8bd63be5a809f7da4af2ff4c33be96ece3373aa596bec55c60c548c115ed011"),
            hidden_puzzle_hash: Some(b32(
                "e5380d52cb3751ed5b07ceb5dc8eaf48335196e6e2847159c77354fad8372c3e",
            )),
        };
        assert_eq!(
            v11.wrap(b32(
                "4d3fbd4e5db29059a9f9e94474510a2e96f9d4017845ab5cea58bed984aa04a1"
            ))
            .unwrap(),
            b32("87b7f0ce1f5ed425c958cad384eb45754a7d02c7cbaa42b5f2d929f2a7d6eb4c")
        );
    }

    /// The two-argument constructor leaves the wrapper list empty, and the
    /// wrapper list crosses the facade in both directions.
    #[test]
    fn tweak_data_carries_wrappers_both_ways() {
        let wrapper = SilentPaymentWrapper {
            asset_id: Bytes32::new([3; 32]),
            hidden_puzzle_hash: Some(Bytes32::new([4; 32])),
        };
        let bare = TweakData::new(Vec::new(), Vec::new()).unwrap();
        assert!(bare.wrappers.is_empty());
        assert!(chia_sdk_driver::TweakData::from(bare).wrappers.is_empty());

        let with = TweakData::with_wrappers(Vec::new(), Vec::new(), vec![wrapper]).unwrap();
        let driver = chia_sdk_driver::TweakData::from(with);
        assert_eq!(
            driver.wrappers,
            vec![chia_sdk_driver::Wrapper::revocable(
                Bytes32::new([3; 32]),
                Bytes32::new([4; 32])
            )]
        );
        let back = TweakData::from(driver);
        assert_eq!(back.wrappers.len(), 1);
        assert_eq!(back.wrappers[0].asset_id, Bytes32::new([3; 32]));
        assert_eq!(
            back.wrappers[0].hidden_puzzle_hash,
            Some(Bytes32::new([4; 32]))
        );
    }

    /// `derive_settlement_outputs` returns errors across the FFI instead of
    /// panicking: an empty group, an other payment that is not CLVM, and an
    /// identity scan or spend key.
    #[test]
    fn derive_settlement_outputs_errors() {
        let scan = SecretKey::from_seed(&[1u8; 32]);
        let spend = SecretKey::from_seed(&[2u8; 32]);
        let recipient = SilentPaymentSettlementRecipient {
            scan_pk: scan.public_key(),
            spend_pk: spend.public_key(),
            wrapper: None,
            settled: true,
            amount: 1,
        };
        let nonce = Bytes32::new([9; 32]);

        let result = SilentPayments::derive_settlement_outputs(
            Vec::new(),
            vec![recipient.clone()],
            Vec::new(),
            nonce,
        );
        assert!(matches!(
            result,
            Err(bindy::Error::Driver(
                chia_sdk_driver::DriverError::SilentPaymentInvalidSettlementInput
            ))
        ));

        let group = vec![SilentPaymentGroupCoin {
            coin_id: Bytes32::new([5; 32]),
            secret_key: SecretKey::from_seed(&[3u8; 32]),
            wrapper: None,
        }];
        let malformed = SilentPaymentOtherPayment {
            wrapper: None,
            payment: Bytes::new(vec![0xff]),
        };
        assert!(
            SilentPayments::derive_settlement_outputs(
                group.clone(),
                vec![recipient.clone()],
                vec![malformed],
                nonce
            )
            .is_err()
        );

        // An identity scan key or spend key is refused (CHIP-0057
        // SendSilentPayment), as the action system refuses such an address.
        for identity_scan in [true, false] {
            let mut bad = recipient.clone();
            if identity_scan {
                bad.scan_pk = PublicKey::default();
            } else {
                bad.spend_pk = PublicKey::default();
            }
            let result = SilentPayments::derive_settlement_outputs(
                group.clone(),
                vec![bad],
                Vec::new(),
                nonce,
            );
            assert!(matches!(
                result,
                Err(bindy::Error::Driver(
                    chia_sdk_driver::DriverError::SilentPayment(
                        chia_sdk_utils::silent_payments::SilentPaymentError::IdentityPublicKey
                    )
                ))
            ));
        }

        // A well-formed call gives one settled XCH payment, (p amount ()).
        let settlement =
            SilentPayments::derive_settlement_outputs(group, vec![recipient], Vec::new(), nonce)
                .unwrap();
        assert_eq!(settlement.settlement_outputs.len(), 1);
        assert_eq!(settlement.announcements.len(), 1);
        assert_eq!(settlement.notarized_payments.len(), 1);
        assert!(settlement.group_outputs.is_empty());
        let payment = settlement.settled_payments[0].as_ref();
        assert_eq!(&payment[1..2], &[0xa0]);
        assert_eq!(&payment[2..34], settlement.settlement_outputs[0].as_ref());
        assert!(payment.ends_with(&[0xff, 0x80, 0x80]));
    }

    /// The facade exposes the same limit the sender and the scanner enforce.
    #[test]
    fn k_max_is_the_shared_limit() {
        let k_max = SilentPayments::k_max().unwrap();
        assert_eq!(k_max, 2400);
        assert_eq!(k_max as usize, chia_sdk_driver::K_MAX_DEFAULT);
    }

    /// The tweak point facade: a point for a normal group, nothing for an
    /// identity key sum, and an error (not a panic) for no coin ids.
    #[test]
    fn compute_tweak_point_facade() {
        let sender_pk = SecretKey::from_seed(&[7u8; 32]).public_key();
        let coin_ids = vec![Bytes32::new([0x11; 32])];

        let point = SilentPayments::compute_tweak_point(coin_ids.clone(), sender_pk)
            .unwrap()
            .expect("a tweak point");
        assert_eq!(
            Some(point),
            chia_sdk_driver::compute_tweak_point(&coin_ids, &sender_pk)
        );

        assert!(
            SilentPayments::compute_tweak_point(coin_ids, PublicKey::default())
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            SilentPayments::compute_tweak_point(Vec::new(), sender_pk),
            Err(bindy::Error::Driver(
                chia_sdk_driver::DriverError::SilentPaymentNoXchInputs
            ))
        ));
    }
}
