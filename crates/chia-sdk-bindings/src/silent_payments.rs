//! Bindings for silent payments (CHIP-0057).

use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::offer::Payment;
use chia_sdk_driver::SpendContext;
use clvmr::serde::{node_from_bytes, node_to_bytes};

use crate::Mnemonic;

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

    /// The address of the change label (m = 0). It must not be shared.
    pub fn change_address(&self, network: SilentPaymentNetwork) -> Result<SilentPaymentAddress> {
        Ok(self.0.change_address(network.into()).into())
    }

    pub fn label_registry(&self, labels: Vec<u32>) -> Result<LabelRegistry> {
        Ok(self.0.label_registry(labels).into())
    }
}

#[derive(Clone)]
pub struct LabelRegistry(Arc<Mutex<chia_sdk_utils::silent_payments::LabelRegistry>>);

impl LabelRegistry {
    /// Only the scan public key is kept.
    pub fn new(scan_sk: SecretKey) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(
            chia_sdk_utils::silent_payments::LabelRegistry::new(&scan_sk),
        ))))
    }

    /// `scan_sk` must be the key the registry was created for.
    pub fn register(&self, scan_sk: SecretKey, m: u32) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .register(&scan_sk, m)
            .map_err(chia_sdk_driver::DriverError::from)?;
        Ok(())
    }

    pub fn scan_pk(&self) -> Result<PublicKey> {
        Ok(*self.0.lock().unwrap().scan_pk())
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
        match Arc::try_unwrap(value.0) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        }
    }
}

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

/// The driver's `Wrapper`. The bindings already have `WrapperMemo` and `ParsedWrapper`.
#[derive(Clone)]
pub struct SilentPaymentWrapper {
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Option<Bytes32>,
}

impl SilentPaymentWrapper {
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

#[derive(Clone)]
pub struct TweakData {
    pub tweak_points: Vec<PublicKey>,
    pub outputs: Vec<OutputMeta>,
    pub wrappers: Vec<SilentPaymentWrapper>,
}

impl TweakData {
    // Explicit, not bindy's `"new": true`, which would take every field.
    pub fn new(tweak_points: Vec<PublicKey>, outputs: Vec<OutputMeta>) -> Result<Self> {
        Ok(Self {
            tweak_points,
            outputs,
            wrappers: Vec::new(),
        })
    }

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

/// A scalar mod r. It is its own class, not a bytes type, so that every value is reduced.
#[derive(Clone)]
pub struct ScalarField(chia_sdk_types::silent_payments::ScalarField);

impl ScalarField {
    pub fn from_bytes(bytes: Bytes32) -> Result<Self> {
        // The constructor without reduction is not exposed: an unreduced scalar gives payments no
        // scanner finds.
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

/// `puzzle_hash` is the bare one-time puzzle hash; `coin()` is the coin as it is on chain.
#[derive(Clone)]
pub struct DetectedSilentPaymentCoin {
    pub coin_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub parent_coin_id: Bytes32,
    pub k: u32,
    pub label: Option<u32>,
    pub tweak: ScalarField,
    pub wrapper: Option<SilentPaymentWrapper>,
}

impl DetectedSilentPaymentCoin {
    pub fn coin(&self) -> Result<Coin> {
        let wrapper = self.wrapper.clone().map(chia_sdk_driver::Wrapper::from);
        Ok(Coin::new(
            self.parent_coin_id,
            chia_sdk_driver::wrap_puzzle_hash(wrapper.as_ref(), self.puzzle_hash),
            self.amount,
        ))
    }

    /// The coin is spent with the synthetic key of the result.
    pub fn onetime_sk(&self, spend_sk: SecretKey) -> Result<SecretKey> {
        Ok(chia_sdk_driver::derive_onetime_sk(
            &spend_sk,
            &self.tweak.0,
        )?)
    }
}

impl From<chia_sdk_driver::DetectedSilentPaymentCoin> for DetectedSilentPaymentCoin {
    fn from(value: chia_sdk_driver::DetectedSilentPaymentCoin) -> Self {
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

/// `scalar` links a labeled address to the wallet's other addresses and must be kept private.
#[derive(Clone)]
pub struct SilentPaymentLabel {
    pub scalar: ScalarField,
    pub public_key: PublicKey,
}

#[derive(Clone)]
pub struct SilentPayments;

/// `secret_key` is the raw wallet secret key; the synthetic key is derived from it.
#[derive(Clone)]
pub struct SilentPaymentSecretKey {
    pub p2_puzzle_hash: Bytes32,
    pub secret_key: SecretKey,
}

#[derive(Clone)]
pub struct SilentPaymentEligibleSpend {
    pub synthetic_key: PublicKey,
    pub wrapper: Option<SilentPaymentWrapper>,
}

#[derive(Clone)]
pub struct SilentPaymentSpendGroupCoin {
    pub coin_id: Bytes32,
    pub wrapper: Option<SilentPaymentWrapper>,
}

/// `outputs` are the coins its members create and the coins created by the spends whose puzzle
/// announcements they assert (settlement coins).
#[derive(Clone)]
pub struct SilentPaymentSpendGroup {
    pub coins: Vec<SilentPaymentSpendGroupCoin>,
    pub tweak_point: PublicKey,
    pub outputs: Vec<Coin>,
}

// bindy cannot marshal tuples or `Vec<Option<T>>`, so the arguments and results of
// `derive_settlement_outputs` are small classes, and CLVM values are serialized bytes.

/// `secret_key` is the SYNTHETIC secret key of the coin, unlike `SilentPaymentSecretKey`, and
/// `puzzle_hash` the coin's puzzle hash on chain. `derive_settlement_outputs` fails with
/// `SilentPaymentKeyNotSynthetic` if the key does not belong to the coin.
#[derive(Clone)]
pub struct SilentPaymentGroupCoin {
    pub coin_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub secret_key: SecretKey,
    pub wrapper: Option<SilentPaymentWrapper>,
}

impl SilentPaymentGroupCoin {
    pub fn from_coin(
        coin: Coin,
        secret_key: SecretKey,
        wrapper: Option<SilentPaymentWrapper>,
    ) -> Result<Self> {
        Ok(Self {
            coin_id: coin.coin_id(),
            puzzle_hash: coin.puzzle_hash,
            secret_key,
            wrapper,
        })
    }
}

/// `settled` is true for a payment made by a settlement coin and false for an output of the
/// party's own spend group. `spend_pk` is the spend key of the address paid (`B_m` for a label).
#[derive(Clone)]
pub struct SilentPaymentSettlementRecipient {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    pub wrapper: Option<SilentPaymentWrapper>,
    pub settled: bool,
    pub amount: u64,
}

/// `payment` is the serialized CLVM of the payment, `(puzzle_hash amount memos)`.
#[derive(Clone)]
pub struct SilentPaymentOtherPayment {
    pub wrapper: Option<SilentPaymentWrapper>,
    pub payment: Bytes,
}

#[derive(Clone)]
pub struct SilentPaymentGroupOutput {
    pub k: u32,
    pub wrapper: Option<SilentPaymentWrapper>,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub wrapped_puzzle_hash: Bytes32,
}

/// `settlement_outputs` and `settled_payments` follow the order of the settled recipients;
/// `announcements` is aligned with `notarized_payments`, one per asset.
#[derive(Clone)]
pub struct SilentPaymentSettlement {
    pub settlement_outputs: Vec<Bytes32>,
    pub announcements: Vec<Bytes32>,
    pub settled_payments: Vec<Bytes>,
    pub notarized_payments: Vec<Bytes>,
    pub group_outputs: Vec<SilentPaymentGroupOutput>,
}

fn serialized<T>(ctx: &mut SpendContext, value: &T) -> Result<Bytes>
where
    T: clvm_traits::ToClvm<clvmr::Allocator>,
{
    let ptr = ctx.alloc(value)?;
    Ok(node_to_bytes(ctx, ptr)?.into())
}

impl SilentPayments {
    /// `K_max`, the most outputs to one scan key in a spend group; pass it to `scan_from_tweaks`.
    pub fn k_max() -> Result<u32> {
        Ok(chia_sdk_types::silent_payments::K_MAX)
    }

    /// The change label (m = 0) is always checked and is reported as label 0.
    pub fn scan_from_tweaks(
        b_scan: SecretKey,
        b_spend_pub: PublicKey,
        data: TweakData,
        labels: LabelRegistry,
        k_max: u32,
    ) -> Result<Vec<DetectedSilentPaymentCoin>> {
        let driver_data: chia_sdk_driver::TweakData = data.into();
        let driver_labels: chia_sdk_utils::silent_payments::LabelRegistry = labels.into();
        let detections = chia_sdk_driver::scan_from_tweaks(
            &b_scan,
            &b_spend_pub,
            &driver_data,
            Some(&driver_labels),
            k_max as usize,
        )?;
        Ok(detections.into_iter().map(Into::into).collect())
    }

    pub fn generate_label(scan_sk: SecretKey, m: u32) -> Result<SilentPaymentLabel> {
        let (scalar, public_key) = chia_sdk_utils::silent_payments::generate_label(&scan_sk, m);
        Ok(SilentPaymentLabel {
            scalar: scalar.into(),
            public_key,
        })
    }

    pub fn derive_onetime_sk(spend_sk: SecretKey, tweak: ScalarField) -> Result<SecretKey> {
        Ok(chia_sdk_driver::derive_onetime_sk(&spend_sk, &tweak.0)?)
    }

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
        Ok(chia_sdk_driver::compute_input_hash(&coin_ids, &aggregated_sender_pk)?.into())
    }

    /// Returns nothing for a group that a block's tweak points leave out: its key sum is the
    /// identity element or its input hash is zero.
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

    /// The sum of the synthetic secret keys of the spend group. Fails if it is zero.
    pub fn aggregate_sender_sks(sks: Vec<SecretKey>) -> Result<SecretKey> {
        Ok(chia_sdk_driver::aggregate_sender_sks(&sks)?)
    }

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

    /// In the order of the tweak points of `tweak_data_from_block_spends`.
    pub fn spend_groups_from_block_spends(
        coin_spends: Vec<CoinSpend>,
        additions: Vec<Coin>,
    ) -> Result<Vec<SilentPaymentSpendGroup>> {
        let groups = chia_sdk_driver::spend_groups_from_block_spends(&coin_spends, &additions)?;
        Ok(groups
            .into_iter()
            .map(|group| SilentPaymentSpendGroup {
                coins: group
                    .coin_ids
                    .into_iter()
                    .zip(group.member_wrappers)
                    .map(|(coin_id, wrapper)| SilentPaymentSpendGroupCoin {
                        coin_id,
                        wrapper: wrapper.map(Into::into),
                    })
                    .collect(),
                tweak_point: group.tweak_point,
                outputs: group.outputs,
            })
            .collect())
    }

    pub fn eligible_spend(coin_spend: CoinSpend) -> Result<Option<SilentPaymentEligibleSpend>> {
        Ok(
            chia_sdk_driver::eligible_spend(&coin_spend).map(|(synthetic_key, wrapper)| {
                SilentPaymentEligibleSpend {
                    synthetic_key,
                    wrapper: wrapper.map(Into::into),
                }
            }),
        )
    }

    pub fn sort_wrappers(wrappers: Vec<SilentPaymentWrapper>) -> Result<Vec<SilentPaymentWrapper>> {
        Ok(
            chia_sdk_driver::sort_wrappers(wrappers.into_iter().map(Into::into))
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    /// CHIP-0057 `DeriveSettlementOutputs`. `recipients` are the settled payments and the outputs
    /// of the party's own group, such as its change, in one `k` sequence per scan key.
    pub fn derive_settlement_outputs(
        group: Vec<SilentPaymentGroupCoin>,
        recipients: Vec<SilentPaymentSettlementRecipient>,
        other_payments: Vec<SilentPaymentOtherPayment>,
        nonce: Bytes32,
    ) -> Result<SilentPaymentSettlement> {
        let mut ctx = SpendContext::new();

        // The driver checks each key against its coin's puzzle hash.
        let group: Vec<chia_sdk_driver::SettlementGroupCoin> = group
            .into_iter()
            .map(|c| chia_sdk_driver::SettlementGroupCoin {
                coin_id: c.coin_id,
                puzzle_hash: c.puzzle_hash,
                secret_key: chia_sdk_driver::SyntheticSecretKey::from_synthetic_unchecked(
                    c.secret_key,
                ),
                wrapper: c.wrapper.map(Into::into),
            })
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
            &group,
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
    use chia_puzzle_types::{DeriveSynthetic, standard::StandardArgs};

    /// An empty `coin_ids` list passed to the FFI-reachable
    /// `compute_input_hash` facade returns `Err` (a typed
    /// `DriverError::SilentPaymentNoXchInputs`) instead of panicking across the
    /// FFI boundary.
    #[test]
    fn empty_input_returns_err_not_panic() {
        // `PublicKey::default()` is the identity point; the driver rejects the
        // empty list before the aggregated PK is ever read, so any value is
        // fine here.
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
    /// panicking: an empty group, a key that does not belong to its coin (the
    /// raw key, for an XCH and a CAT coin, and the wrong wrapper), an other
    /// payment that is not CLVM, and an identity scan or spend key.
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

        let group_sk = SecretKey::from_seed(&[3u8; 32]);
        let group = vec![SilentPaymentGroupCoin {
            coin_id: Bytes32::new([5; 32]),
            puzzle_hash: StandardArgs::curry_tree_hash(group_sk.public_key()).into(),
            secret_key: group_sk.clone(),
            wrapper: None,
        }];

        // The raw wallet key of a coin held at its synthetic key's puzzle hash
        // is refused, not used to derive undetectable outputs, for an XCH coin
        // and a CAT coin; so is the coin's synthetic key under the wrong
        // wrapper. The synthetic key under the coin's wrapper is accepted.
        let synthetic_sk = group_sk.derive_synthetic();
        let synthetic_p2: Bytes32 = StandardArgs::curry_tree_hash(synthetic_sk.public_key()).into();
        let cat = SilentPaymentWrapper {
            asset_id: Bytes32::new([6; 32]),
            hidden_puzzle_hash: None,
        };
        let not_synthetic = |entry: SilentPaymentGroupCoin| {
            matches!(
                SilentPayments::derive_settlement_outputs(
                    vec![entry],
                    vec![recipient.clone()],
                    Vec::new(),
                    nonce
                ),
                Err(bindy::Error::Driver(
                    chia_sdk_driver::DriverError::SilentPaymentKeyNotSynthetic
                ))
            )
        };
        for wrapper in [None, Some(cat.clone())] {
            let puzzle_hash = match &wrapper {
                None => synthetic_p2,
                Some(w) => w.wrap(synthetic_p2).unwrap(),
            };
            let coin = Coin::new(Bytes32::new([5; 32]), puzzle_hash, 1);
            let of_coin = |sk: &SecretKey, w: Option<SilentPaymentWrapper>| {
                SilentPaymentGroupCoin::from_coin(coin, sk.clone(), w).unwrap()
            };
            assert_eq!(of_coin(&synthetic_sk, None).coin_id, coin.coin_id());
            assert!(not_synthetic(of_coin(&group_sk, wrapper.clone())));
            let wrong = if wrapper.is_some() {
                None
            } else {
                Some(cat.clone())
            };
            assert!(not_synthetic(of_coin(&synthetic_sk, wrong)));
            assert!(
                SilentPayments::derive_settlement_outputs(
                    vec![of_coin(&synthetic_sk, wrapper)],
                    vec![recipient.clone()],
                    Vec::new(),
                    nonce
                )
                .is_ok()
            );
        }
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
