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
        /// The silent payment address that is paid.
        recipient: SilentPaymentAddress,
        /// The amount of the payment, in the smallest unit of the asset.
        amount: u64,
    },
    /// A requested payment to an address that is not a silent payment address,
    /// as the party lists it in its offer. It is listed in the notarized
    /// payment of its asset after the asset's silent payments.
    Other(Payment),
}

/// A CHIP-0057 payment through a settlement coin, created by
/// [`Action::silent_payment_requested`](crate::Action::silent_payment_requested),
/// [`Action::silent_payment_requested_other`](crate::Action::silent_payment_requested_other),
/// [`Action::silent_payment_settle`](crate::Action::silent_payment_settle) and
/// [`Action::silent_payment_settle_other`](crate::Action::silent_payment_settle_other).
///
/// When the transaction is prepared, the party's entries are grouped per asset
/// into one notarized payment each (CHIP-0057 `DeriveSettlementOutputs`), and a
/// coin of the party's spend group asserts its announcement as a required
/// condition.
///
/// Every entry of a party carries the same `nonce`, and the entries of one
/// asset have one payer ([`DriverError::SilentPaymentSettlementMismatch`]
/// otherwise). A silent payment takes the next `k` of its scan key, shared with
/// every other silent payment output of the transaction.
#[derive(Debug, Clone)]
pub struct SilentPaymentSettledAction {
    /// The asset of the payment: [`Id::Xch`], or [`Id::Existing`] with the
    /// asset ID of a CAT.
    ///
    /// The hidden puzzle hash of a revocable CAT is taken from the settlement
    /// coin the transaction spends ([`SettlementPayer::SpentSettlementCoin`]),
    /// or from [`crate::SilentPaymentState::requested_asset_info`]
    /// ([`SettlementPayer::Counterparty`]).
    pub id: Id,
    /// The offer's nonce for the party's requested payments.
    pub nonce: Bytes32,
    /// The requested payment.
    pub entry: SettledEntry,
    /// The settlement coin that makes the payment.
    pub payer: SettlementPayer,
}

impl SilentPaymentSettledAction {
    /// Creates a settled payment of `entry` in the asset `id` under `nonce`, paid by `payer`.
    pub fn new(id: Id, nonce: Bytes32, entry: SettledEntry, payer: SettlementPayer) -> Self {
        Self {
            id,
            nonce,
            entry,
            payer,
        }
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
            *deltas.update(self.id) += Delta::new(0, self.amount());
            deltas.set_needed(self.id);
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

        let (asset, settlement_index, parent_coin) = match self.payer {
            SettlementPayer::Counterparty => {
                let asset = match self.id {
                    Id::Xch => None,
                    Id::Existing(asset_id) => Some(Wrapper {
                        asset_id,
                        hidden_puzzle_hash: spends
                            .silent_payments
                            .requested_asset_info
                            .cat(asset_id)
                            .and_then(|info| info.hidden_puzzle_hash),
                    }),
                    Id::New(_) => return Err(DriverError::SilentPaymentNewCatUnsupported),
                };
                (asset, None, None)
            }
            SettlementPayer::SpentSettlementCoin => {
                let (asset, index, coin) = settlement_source(spends, self.id)?;
                (asset, Some(index), Some(coin))
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
                        asset,
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
                    .silent_payments
                    .settled_others
                    .push(SettledOtherPayment {
                        asset,
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

/// The first settlement coin of the asset that the transaction spends: the asset as a wrapper
/// (`None` for XCH, with the coin's hidden puzzle hash for a revocable CAT), the coin's position
/// in the asset's items and the coin.
fn settlement_source(
    spends: &Spends,
    id: Id,
) -> Result<(Option<Wrapper>, usize, Coin), DriverError> {
    let found = match id {
        Id::Xch => spends
            .xch
            .items
            .iter()
            .position(|item| item.kind.is_settlement())
            .map(|index| (None, index, spends.xch.items[index].asset.coin())),
        Id::Existing(_) => spends.cats.get(&id).and_then(|cat| {
            cat.items
                .iter()
                .position(|item| item.kind.is_settlement())
                .map(|index| {
                    let cat = &cat.items[index].asset;
                    let wrapper = Wrapper {
                        asset_id: cat.info.asset_id,
                        hidden_puzzle_hash: cat.info.hidden_puzzle_hash,
                    };
                    (Some(wrapper), index, cat.coin)
                })
        }),
        Id::New(_) => return Err(DriverError::SilentPaymentNewCatUnsupported),
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
        Action, DriverError, Id, SettlementPayer, SpendContext, Spends,
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
            .map(|_| Action::silent_payment_send(Id::Xch, recipient.clone(), 1, Memos::None))
            .collect();
        actions.push(Action::silent_payment_requested(
            Id::Existing(w.asset_id),
            recipient.clone(),
            1,
            nonce,
        ));
        spends.apply(&mut ctx, &actions)?;
        assert_eq!(spends.silent_payments.pending.len(), K_MAX as usize);
        let last = spends.silent_payments.pending.last().unwrap();
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
            Action::silent_payment_requested(Id::Xch, recipient.clone(), 1, nonce),
            Action::silent_payment_send(Id::Xch, recipient.clone(), 1, Memos::None),
        ] {
            let result = spends.apply(&mut ctx, &[action]);
            assert!(
                matches!(result, Err(DriverError::SilentPaymentTooManyOutputs)),
                "expected SilentPaymentTooManyOutputs, got {result:?}"
            );
        }
        assert_eq!(spends.silent_payments.pending.len(), K_MAX as usize);
        Ok(())
    }

    /// The assets are named by `Id`. The hidden puzzle hash of a revocable CAT
    /// comes from the settlement coin the transaction spends (the taker), or
    /// from the requested asset info (the maker).
    #[test]
    fn test_revocable_cat_hidden_puzzle_hash_sources() -> Result<()> {
        use chia_protocol::Coin;
        use chia_puzzles::SETTLEMENT_PAYMENT_HASH;

        use crate::{Cat, CatAssetInfo, CatInfo};

        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(10);
        let recipient = SilentPaymentAddress::new(
            SecretKey::from_bytes(&[0x42u8; 32])?.public_key(),
            SecretKey::from_bytes(&[0x43u8; 32])?.public_key(),
            SilentPaymentNetwork::Mainnet,
        );
        let asset_id = Bytes32::new([0x33; 32]);
        let hidden = Bytes32::new([7; 32]);
        let nonce = Bytes32::new([0x55; 32]);
        let source = |spends: &Spends| spends.silent_payments.pending.last().unwrap().source;

        // The taker: from the revocable settlement coin.
        let info = CatInfo::new(asset_id, Some(hidden), SETTLEMENT_PAYMENT_HASH.into());
        let settlement = Cat::new(
            Coin::new(Bytes32::new([0x11; 32]), info.puzzle_hash().into(), 1_000),
            None,
            info,
        );
        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);
        spends.add(settlement);
        spends.apply(
            &mut ctx,
            &[Action::silent_payment_settle(
                Id::Existing(asset_id),
                recipient.clone(),
                100,
                nonce,
            )],
        )?;
        assert!(matches!(
            source(&spends),
            SilentPaymentSource::Settled { asset: Some(w), .. } if w == Wrapper::revocable(asset_id, hidden)
        ));

        // The maker: not revocable unless the requested asset info says so.
        for expected in [Wrapper::cat(asset_id), Wrapper::revocable(asset_id, hidden)] {
            let mut spends = Spends::new(alice.puzzle_hash);
            spends.add(alice.coin);
            if let Some(hidden) = expected.hidden_puzzle_hash {
                spends
                    .silent_payments
                    .requested_asset_info
                    .insert_cat(asset_id, CatAssetInfo::new(Some(hidden)))?;
            }
            spends.apply(
                &mut ctx,
                &[Action::silent_payment_requested(
                    Id::Existing(asset_id),
                    recipient.clone(),
                    100,
                    nonce,
                )],
            )?;
            assert!(matches!(
                source(&spends),
                SilentPaymentSource::Settled { asset: Some(w), .. } if w == expected
            ));
        }
        Ok(())
    }
}
