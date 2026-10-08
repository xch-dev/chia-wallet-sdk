use chia_protocol::Bytes32;
use chia_puzzle_types::{Memos, offer::Payment};
use chia_sdk_utils::silent_payments::SilentPaymentAddress;

use super::silent_payment_send::{next_k, record_pending};
use chia_protocol::Coin;

use crate::{
    Asset, Delta, Deltas, DriverError, Id, SpendAction, SpendContext, Spends,
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
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        // A payment the counterparty's settlement coin makes moves none of the
        // party's own value. One made by a settlement coin the transaction
        // spends is paid out of that asset, as `SettleAction` counts it.
        if self.payer == SettlementPayer::SpentSettlementCoin {
            *deltas.update(self.id()) += Delta::new(0, self.amount());
            deltas.set_needed(self.id());
        }
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

        let (settlement_index, parent_coin) = match self.payer {
            SettlementPayer::Counterparty => (None, None),
            SettlementPayer::SpentSettlementCoin => {
                let (index, coin) = settlement_source(spends, self.asset.as_ref())?;
                (Some(index), Some(coin))
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
                    parent_coin,
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

/// The first settlement coin of the asset (`None` for XCH) that the transaction spends, as its
/// position in the asset's items and its coin. For a CAT the settlement coin's hidden puzzle hash
/// must be the wrapper's, so a revocable CAT is settled by a revocable settlement coin.
fn settlement_source(
    spends: &Spends,
    asset: Option<&Wrapper>,
) -> Result<(usize, Coin), DriverError> {
    let found = match asset {
        None => spends
            .xch
            .items
            .iter()
            .position(|item| item.kind.is_settlement())
            .map(|index| (index, spends.xch.items[index].asset.coin())),
        Some(wrapper) => spends
            .cats
            .get(&Id::Existing(wrapper.asset_id))
            .and_then(|cat| {
                cat.items
                    .iter()
                    .position(|item| {
                        item.kind.is_settlement()
                            && item.asset.info.hidden_puzzle_hash == wrapper.hidden_puzzle_hash
                    })
                    .map(|index| (index, cat.items[index].asset.coin))
            }),
    };
    found.ok_or(DriverError::SilentPaymentSettlementCoinMissing)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_bls::SecretKey;
    use chia_protocol::Bytes32;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::silent_payments::K_MAX;
    use chia_sdk_utils::silent_payments::{SilentPaymentAddress, SilentPaymentNetwork};

    use crate::{
        Action, DriverError, SettlementPayer, SpendContext, Spends,
        silent_payments::{SilentPaymentSource, Wrapper},
    };

    /// CHIP-0057 "Payments through Settlement Coins": the counter `k`,
    /// `K_MAX` and sender requirement 7 apply to the party's whole spend
    /// group, so settled payments and the group's own outputs to one scan key
    /// share one sequence of `k` and one limit.
    #[test]
    fn test_k_max_counts_settled_outputs() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(10_000);
        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );
        let w = Wrapper::cat(Bytes32::new([0x33; 32]));
        let nonce = Bytes32::new([0x55; 32]);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        // K_MAX - 1 outputs of the group and one settled payment are accepted.
        let mut actions: Vec<Action> = (1..K_MAX)
            .map(|_| Action::silent_payment_send(recipient.clone(), 1, Memos::None))
            .collect();
        actions.push(Action::silent_payment_requested(
            Some(w),
            recipient.clone(),
            1,
            nonce,
        ));
        spends.apply(&mut ctx, &actions)?;
        assert_eq!(spends.silent_payments_pending.len(), K_MAX as usize);
        let last = spends.silent_payments_pending.last().unwrap();
        assert_eq!(last.k, K_MAX - 1);
        assert_eq!(
            last.source,
            SilentPaymentSource::Settled {
                asset: Some(w),
                nonce,
                payer: SettlementPayer::Counterparty,
                settlement_index: None,
            }
        );
        assert_eq!(last.parent_coin, None);

        // One more output to the same scan key fails, settled or not.
        for action in [
            Action::silent_payment_requested(None, recipient.clone(), 1, nonce),
            Action::silent_payment_send(recipient.clone(), 1, Memos::None),
        ] {
            let result = spends.apply(&mut ctx, &[action]);
            assert!(
                matches!(result, Err(DriverError::SilentPaymentTooManyOutputs)),
                "expected SilentPaymentTooManyOutputs, got {result:?}"
            );
        }
        assert_eq!(spends.silent_payments_pending.len(), K_MAX as usize);
        Ok(())
    }
}
