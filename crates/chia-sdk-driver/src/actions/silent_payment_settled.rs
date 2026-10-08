use chia_protocol::Bytes32;
use chia_puzzle_types::{Memos, offer::Payment};
use chia_sdk_utils::silent_payments::SilentPaymentAddress;

use super::silent_payment_send::{next_k, record_pending};
use crate::{
    Deltas, DriverError, Id, SpendAction, SpendContext, Spends,
    silent_payments::{SettledOtherPayment, SilentPaymentSource, Wrapper},
};

/// Who creates a settled payment (CHIP-0057 "Payments through Settlement
/// Coins").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementPayer {
    /// The counterparty's settlement coin, which this transaction does not
    /// spend: the party is the maker of an offer, and its requested payments
    /// are published in [`crate::Outputs::silent_payment_requests`] for the
    /// offer file.
    Counterparty,
    /// A settlement coin of the payment's asset that this transaction spends:
    /// the party is the taker of an offer and is paid from the coins the
    /// maker offered. The amount counts against that asset, as
    /// [`crate::Action::settle`] does.
    SpentSettlementCoin,
}

/// A requested payment of a party, paid through a settlement coin.
///
/// The silent payment variant holds the address inline, as
/// [`crate::SilentPaymentSendAction`] does.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum SettledEntry {
    /// A payment to a silent payment address. Its one-time puzzle hash is
    /// derived from the party's spend group, with the `k` counter of the
    /// address's scan key, when the transaction is prepared.
    SilentPayment {
        recipient: SilentPaymentAddress,
        amount: u64,
    },
    /// A requested payment to an address that is not a silent payment address,
    /// as the party lists it in its offer. It is listed in the notarized
    /// payment of its asset after the asset's silent payments.
    Other(Payment),
}

/// CHIP-0057 silent payment through a settlement coin (chip-0057-gated),
/// created by [`Action::silent_payment_requested`](crate::Action::silent_payment_requested),
/// [`Action::silent_payment_requested_other`](crate::Action::silent_payment_requested_other),
/// [`Action::silent_payment_settle`](crate::Action::silent_payment_settle) and
/// [`Action::silent_payment_settle_other`](crate::Action::silent_payment_settle_other).
///
/// When the transaction is prepared, the party's settled entries are grouped
/// per asset (`asset`, `None` for XCH) in order of first appearance: the
/// asset's silent payments in the order of the actions, then its other
/// payments, as one notarized payment `(nonce . payments)`. An XCH silent
/// payment is listed as `(p amount ())` and a CAT one as `(p amount (p))`. A
/// coin of the party's spend group asserts the settlement coin's puzzle
/// announcement of that notarized payment, as a required condition: it does not
/// depend on [`crate::ConditionConfig::disable_settlement_assertions`].
///
/// Every entry of a party carries the same `nonce`, the offer's nonce for its
/// requested payments, and the entries of one asset have one payer
/// ([`DriverError::SilentPaymentSettlementMismatch`] otherwise). A silent
/// payment takes the next `k` of its scan key, shared with every other silent
/// payment output of the transaction, and counts towards `K_MAX`.
#[derive(Debug, Clone)]
pub struct SilentPaymentSettledAction {
    /// The asset of the payment: `None` for XCH, or the CAT's wrapper (with
    /// the hidden puzzle hash of a revocable CAT).
    pub asset: Option<Wrapper>,
    /// The offer's nonce for the party's requested payments.
    pub nonce: Bytes32,
    pub entry: SettledEntry,
    pub payer: SettlementPayer,
}

impl SilentPaymentSettledAction {
    pub fn new(
        asset: Option<Wrapper>,
        nonce: Bytes32,
        entry: SettledEntry,
        payer: SettlementPayer,
    ) -> Self {
        Self {
            asset,
            nonce,
            entry,
            payer,
        }
    }

    /// The asset's id in the action system: [`Id::Xch`] or
    /// [`Id::Existing`] with the CAT's asset ID.
    pub fn id(&self) -> Id {
        self.asset.map_or(Id::Xch, |w| Id::Existing(w.asset_id))
    }

    /// The amount of the entry.
    pub fn amount(&self) -> u64 {
        match &self.entry {
            SettledEntry::SilentPayment { amount, .. } => *amount,
            SettledEntry::Other(payment) => payment.amount,
        }
    }
}

impl SpendAction for SilentPaymentSettledAction {
    fn calculate_delta(&self, _deltas: &mut Deltas, _index: usize) {
        // A payment the counterparty's settlement coin makes moves none of the
        // party's own value.
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        if let SettledEntry::SilentPayment { recipient, .. } = &self.entry {
            recipient.validate()?;
        }

        let settlement_index = match self.payer {
            SettlementPayer::Counterparty => None,
            SettlementPayer::SpentSettlementCoin => {
                return Err(DriverError::SilentPaymentSettlementCoinMissing);
            }
        };

        match &self.entry {
            SettledEntry::SilentPayment { recipient, amount } => {
                let k = next_k(spends, &recipient.scan_pk)?;
                record_pending(
                    spends,
                    recipient,
                    k,
                    SilentPaymentSource::Settled {
                        asset: self.asset,
                        nonce: self.nonce,
                        payer: self.payer,
                        settlement_index,
                    },
                    None,
                    *amount,
                    Memos::None,
                );
            }
            SettledEntry::Other(payment) => {
                spends
                    .silent_payment_settled_others
                    .push(SettledOtherPayment {
                        asset: self.asset,
                        nonce: self.nonce,
                        payer: self.payer,
                        settlement_index,
                        payment: payment.clone(),
                    });
            }
        }

        Ok(())
    }
}
