//! Silent payments through settlement coins (CHIP-0057 "Payments through
//! Settlement Coins").
//!
//! In an offer, a party's requested payment is created by a settlement coin,
//! not by a coin of the party. The party derives the one-time puzzle hash of
//! such a payment from its own spend group, exactly as for an output its own
//! coins create, and one coin of its group asserts the settlement coin's
//! puzzle announcement of the notarized payment that lists it.
//!
//! [`derive_settlement_outputs`] is the CHIP's procedure `DeriveSettlementOutputs`,
//! a pure function of the party's spend group, recipient entries, other
//! requested payments and nonce. The action system derives the same values
//! from a transaction with [`crate::Action::silent_payment_requested`] and
//! [`crate::Action::silent_payment_settle`], using the helpers of this module.
//!
//! The encoding of a settled payment is fixed by the CHIP: an XCH payment is
//! `(p amount ())`, with an empty memo list (`Memos::Some(NodePtr::NIL)`), and
//! a CAT payment `(p amount (p))`, hinted with the bare one-time puzzle hash.
//! `(p amount)` (`Memos::None`) has another tree hash, and so another
//! announcement, and is never produced here.

use std::collections::HashMap;

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment},
    standard::StandardArgs,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    conditions::AssertPuzzleAnnouncement, payment_assertion, silent_payments::K_MAX,
    tree_hash_notarized_payment,
};
use chia_sdk_utils::silent_payments::{SilentPaymentAddress, SilentPaymentNetwork};
use clvmr::NodePtr;

use super::layers::{Wrapper, wrap_puzzle_hash};
use super::protocol::{aggregate_sender_sks, compute_input_hash, derive_one_time_puzzle_hash};
use super::send_keys::SyntheticSecretKey;
use crate::{DriverError, SpendContext};

/// A requested payment as an offer lists it: `(p amount ())` for XCH
/// (`wrapper` is `None`) and `(p amount (p))` for a CAT, the memo list holding
/// the hint of the CAT payment (CHIP-0057 `DeriveSettlementOutputs`).
///
/// # Errors
///
/// Fails only if the hint cannot be allocated.
pub fn encoded_payment(
    ctx: &mut SpendContext,
    puzzle_hash: Bytes32,
    amount: u64,
    wrapper: Option<&Wrapper>,
) -> Result<Payment, DriverError> {
    let memos = match wrapper {
        None => Memos::Some(NodePtr::NIL),
        Some(_) => ctx.hint(puzzle_hash)?,
    };
    Ok(Payment::new(puzzle_hash, amount, memos))
}

/// `wrap(W, SETTLEMENT_PAYMENTS_HASH)`: the puzzle hash of the settlement coin
/// of asset `W`, the bare settlement puzzle hash for XCH.
#[must_use]
pub fn settlement_puzzle_hash(wrapper: Option<&Wrapper>) -> Bytes32 {
    wrap_puzzle_hash(wrapper, SETTLEMENT_PAYMENT_HASH.into())
}

/// The `ASSERT_PUZZLE_ANNOUNCEMENT` of a notarized payment made by the
/// settlement coin of asset `W`: it asserts
/// `SHA256(wrap(W, SETTLEMENT_PAYMENTS_HASH) || tree_hash((nonce . payments)))`.
#[must_use]
pub fn settlement_assertion(
    ctx: &SpendContext,
    wrapper: Option<&Wrapper>,
    notarized_payment: &NotarizedPayment,
) -> AssertPuzzleAnnouncement {
    payment_assertion(
        settlement_puzzle_hash(wrapper),
        tree_hash_notarized_payment(ctx, notarized_payment),
    )
}

/// The puzzle announcement the settlement coin of asset `W` makes for the
/// notarized payment its solution lists:
/// `SHA256(wrap(W, SETTLEMENT_PAYMENTS_HASH) || tree_hash((nonce . payments)))`.
#[must_use]
pub fn settlement_announcement(
    ctx: &SpendContext,
    wrapper: Option<&Wrapper>,
    notarized_payment: &NotarizedPayment,
) -> Bytes32 {
    settlement_assertion(ctx, wrapper, notarized_payment).announcement_id
}

/// A coin of the party's spend group in `DeriveSettlementOutputs`: an XCH coin
/// (`wrapper` is `None`) or a CAT coin whose inner puzzle is the standard
/// puzzle (plain, or under the revocation layer of a revocable CAT).
///
/// - `coin_id` and `puzzle_hash` are the coin's ID and its puzzle hash on
///   chain: the standard puzzle hash for XCH, `wrap(W, standard puzzle hash)`
///   for a CAT coin.
/// - `secret_key` is the coin's SYNTHETIC secret key, the one whose public key
///   is curried into the coin's standard puzzle.
/// - `wrapper` is the coin's asset.
///
/// [`derive_settlement_outputs`] checks that the key belongs to the coin:
/// `wrap(wrapper, StandardArgs::curry_tree_hash(secret_key.public_key()))`
/// must equal `puzzle_hash`. That is, the key curries to the coin's standard
/// puzzle, and that puzzle under `wrapper` is the coin's puzzle. A raw wallet
/// key, the key of another coin or the wrong wrapper is refused: it would give
/// a key sum, and so one-time puzzle hashes, that no scanner reconstructs.
///
/// The check is only as good as `puzzle_hash`, which must be the coin's, not a
/// hash computed from the key. [`SettlementGroupCoin::new`] takes the coin ID
/// and the puzzle hash from the coin itself, so they cannot come from
/// different coins; use it for a coin of a transaction. The fields are public
/// because the CHIP's procedure takes coin IDs, and the coin IDs of its test
/// vectors have no coin behind them.
#[derive(Debug, Clone)]
pub struct SettlementGroupCoin {
    pub coin_id: Bytes32,
    pub puzzle_hash: Bytes32,
    pub secret_key: SyntheticSecretKey,
    pub wrapper: Option<Wrapper>,
}

impl SettlementGroupCoin {
    /// The entry of `coin`, held with the synthetic key `secret_key` in the
    /// asset `wrapper` (`None` for XCH): its coin ID and puzzle hash are the
    /// coin's.
    #[must_use]
    pub fn new(coin: &Coin, secret_key: SyntheticSecretKey, wrapper: Option<Wrapper>) -> Self {
        Self {
            coin_id: coin.coin_id(),
            puzzle_hash: coin.puzzle_hash,
            secret_key,
            wrapper,
        }
    }

    /// Whether `secret_key` is the synthetic key of this coin:
    /// `wrap(wrapper, StandardArgs::curry_tree_hash(secret_key.public_key()))`
    /// equals the coin's `puzzle_hash`.
    fn key_belongs_to_coin(&self) -> bool {
        let p2_puzzle_hash: Bytes32 =
            StandardArgs::curry_tree_hash(self.secret_key.public_key().into_inner()).into();
        wrap_puzzle_hash(self.wrapper.as_ref(), p2_puzzle_hash) == self.puzzle_hash
    }
}

/// A recipient entry of `DeriveSettlementOutputs`: an output to a silent
/// payment address, paid in the asset `wrapper` (`None` for XCH).
///
/// `spend_pk` is the spend key of the address that is paid: `B_spend`, or
/// `B_m = B_spend + label_pk(m)` for a labeled address (the party's change is
/// label 0). `settled` is true for a requested payment, paid through the
/// settlement coin of the asset, and false for an output a coin of the party's
/// spend group creates, such as its change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementRecipient {
    pub scan_pk: PublicKey,
    pub spend_pk: PublicKey,
    pub wrapper: Option<Wrapper>,
    pub settled: bool,
    pub amount: u64,
}

/// A requested payment of the party to an address that is not a silent
/// payment address, in the asset `wrapper`, as the party lists it in its
/// offer (for example `(q amount ())`).
#[derive(Debug, Clone)]
pub struct OtherPayment {
    pub wrapper: Option<Wrapper>,
    pub payment: Payment,
}

/// The notarized payment of one asset of the party's requested payments, with
/// the settlement coin's puzzle hash and the announcement it makes for it.
#[derive(Debug, Clone)]
pub struct SettlementNotarizedPayment {
    /// The asset: `None` for XCH.
    pub wrapper: Option<Wrapper>,
    /// `(nonce . payments)`: the settled silent payments of the asset in the
    /// order of the recipient entries, then the asset's other payments.
    pub notarized_payment: NotarizedPayment,
    /// `wrap(W, SETTLEMENT_PAYMENTS_HASH)`.
    pub settlement_puzzle_hash: Bytes32,
    /// The puzzle announcement the party's spend group asserts.
    pub announcement: Bytes32,
}

/// An output of the party's own spend group (a recipient entry that is not
/// settled), as a coin of the group of asset `wrapper` creates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementGroupOutput {
    pub k: u32,
    pub wrapper: Option<Wrapper>,
    /// The bare one-time puzzle hash.
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    /// `wrap(W, puzzle_hash)`: the puzzle hash of the coin created.
    pub wrapped_puzzle_hash: Bytes32,
}

/// The result of [`derive_settlement_outputs`].
#[derive(Debug, Clone)]
pub struct SettlementDerivation {
    /// `wrap(W, p)` of each settled entry, in the order of the recipients: the
    /// puzzle hash of the coin the settlement coin of asset `W` must create.
    pub settlement_outputs: Vec<Bytes32>,
    /// The encoded payment of each settled entry, in the order of the
    /// recipients.
    pub settled_payments: Vec<Payment>,
    /// One notarized payment per asset of the party's requested payments, in
    /// order of first appearance (settled entries, then other payments).
    pub notarized_payments: Vec<SettlementNotarizedPayment>,
    /// One `ASSERT_PUZZLE_ANNOUNCEMENT` per asset, aligned with
    /// `notarized_payments`.
    pub assertions: Vec<AssertPuzzleAnnouncement>,
    /// The outputs of the entries that are not settled, in the order of the
    /// recipients.
    pub group_outputs: Vec<SettlementGroupOutput>,
}

/// CHIP-0057 procedure `DeriveSettlementOutputs`.
///
/// `group` is the party's spend group: one coin ID, coin puzzle hash,
/// synthetic secret key and wrapper (`None` for XCH) per coin
/// ([`SettlementGroupCoin`]). `recipients` are the party's recipient entries, `other_payments` its
/// requested payments to addresses that are not silent payment addresses, and
/// `nonce` the offer's nonce for its requested payments.
///
/// The derivation of `SendSilentPayment` runs once over every entry, with one
/// `k` sequence per scan key covering settled entries and the party's own
/// outputs alike, and `K_MAX` outputs at most per scan key. For each asset, in
/// order of first appearance, the payments are the settled entries' encoded
/// payments in the order of the recipients, then the asset's other payments
/// in list order; the notarized payment is `(nonce . payments)` and the
/// assertion is the settlement coin's puzzle announcement of it.
///
/// # Errors
///
/// - [`DriverError::SilentPaymentInvalidSettlementInput`] if the group is
///   empty.
/// - [`DriverError::SilentPaymentKeyNotSynthetic`] if a coin's secret key,
///   curried into the standard puzzle and wrapped in the coin's wrapper, does
///   not give the coin's puzzle hash (a raw wallet key, the key of another
///   coin, or the wrong wrapper).
/// - [`DriverError::SilentPayment`] with
///   [`SilentPaymentError::IdentityPublicKey`](chia_sdk_utils::silent_payments::SilentPaymentError::IdentityPublicKey)
///   if a recipient's scan key or spend key is the identity element, and with
///   [`SilentPaymentError::InvalidPublicKey`](chia_sdk_utils::silent_payments::SilentPaymentError::InvalidPublicKey)
///   if one is outside the prime-order subgroup (`SendSilentPayment` begins
///   with this check; the action system applies it through
///   [`SilentPaymentAddress::validate`]).
/// - [`DriverError::SilentPaymentAssetNotInGroup`] if an entry that is not
///   settled is paid in an asset that no coin of the group holds.
/// - [`DriverError::SilentPaymentTooManyOutputs`] if more than `K_MAX`
///   entries go to one scan key.
/// - The failures of `SendSilentPayment`: [`DriverError::SilentPaymentZeroKeySum`],
///   [`DriverError::SilentPaymentZeroInputHash`], [`DriverError::SilentPaymentZeroTweak`].
pub fn derive_settlement_outputs(
    ctx: &mut SpendContext,
    group: &[SettlementGroupCoin],
    recipients: &[SettlementRecipient],
    other_payments: &[OtherPayment],
    nonce: Bytes32,
) -> Result<SettlementDerivation, DriverError> {
    if group.is_empty() {
        return Err(DriverError::SilentPaymentInvalidSettlementInput);
    }
    // Each key must be the synthetic key of its coin: curried into the
    // standard puzzle and wrapped in the coin's wrapper, it gives the coin's
    // puzzle hash. `Spends::finish_with_keys` makes the same check on a
    // registered key against the p2 puzzle hash of a coin it spends.
    if !group.iter().all(SettlementGroupCoin::key_belongs_to_coin) {
        return Err(DriverError::SilentPaymentKeyNotSynthetic);
    }
    let sender_sks: Vec<SecretKey> = group
        .iter()
        .map(|coin| coin.secret_key.as_inner().clone())
        .collect();
    let coin_ids: Vec<Bytes32> = group.iter().map(|coin| coin.coin_id).collect();
    let coin_wrappers: Vec<Option<Wrapper>> = group.iter().map(|coin| coin.wrapper).collect();
    // SendSilentPayment fails if any B_scan or B_m is the identity element (or
    // outside the subgroup) before anything is derived: an identity scan key
    // makes the shared secret a public constant, and an identity spend key
    // makes the one-time key t_k * G, which anyone holding the scan key can
    // spend. This is the check the action system applies to an address; the
    // network does not take part in it.
    for recipient in recipients {
        SilentPaymentAddress::new(
            recipient.scan_pk,
            recipient.spend_pk,
            SilentPaymentNetwork::Mainnet,
        )
        .validate()?;
    }
    // The Fail step for an asset no coin of the group holds applies to the
    // entries the group creates only.
    if recipients
        .iter()
        .any(|r| !r.settled && !coin_wrappers.contains(&r.wrapper))
    {
        return Err(DriverError::SilentPaymentAssetNotInGroup);
    }

    let a_sum = aggregate_sender_sks(&sender_sks)?;
    let input_hash = compute_input_hash(&coin_ids, &a_sum.public_key());

    let mut counters: HashMap<[u8; 48], u32> = HashMap::new();
    let mut settlement_outputs = Vec::new();
    let mut settled_payments = Vec::new();
    let mut group_outputs = Vec::new();
    // The payments of each asset, in order of first appearance.
    let mut requested: Vec<(Option<Wrapper>, Vec<Payment>)> = Vec::new();

    for recipient in recipients {
        let counter = counters.entry(recipient.scan_pk.to_bytes()).or_insert(0);
        let k = *counter;
        if k >= K_MAX {
            return Err(DriverError::SilentPaymentTooManyOutputs);
        }
        *counter = k + 1;

        let p = derive_one_time_puzzle_hash(
            &recipient.scan_pk,
            &recipient.spend_pk,
            &a_sum,
            &input_hash,
            k,
        )?;
        let wrapper = recipient.wrapper;
        if recipient.settled {
            let payment = encoded_payment(ctx, p, recipient.amount, wrapper.as_ref())?;
            settlement_outputs.push(wrap_puzzle_hash(wrapper.as_ref(), p));
            settled_payments.push(payment.clone());
            push_payment(&mut requested, wrapper, payment);
        } else {
            group_outputs.push(SettlementGroupOutput {
                k,
                wrapper,
                puzzle_hash: p,
                amount: recipient.amount,
                wrapped_puzzle_hash: wrap_puzzle_hash(wrapper.as_ref(), p),
            });
        }
    }
    for other in other_payments {
        push_payment(&mut requested, other.wrapper, other.payment.clone());
    }

    let mut notarized_payments = Vec::with_capacity(requested.len());
    let mut assertions = Vec::with_capacity(requested.len());
    for (wrapper, payments) in requested {
        let notarized_payment = NotarizedPayment::new(nonce, payments);
        let assertion = settlement_assertion(ctx, wrapper.as_ref(), &notarized_payment);
        notarized_payments.push(SettlementNotarizedPayment {
            wrapper,
            notarized_payment,
            settlement_puzzle_hash: settlement_puzzle_hash(wrapper.as_ref()),
            announcement: assertion.announcement_id,
        });
        assertions.push(assertion);
    }

    Ok(SettlementDerivation {
        settlement_outputs,
        settled_payments,
        notarized_payments,
        assertions,
        group_outputs,
    })
}

/// Appends `payment` to the payments of its asset, adding the asset at the
/// end when it is new (order of first appearance).
fn push_payment(
    requested: &mut Vec<(Option<Wrapper>, Vec<Payment>)>,
    wrapper: Option<Wrapper>,
    payment: Payment,
) {
    if let Some((_, payments)) = requested.iter_mut().find(|(w, _)| *w == wrapper) {
        payments.push(payment);
    } else {
        requested.push((wrapper, vec![payment]));
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_utils::silent_payments::SilentPaymentError;
    use chia_sha2::Sha256;
    use clvmr::serde::node_to_bytes;
    use hex_literal::hex;

    use super::*;

    fn sha256(data: &[u8]) -> Bytes32 {
        let mut hasher = Sha256::new();
        hasher.update(data);
        Bytes32::new(hasher.finalize())
    }

    fn serialized(ctx: &mut SpendContext, payment: &Payment) -> Vec<u8> {
        let ptr = ctx.alloc(payment).unwrap();
        node_to_bytes(ctx, ptr).unwrap()
    }

    fn sk(byte: u8) -> SecretKey {
        SecretKey::from_bytes(&[byte; 32]).unwrap()
    }

    /// A coin of the group whose standard puzzle is curried with
    /// `secret_key`'s public key, as the key is synthetic, and wrapped in
    /// `wrapper`.
    fn group_coin(
        secret_key: &SecretKey,
        coin_id: Bytes32,
        wrapper: Option<Wrapper>,
    ) -> SettlementGroupCoin {
        SettlementGroupCoin {
            coin_id,
            puzzle_hash: wrap_puzzle_hash(
                wrapper.as_ref(),
                StandardArgs::curry_tree_hash(secret_key.public_key()).into(),
            ),
            secret_key: SyntheticSecretKey::from_synthetic_unchecked(secret_key.clone()),
            wrapper,
        }
    }

    /// One XCH coin with the key `sk(5)`.
    fn one_coin() -> [SettlementGroupCoin; 1] {
        [group_coin(&sk(5), Bytes32::new([1; 32]), None)]
    }

    fn recipient(
        scan: u8,
        spend: u8,
        wrapper: Option<Wrapper>,
        settled: bool,
    ) -> SettlementRecipient {
        SettlementRecipient {
            scan_pk: sk(scan).public_key(),
            spend_pk: sk(spend).public_key(),
            wrapper,
            settled,
            amount: 1000,
        }
    }

    /// XCH `(p amount ())`, CAT `(p amount (p))`; `Memos::None` would be
    /// `(p amount)`, which is never produced.
    #[test]
    fn payment_encodings() {
        let mut ctx = SpendContext::new();
        let p = Bytes32::new([0x11; 32]);
        let w = Wrapper::cat(Bytes32::new([0x22; 32]));

        let xch = encoded_payment(&mut ctx, p, 1000, None).unwrap();
        let mut expected = vec![0xff, 0xa0];
        expected.extend_from_slice(&p);
        expected.extend_from_slice(&[0xff, 0x82, 0x03, 0xe8, 0xff, 0x80, 0x80]);
        assert_eq!(serialized(&mut ctx, &xch), expected, "(p amount ())");

        let cat = encoded_payment(&mut ctx, p, 1000, Some(&w)).unwrap();
        let mut expected = vec![0xff, 0xa0];
        expected.extend_from_slice(&p);
        expected.extend_from_slice(&[0xff, 0x82, 0x03, 0xe8, 0xff, 0xff, 0xa0]);
        expected.extend_from_slice(&p);
        expected.extend_from_slice(&[0x80, 0x80]);
        assert_eq!(serialized(&mut ctx, &cat), expected, "(p amount (p))");

        let unhinted = Payment::new(p, 1000, Memos::None);
        let mut expected = vec![0xff, 0xa0];
        expected.extend_from_slice(&p);
        expected.extend_from_slice(&[0xff, 0x82, 0x03, 0xe8, 0x80]);
        assert_eq!(serialized(&mut ctx, &unhinted), expected, "(p amount)");
    }

    /// The settlement puzzle hash is the bare settlement puzzle for XCH, and
    /// the CAT-wrapped one for a CAT (Test Vector 12's asset).
    #[test]
    fn settlement_puzzle_hashes() {
        assert_eq!(
            settlement_puzzle_hash(None),
            Bytes32::from(SETTLEMENT_PAYMENT_HASH)
        );
        let w = Wrapper::cat(Bytes32::new(hex!(
            "3a4bb549cd71af49426e287cc910aaca72b2653bfae20540f24d666f4eb40d82"
        )));
        assert_eq!(
            settlement_puzzle_hash(Some(&w)),
            Bytes32::new(hex!(
                "36f8ca158558fd2049b08295ee4dff2b6b624ea1b6228e912c2dd608f5c5e9b4"
            ))
        );
    }

    /// The three encodings of one XCH payment give three announcements; the
    /// values are those of 13-SAGE-ENCODING.md "Values" (nonce = SHA256("nonce"),
    /// p = SHA256("p1"), amount 1000, the XCH settlement coin).
    #[test]
    fn announcement_per_encoding() {
        let mut ctx = SpendContext::new();
        let nonce = sha256(b"nonce");
        let p = sha256(b"p1");
        let chip = encoded_payment(&mut ctx, p, 1000, None).unwrap();
        let unhinted = Payment::new(p, 1000, Memos::None);
        let hinted = Payment::new(p, 1000, ctx.hint(p).unwrap());
        let announce = |ctx: &SpendContext, payment: Payment| {
            settlement_announcement(ctx, None, &NotarizedPayment::new(nonce, vec![payment]))
        };
        assert_eq!(
            announce(&ctx, chip),
            Bytes32::new(hex!(
                "db5b363ea63e03ae82aa0e473ca5400ea4a810fa083b5409e48718a7bc657ad5"
            ))
        );
        assert_eq!(
            announce(&ctx, unhinted),
            Bytes32::new(hex!(
                "b7cc4090216101f80906b85f1769b45f1657b4348ca230e382b393e401de6913"
            ))
        );
        assert_eq!(
            announce(&ctx, hinted),
            Bytes32::new(hex!(
                "9d6d75b4cf69ac88be80b79e3db31ebce33880729bc47c79453760004aa1dcac"
            ))
        );
    }

    /// One k sequence per scan key over settled entries and group outputs; the
    /// settled entries go to the notarized payment of their asset, the others
    /// to the group outputs.
    #[test]
    fn one_k_sequence_over_settled_and_group_outputs() {
        let mut ctx = SpendContext::new();
        let w = Wrapper::cat(Bytes32::new([0x33; 32]));
        let coin_ids = [Bytes32::new([1; 32]), Bytes32::new([2; 32])];
        let sks = [sk(5), sk(6)];
        let recipients = [
            recipient(7, 8, Some(w), true),
            recipient(7, 9, None, false),
            recipient(10, 11, None, true),
        ];
        let group = [
            group_coin(&sks[0], coin_ids[0], None),
            group_coin(&sks[1], coin_ids[1], None),
        ];
        let derivation =
            derive_settlement_outputs(&mut ctx, &group, &recipients, &[], Bytes32::new([4; 32]))
                .unwrap();

        let a_sum = aggregate_sender_sks(&sks).unwrap();
        let input_hash = compute_input_hash(&coin_ids, &a_sum.public_key());
        let p = |r: &SettlementRecipient, k| {
            derive_one_time_puzzle_hash(&r.scan_pk, &r.spend_pk, &a_sum, &input_hash, k).unwrap()
        };
        // Scan key 7: k 0 (settled CAT) and k 1 (group XCH); scan key 10: k 0.
        assert_eq!(
            derivation.settlement_outputs,
            vec![w.wrap(p(&recipients[0], 0)), p(&recipients[2], 0)]
        );
        assert_eq!(derivation.group_outputs.len(), 1);
        assert_eq!(derivation.group_outputs[0].k, 1);
        assert_eq!(
            derivation.group_outputs[0].puzzle_hash,
            p(&recipients[1], 1)
        );

        // Assets in order of first appearance: the CAT, then XCH.
        assert_eq!(derivation.notarized_payments.len(), 2);
        assert_eq!(derivation.assertions.len(), 2);
        assert_eq!(derivation.notarized_payments[0].wrapper, Some(w));
        assert_eq!(derivation.notarized_payments[1].wrapper, None);
        for (np, assertion) in derivation
            .notarized_payments
            .iter()
            .zip(&derivation.assertions)
        {
            assert_eq!(np.announcement, assertion.announcement_id);
            assert_eq!(
                np.announcement,
                settlement_announcement(&ctx, np.wrapper.as_ref(), &np.notarized_payment)
            );
        }
    }

    /// Other payments follow the settled entries of their asset, and an asset
    /// with other payments only gets its own notarized payment after them.
    #[test]
    fn other_payments_follow_the_settled_entries() {
        let mut ctx = SpendContext::new();
        let w = Wrapper::cat(Bytes32::new([0x33; 32]));
        let q = Bytes32::new([0x44; 32]);
        let other_xch = encoded_payment(&mut ctx, q, 250, None).unwrap();
        let other_cat = encoded_payment(&mut ctx, q, 300, Some(&w)).unwrap();
        let derivation = derive_settlement_outputs(
            &mut ctx,
            &one_coin(),
            &[recipient(7, 8, None, true)],
            &[
                OtherPayment {
                    wrapper: Some(w),
                    payment: other_cat,
                },
                OtherPayment {
                    wrapper: None,
                    payment: other_xch,
                },
            ],
            Bytes32::new([4; 32]),
        )
        .unwrap();
        assert_eq!(derivation.notarized_payments.len(), 2);
        let xch = &derivation.notarized_payments[0];
        assert_eq!(xch.wrapper, None);
        assert_eq!(xch.notarized_payment.payments.len(), 2);
        assert_eq!(
            xch.notarized_payment.payments[0],
            derivation.settled_payments[0]
        );
        assert_eq!(xch.notarized_payment.payments[1].puzzle_hash, q);
        assert_eq!(derivation.notarized_payments[1].wrapper, Some(w));
    }

    #[test]
    fn invalid_inputs_fail() {
        let mut ctx = SpendContext::new();
        let w = Wrapper::cat(Bytes32::new([0x33; 32]));
        let nonce = Bytes32::new([4; 32]);
        let entry = [recipient(7, 8, None, true)];

        let empty = derive_settlement_outputs(&mut ctx, &[], &entry, &[], nonce);
        assert!(matches!(
            empty,
            Err(DriverError::SilentPaymentInvalidSettlementInput)
        ));

        // A group output in an asset the group does not hold fails; a settled
        // entry in that asset does not.
        let group_cat = derive_settlement_outputs(
            &mut ctx,
            &one_coin(),
            &[recipient(7, 8, Some(w), false)],
            &[],
            nonce,
        );
        assert!(matches!(
            group_cat,
            Err(DriverError::SilentPaymentAssetNotInGroup)
        ));
        assert!(
            derive_settlement_outputs(
                &mut ctx,
                &one_coin(),
                &[recipient(7, 8, Some(w), true)],
                &[],
                nonce,
            )
            .is_ok()
        );
    }

    /// The key of each coin must be the synthetic key of the coin itself: the
    /// coin's puzzle hash is `wrap(W, curry_tree_hash(synthetic_pk))`, and the
    /// raw wallet key, the key of another coin, the wrong wrapper and a coin
    /// whose puzzle hash is not that key's are all refused before anything is
    /// derived, for an XCH coin, a CAT coin and a revocable CAT coin.
    #[test]
    fn keys_must_belong_to_their_coin() {
        let mut ctx = SpendContext::new();
        let nonce = Bytes32::new([4; 32]);
        let entry = [recipient(7, 8, None, true)];
        let derive = |ctx: &mut SpendContext, coin: SettlementGroupCoin| {
            derive_settlement_outputs(ctx, &[coin], &entry, &[], nonce)
        };
        let refused = |result: Result<SettlementDerivation, DriverError>| {
            matches!(result, Err(DriverError::SilentPaymentKeyNotSynthetic))
        };

        let raw = sk(5);
        let synthetic = SyntheticSecretKey::from_raw(&raw);
        let synthetic_p2: Bytes32 =
            StandardArgs::curry_tree_hash(synthetic.public_key().into_inner()).into();
        let raw_p2: Bytes32 = StandardArgs::curry_tree_hash(raw.public_key()).into();
        let raw_key = SyntheticSecretKey::from_synthetic_unchecked(raw.clone());
        let parent = Bytes32::new([1; 32]);
        let cat = Wrapper::cat(Bytes32::new([0x33; 32]));
        let revocable = Wrapper::revocable(Bytes32::new([0x33; 32]), Bytes32::new([0x66; 32]));

        for wrapper in [None, Some(cat), Some(revocable)] {
            let at = format!("{wrapper:?}");
            // The wallet's coin: at the puzzle hash of the synthetic key.
            let coin = Coin::new(
                parent,
                wrap_puzzle_hash(wrapper.as_ref(), synthetic_p2),
                1000,
            );

            // The synthetic key of the coin is accepted, and the entry has the
            // coin's ID.
            let entry_of_coin = SettlementGroupCoin::new(&coin, synthetic.clone(), wrapper);
            assert_eq!(entry_of_coin.coin_id, coin.coin_id(), "{at}");
            assert!(derive(&mut ctx, entry_of_coin).is_ok(), "{at}");

            // The raw wallet key of that coin is refused. Before the coin's
            // puzzle hash was checked, the raw key passed with a p2 puzzle
            // hash computed from it, `curry_tree_hash(raw_pk)`.
            assert!(
                refused(derive(
                    &mut ctx,
                    SettlementGroupCoin::new(&coin, raw_key.clone(), wrapper)
                )),
                "{at}: raw key"
            );

            // A coin whose puzzle hash is computed from the raw key accepts
            // the raw key, but it is another coin: its ID is not the wallet
            // coin's, so it cannot stand for the coin the offer spends.
            let from_raw = Coin::new(parent, wrap_puzzle_hash(wrapper.as_ref(), raw_p2), 1000);
            let entry_from_raw = SettlementGroupCoin::new(&from_raw, raw_key.clone(), wrapper);
            assert_ne!(entry_from_raw.coin_id, coin.coin_id(), "{at}");
            assert!(derive(&mut ctx, entry_from_raw).is_ok(), "{at}");

            // The key of another coin is refused.
            assert!(
                refused(derive(
                    &mut ctx,
                    SettlementGroupCoin::new(&coin, SyntheticSecretKey::from_raw(&sk(6)), wrapper)
                )),
                "{at}: another coin's key"
            );

            // A tampered puzzle hash is refused.
            let mut tampered = SettlementGroupCoin::new(&coin, synthetic.clone(), wrapper);
            tampered.puzzle_hash = Bytes32::new([0xab; 32]);
            assert!(refused(derive(&mut ctx, tampered)), "{at}: tampered");
            // So is the bare p2 puzzle hash given for a CAT coin.
            if wrapper.is_some() {
                let mut bare = SettlementGroupCoin::new(&coin, synthetic.clone(), wrapper);
                bare.puzzle_hash = synthetic_p2;
                assert!(refused(derive(&mut ctx, bare)), "{at}: bare p2");
            }

            // The wrong wrapper is refused: none, another asset, or the plain
            // CAT for the revocable CAT and the reverse.
            let other_asset = Wrapper::cat(Bytes32::new([0x44; 32]));
            let plain_or_revocable = if wrapper == Some(cat) {
                Some(revocable)
            } else {
                Some(cat)
            };
            for wrong in [None, Some(other_asset), plain_or_revocable] {
                if wrong == wrapper {
                    continue;
                }
                assert!(
                    refused(derive(
                        &mut ctx,
                        SettlementGroupCoin::new(&coin, synthetic.clone(), wrong)
                    )),
                    "{at}: wrapper {wrong:?}"
                );
            }
        }

        // Every coin of the group is checked, not only the first.
        let good = group_coin(&sk(5), Bytes32::new([1; 32]), None);
        let mut bad = group_coin(&sk(6), Bytes32::new([2; 32]), None);
        bad.puzzle_hash = good.puzzle_hash;
        assert!(refused(derive_settlement_outputs(
            &mut ctx,
            &[good, bad],
            &entry,
            &[],
            nonce
        )));
    }

    /// A curve point outside the prime-order subgroup.
    fn off_subgroup_point() -> PublicKey {
        (1..=u8::MAX)
            .find_map(|x| {
                let mut bytes = [0u8; 48];
                bytes[0] = 0x80;
                bytes[47] = x;
                PublicKey::from_bytes_unchecked(&bytes)
                    .ok()
                    .filter(|p| !p.is_valid() && !p.is_inf())
            })
            .expect("a curve point outside the prime-order subgroup")
    }

    /// CHIP-0057 `SendSilentPayment` fails if any `B_scan` or `B_m` is the
    /// identity element, and `DeriveSettlementOutputs` runs its derivation:
    /// an identity scan key or spend key is refused, settled or not, as is a
    /// key outside the subgroup, before anything is derived.
    #[test]
    fn identity_or_invalid_recipient_keys_fail() {
        let mut ctx = SpendContext::new();
        let nonce = Bytes32::new([4; 32]);
        let derive = |ctx: &mut SpendContext, entry: SettlementRecipient| {
            derive_settlement_outputs(ctx, &one_coin(), &[entry], &[], nonce)
        };

        for settled in [true, false] {
            let mut scan = recipient(7, 8, None, settled);
            scan.scan_pk = PublicKey::default();
            assert!(scan.scan_pk.is_inf());
            assert!(matches!(
                derive(&mut ctx, scan),
                Err(DriverError::SilentPayment(
                    SilentPaymentError::IdentityPublicKey
                ))
            ));

            let mut spend = recipient(7, 8, None, settled);
            spend.spend_pk = PublicKey::default();
            assert!(matches!(
                derive(&mut ctx, spend),
                Err(DriverError::SilentPayment(
                    SilentPaymentError::IdentityPublicKey
                ))
            ));

            let mut invalid = recipient(7, 8, None, settled);
            invalid.scan_pk = off_subgroup_point();
            assert!(matches!(
                derive(&mut ctx, invalid),
                Err(DriverError::SilentPayment(
                    SilentPaymentError::InvalidPublicKey
                ))
            ));

            assert!(derive(&mut ctx, recipient(7, 8, None, settled)).is_ok());
        }
    }

    /// `K_MAX` counts settled entries and group outputs to one scan key.
    #[test]
    fn k_max_counts_settled_entries() {
        let mut ctx = SpendContext::new();
        let mut entries = vec![recipient(7, 8, None, false); K_MAX as usize - 1];
        entries.push(recipient(7, 8, None, true));
        let nonce = Bytes32::new([4; 32]);
        let derivation =
            derive_settlement_outputs(&mut ctx, &one_coin(), &entries, &[], nonce).unwrap();
        assert_eq!(derivation.group_outputs.len(), K_MAX as usize - 1);
        assert_eq!(derivation.settlement_outputs.len(), 1);

        entries.push(recipient(7, 8, None, true));
        assert!(matches!(
            derive_settlement_outputs(&mut ctx, &one_coin(), &entries, &[], nonce),
            Err(DriverError::SilentPaymentTooManyOutputs)
        ));
    }
}
