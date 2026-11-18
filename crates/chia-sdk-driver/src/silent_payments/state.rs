//! The silent payment state of [`Spends`] and the derivation of the outputs when the
//! transaction is prepared.

use std::{collections::HashMap, mem};

use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    offer::{NotarizedPayment, Payment},
    standard::StandardArgs,
};
use chia_sdk_types::conditions::CreateCoin;
use indexmap::IndexMap;

use super::{
    SettledOtherPayment, SilentPaymentPending, SilentPaymentSource, SyntheticSecretKey, Wrapper,
    aggregate_sender_sks, compute_input_hash, derive_one_time_puzzle_hash, encoded_payment,
    settlement_assertion,
};
use crate::{
    Action, Asset, AssetInfo, CatAssetInfo, Deltas, DriverError, FeeOptions, FeeTransaction,
    Finished, Id, Outputs, Relation, SettlementPayer, SpendContext, SpendKind, SpendPosition,
    Spends, Unfinished, spend_with_minimum_fee,
};

/// The CHIP-0057 silent payments applied to [`Spends`]: the `k` counter of each scan key and the
/// outputs that are derived when the transaction is prepared. It holds no secret key.
#[derive(Debug, Clone, Default)]
pub struct SilentPaymentState {
    /// The revocable CATs the transaction requests with
    /// [`Action::silent_payment_requested`](crate::Action::silent_payment_requested). A requested CAT
    /// that is not listed is taken to be not revocable. Set it before the actions are applied.
    pub requested_asset_info: AssetInfo,
    /// The next `k` of each scan key, by its compressed public key.
    pub(crate) counters: HashMap<[u8; 48], u32>,
    pub(crate) pending: Vec<SilentPaymentPending>,
    pub(crate) settled_others: Vec<SettledOtherPayment>,
}

impl SilentPaymentState {
    /// Whether a silent payment action has been applied and its outputs are not derived yet.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.settled_others.is_empty()
    }
}

impl Spends<Unfinished> {
    /// [`Spends::prepare`] for a transaction with silent payment actions. `secret_keys` are the
    /// synthetic secret keys of the spend group by p2 puzzle hash: every XCH coin spent and the
    /// first CAT coin of each asset. They are only borrowed for the call, and a key that is not its
    /// coin's returns [`DriverError::SilentPaymentKeyNotSynthetic`].
    pub fn prepare_with_silent_payment_keys(
        mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        secret_keys: &IndexMap<Bytes32, SyntheticSecretKey>,
    ) -> Result<Spends<Finished>, DriverError> {
        // The outputs are emitted before the change, so that they keep the position a normal
        // payment would have.
        let group = if self.silent_payments.has_pending() {
            Some(derive_silent_payment_outputs(
                ctx,
                &mut self,
                deltas,
                relation,
                secret_keys,
            )?)
        } else {
            None
        };

        self.prepare_unfinished(ctx, deltas, relation)?;

        // A scanner would not find outputs derived from a group other than the bound one.
        if let Some(mut expected) = group {
            let mut actual: Vec<Bytes32> = self
                .iter_conditions_spends()
                .map(|(coin, _)| coin.coin_id())
                .collect();
            expected.sort_unstable();
            actual.sort_unstable();
            if expected != actual {
                return Err(DriverError::SilentPaymentInputSetChanged);
            }
        }

        Ok(self.into_finished())
    }

    /// [`Spends::finish_with_keys`] for a transaction with silent payment actions; see
    /// [`Spends::prepare_with_silent_payment_keys`].
    pub fn finish_with_silent_payment_keys(
        self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
        secret_keys: &IndexMap<Bytes32, SyntheticSecretKey>,
    ) -> Result<Outputs, DriverError> {
        self.prepare_with_silent_payment_keys(ctx, deltas, relation, secret_keys)?
            .spend_with_keys(ctx, synthetic_keys)
    }

    /// [`Spends::apply_with_fee`] for a transaction with silent payment actions. A coin selected
    /// for the fee joins the spend group, so `secret_keys` needs the key of every coin in
    /// `xch_coins`; see [`Spends::prepare_with_silent_payment_keys`]. The group's coins are linked
    /// on chain: when spending coins received at a silent payment address, pass only such coins.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_with_fee_and_silent_payment_keys<T>(
        &self,
        ctx: &mut SpendContext,
        actions: &[Action],
        xch_coins: &[Coin],
        fee: u64,
        relation: Relation,
        secret_keys: &IndexMap<Bytes32, SyntheticSecretKey>,
        spend_p2: impl FnOnce(&mut SpendContext, Spends<Finished>) -> Result<T, DriverError>,
    ) -> Result<T, DriverError> {
        self.apply_with_fee_using(
            ctx,
            actions,
            xch_coins,
            fee,
            relation,
            |spends, ctx, deltas, relation| {
                spends.prepare_with_silent_payment_keys(ctx, deltas, relation, secret_keys)
            },
            spend_p2,
        )
    }

    /// [`Spends::apply_with_minimum_fee`] for a transaction with silent payment actions; see
    /// [`Spends::apply_with_fee_and_silent_payment_keys`].
    #[allow(clippy::too_many_arguments)]
    pub fn apply_with_minimum_fee_and_silent_payment_keys<T>(
        &self,
        ctx: &mut SpendContext,
        actions: &[Action],
        xch_coins: &[Coin],
        relation: Relation,
        options: FeeOptions<'_>,
        secret_keys: &IndexMap<Bytes32, SyntheticSecretKey>,
        mut spend_p2: impl FnMut(&mut SpendContext, Spends<Finished>) -> Result<T, DriverError>,
    ) -> Result<FeeTransaction<T>, DriverError> {
        spend_with_minimum_fee(ctx, options, |ctx, fee| {
            self.apply_with_fee_and_silent_payment_keys(
                ctx,
                actions,
                xch_coins,
                fee,
                relation,
                secret_keys,
                &mut spend_p2,
            )
        })
    }
}

/// Derives and emits the pending silent payment outputs from the spend group, the coins
/// [`Relation::AssertConcurrent`] binds (CHIP-0057 "Inputs for Shared Secret Derivation", "Spend
/// Groups across Assets"). Returns the coin ids of the group.
fn derive_silent_payment_outputs(
    ctx: &mut SpendContext,
    spends: &mut Spends,
    deltas: &Deltas,
    relation: Relation,
    secret_keys: &IndexMap<Bytes32, SyntheticSecretKey>,
) -> Result<Vec<Bytes32>, DriverError> {
    // CHIP-0057 "Eligible Spends": a cycle through any other spend would not bind the coins on
    // either side of it.
    if !spends.dids.is_empty() || !spends.nfts.is_empty() || !spends.options.is_empty() {
        return Err(DriverError::SilentPaymentIneligibleAssetBundle);
    }
    if spends
        .cats
        .values()
        .any(|cat| cat.items.iter().any(|item| item.revoke))
    {
        return Err(DriverError::SilentPaymentIssuerPathSpend);
    }
    if spends.cats.keys().any(|id| matches!(id, Id::New(_))) {
        return Err(DriverError::SilentPaymentNewCatUnsupported);
    }

    // Creating the change can add an intermediate coin to the group, so the change is created on
    // a copy first to learn the final set.
    let group: Vec<SilentPaymentGroupCoin> = {
        let mut trial = spends.clone();
        trial.create_change(ctx, deltas)?;
        trial
            .conditions_spend_positions()
            .into_iter()
            .map(|position| trial.silent_payment_group_coin(position))
            .collect::<Option<Vec<_>>>()
            .ok_or(DriverError::SilentPaymentIneligibleAssetBundle)?
    };

    // Without the cycle, the coins would be separate single-input groups to a scanner.
    if group.len() >= 2 && !matches!(relation, Relation::AssertConcurrent) {
        return Err(DriverError::SilentPaymentRequiresInputBinding);
    }

    if group.is_empty() {
        return Err(DriverError::SilentPaymentNoXchInputs);
    }

    // One secret key and one coin id per coin in the group, even when coins share a key.
    let mut group_coin_ids: Vec<Bytes32> = Vec::with_capacity(group.len());
    let mut sender_sks: Vec<SecretKey> = Vec::with_capacity(group.len());
    for coin in &group {
        let Some(sk) = secret_keys.get(&coin.p2_puzzle_hash) else {
            return Err(if coin.ephemeral {
                DriverError::SilentPaymentIntermediateKeyMissing
            } else {
                DriverError::SilentPaymentMultiPartyUnsupported
            });
        };
        // The key must curry to the coin's (inner) puzzle hash, which also proves the coin is
        // locked to the standard puzzle.
        let sk = sk.as_inner();
        if Bytes32::from(StandardArgs::curry_tree_hash(sk.public_key())) != coin.p2_puzzle_hash {
            return Err(DriverError::SilentPaymentKeyNotSynthetic);
        }
        sender_sks.push(sk.clone());
        group_coin_ids.push(coin.coin_id);
    }

    // Every output that is not settled must be created by a coin of the group of the output's
    // own asset (CHIP-0057 sender requirement 4). A settled output is created by a settlement
    // coin, which is never in the group.
    if spends
        .silent_payments
        .pending
        .iter()
        .filter(|pending| !matches!(pending.source, SilentPaymentSource::Settled { .. }))
        .any(|pending| {
            !group.iter().any(|coin| {
                pending
                    .parent_coin
                    .is_some_and(|parent| parent.coin_id() == coin.coin_id)
                    && match pending.source {
                        SilentPaymentSource::Xch(_) => coin.wrapper.is_none(),
                        SilentPaymentSource::Cat(id, _) => coin
                            .wrapper
                            .is_some_and(|wrapper| Id::Existing(wrapper.asset_id) == id),
                        SilentPaymentSource::Settled { .. } => false,
                    }
            })
        })
    {
        return Err(DriverError::SilentPaymentParentNotEligible);
    }

    // Fails if the keys sum to zero mod r.
    let aggregated_sender_sk = aggregate_sender_sks(&sender_sks)?;
    let agg_pk = aggregated_sender_sk.public_key();

    let input_hash = compute_input_hash(&group_coin_ids, &agg_pk)?;
    if input_hash.is_zero() {
        return Err(DriverError::SilentPaymentZeroInputHash);
    }

    let pending = mem::take(&mut spends.silent_payments.pending);
    let mut settled: Vec<SettledPayment> = Vec::new();

    for p in &pending {
        let ph = derive_one_time_puzzle_hash(
            &p.scan_pk,
            &p.spend_pk,
            &aggregated_sender_sk,
            &input_hash,
            p.k,
        )?;

        match p.source {
            SilentPaymentSource::Xch(index) => {
                let create_coin = CreateCoin::new(ph, p.amount, p.memos);
                let parent_coin = p
                    .parent_coin
                    .ok_or(DriverError::SilentPaymentParentNotEligible)?;

                let parent = &mut spends.xch.items[index];
                parent.kind.create_coin_with_assertion(
                    ctx,
                    parent_coin,
                    &mut spends.xch.payment_assertions,
                    create_coin,
                );

                spends
                    .outputs
                    .xch
                    .push(Coin::new(parent_coin.coin_id(), ph, p.amount));
            }
            SilentPaymentSource::Cat(id, index) => {
                // CHIP-0057 "Outputs in CATs": the bare one-time puzzle hash, hinted with it.
                let create_coin = CreateCoin::new(ph, p.amount, ctx.hint(ph)?);
                let parent_coin = p
                    .parent_coin
                    .ok_or(DriverError::SilentPaymentParentNotEligible)?;

                let cat = spends
                    .cats
                    .get_mut(&id)
                    .ok_or(DriverError::SilentPaymentAssetNotInGroup)?;
                let parent = cat
                    .items
                    .get_mut(index)
                    .ok_or(DriverError::SilentPaymentParentNotEligible)?;
                parent.kind.create_coin_with_assertion(
                    ctx,
                    parent_coin,
                    &mut cat.payment_assertions,
                    create_coin,
                );

                let child = parent.asset.child(ph, p.amount);
                spends.outputs.cats.entry(id).or_default().push(child);
            }
            SilentPaymentSource::Settled {
                asset,
                nonce,
                payer,
                settlement_index,
            } => {
                let payment = encoded_payment(ctx, ph, p.amount, asset.as_ref())?;
                settled.push(SettledPayment {
                    asset,
                    nonce,
                    payer,
                    settlement_index,
                    payment,
                });
            }
        }
    }

    let others = mem::take(&mut spends.silent_payments.settled_others);
    if !settled.is_empty() || !others.is_empty() {
        settle_silent_payments(ctx, spends, settled, others)?;
    }

    Ok(group_coin_ids)
}

/// A settled silent payment of the party, encoded, in the order of the actions.
struct SettledPayment {
    asset: Option<Wrapper>,
    nonce: Bytes32,
    payer: SettlementPayer,
    settlement_index: Option<usize>,
    payment: Payment,
}

/// The party's requested payments in one asset: one payer, and the payments in the order they
/// are listed (silent payments first, then other payments).
struct AssetSettlement {
    asset: Option<Wrapper>,
    payer: SettlementPayer,
    settlement_index: Option<usize>,
    payments: Vec<Payment>,
}

/// Groups the party's settled payments per asset into one notarized payment each and asserts its
/// announcement (CHIP-0057 `DeriveSettlementOutputs`).
fn settle_silent_payments(
    ctx: &mut SpendContext,
    spends: &mut Spends,
    settled: Vec<SettledPayment>,
    others: Vec<SettledOtherPayment>,
) -> Result<(), DriverError> {
    // One nonce for all of the party's requested payments.
    let nonce = settled
        .first()
        .map(|entry| entry.nonce)
        .or_else(|| others.first().map(|other| other.nonce))
        .ok_or(DriverError::SilentPaymentInvalidSettlementInput)?;
    if settled.iter().any(|entry| entry.nonce != nonce)
        || others.iter().any(|other| other.nonce != nonce)
    {
        return Err(DriverError::SilentPaymentSettlementMismatch);
    }

    // Assets in order of first appearance, those of the silent payments first; an other payment
    // joins its asset after the asset's silent payments.
    let mut assets: Vec<AssetSettlement> = Vec::new();
    for entry in settled {
        if let Some(asset) = assets.iter_mut().find(|a| a.asset == entry.asset) {
            if asset.payer != entry.payer {
                return Err(DriverError::SilentPaymentSettlementMismatch);
            }
            asset.payments.push(entry.payment);
        } else {
            assets.push(AssetSettlement {
                asset: entry.asset,
                payer: entry.payer,
                settlement_index: entry.settlement_index,
                payments: vec![entry.payment],
            });
        }
    }
    for other in others {
        let Some(asset) = assets.iter_mut().find(|a| a.asset == other.asset) else {
            assets.push(AssetSettlement {
                asset: other.asset,
                payer: other.payer,
                settlement_index: other.settlement_index,
                payments: vec![other.payment],
            });
            continue;
        };
        if asset.payer != other.payer {
            return Err(DriverError::SilentPaymentSettlementMismatch);
        }
        asset.settlement_index = asset.settlement_index.or(other.settlement_index);
        asset.payments.push(other.payment);
    }

    for asset in assets {
        let notarized_payment = NotarizedPayment::new(nonce, asset.payments);
        let assertion = settlement_assertion(ctx, asset.asset.as_ref(), &notarized_payment);
        spends.conditions.required = mem::take(&mut spends.conditions.required).with(assertion);

        match asset.payer {
            SettlementPayer::Counterparty => match asset.asset {
                None => spends
                    .outputs
                    .silent_payment_requests
                    .xch
                    .push(notarized_payment),
                Some(wrapper) => {
                    spends
                        .outputs
                        .silent_payment_requests
                        .cats
                        .entry(wrapper.asset_id)
                        .or_default()
                        .push(notarized_payment);
                    spends.outputs.silent_payment_asset_info.insert_cat(
                        wrapper.asset_id,
                        CatAssetInfo::new(wrapper.hidden_puzzle_hash),
                    )?;
                }
            },
            SettlementPayer::SpentSettlementCoin => {
                let index = asset
                    .settlement_index
                    .ok_or(DriverError::SilentPaymentSettlementCoinMissing)?;
                match asset.asset {
                    None => {
                        let item = spends
                            .xch
                            .items
                            .get_mut(index)
                            .ok_or(DriverError::SilentPaymentSettlementCoinMissing)?;
                        let SpendKind::Settlement(spend) = &mut item.kind else {
                            return Err(DriverError::CannotSettleFromSpend);
                        };
                        for payment in &notarized_payment.payments {
                            spends.outputs.xch.push(Coin::new(
                                item.asset.coin_id(),
                                payment.puzzle_hash,
                                payment.amount,
                            ));
                        }
                        spend.add_notarized_payment(notarized_payment);
                    }
                    Some(wrapper) => {
                        let id = Id::Existing(wrapper.asset_id);
                        let item = spends
                            .cats
                            .get_mut(&id)
                            .and_then(|cat| cat.items.get_mut(index))
                            .ok_or(DriverError::SilentPaymentSettlementCoinMissing)?;
                        let SpendKind::Settlement(spend) = &mut item.kind else {
                            return Err(DriverError::CannotSettleFromSpend);
                        };
                        for payment in &notarized_payment.payments {
                            let child = item.asset.child(payment.puzzle_hash, payment.amount);
                            spends.outputs.cats.entry(id).or_default().push(child);
                        }
                        spend.add_notarized_payment(notarized_payment);
                    }
                }
            }
        }
    }

    Ok(())
}

/// A coin of the silent payment spend group.
struct SilentPaymentGroupCoin {
    coin_id: Bytes32,
    /// The puzzle hash of the coin's standard puzzle (the inner puzzle of a CAT).
    p2_puzzle_hash: Bytes32,
    ephemeral: bool,
    /// `None` for an XCH coin, the coin's wrapper for a CAT coin.
    wrapper: Option<Wrapper>,
}

impl Spends<Unfinished> {
    /// The spend at `position` as a coin of the silent payment spend group, or `None` for a
    /// position that is not an XCH or CAT spend.
    fn silent_payment_group_coin(&self, position: SpendPosition) -> Option<SilentPaymentGroupCoin> {
        match position {
            SpendPosition::Xch(index) => {
                let item = self.xch.items.get(index)?;
                Some(SilentPaymentGroupCoin {
                    coin_id: item.asset.coin_id(),
                    p2_puzzle_hash: item.p2_puzzle_hash(),
                    ephemeral: item.ephemeral,
                    wrapper: None,
                })
            }
            SpendPosition::Cat(id, index) => {
                let item = self.cats.get(&id)?.items.get(index)?;
                Some(SilentPaymentGroupCoin {
                    coin_id: item.asset.coin_id(),
                    p2_puzzle_hash: item.p2_puzzle_hash(),
                    ephemeral: item.ephemeral,
                    wrapper: Some(Wrapper {
                        asset_id: item.asset.info.asset_id,
                        hidden_puzzle_hash: item.asset.info.hidden_puzzle_hash,
                    }),
                })
            }
            SpendPosition::Did(..) | SpendPosition::Nft(..) | SpendPosition::OptionContract(..) => {
                None
            }
        }
    }
}
