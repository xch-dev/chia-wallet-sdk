use std::{
    collections::{HashMap, HashSet},
    mem,
};

use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_sdk_types::{Conditions, announcement_id, conditions::AssertPuzzleAnnouncement};
use indexmap::IndexMap;

use crate::{
    Action, Asset, Cat, CatSpend, ConditionsSpend, Delta, Deltas, Did, DriverError, FungibleSpend,
    FungibleSpends, Id, Layer, Nft, OptionContract, Relation, SettlementLayer, SingletonSpends,
    Spend, SpendAction, SpendContext, SpendKind, SpendWithConditions, SpendableAsset,
    StandardLayer,
};

/// The coins being spent in a transaction, and what each of them will output.
///
/// Coins are [added](Spends::add), then [actions are applied](Spends::apply), and then the spends
/// are [prepared](Spends::prepare) and [spent](Spends::spend). See the
/// [module documentation](crate::action_system) for details.
#[derive(Debug, Clone)]
#[must_use]
pub struct Spends<S = Unfinished> {
    /// The spends of XCH coins.
    pub xch: FungibleSpends<Coin>,
    /// The spends of each CAT, keyed by [`Id::Existing`] asset id or by the [`Id::New`] of the
    /// action that issued it.
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    /// The spends of each DID, keyed by launcher id or by the action that created it.
    pub dids: IndexMap<Id, SingletonSpends<Did>>,
    /// The spends of each NFT, keyed by launcher id or by the action that minted it.
    pub nfts: IndexMap<Id, SingletonSpends<Nft>>,
    /// The spends of each option contract, keyed by launcher id or by the action that minted it.
    pub options: IndexMap<Id, SingletonSpends<OptionContract>>,
    /// The p2 puzzle hash of coins that are created and spent within the transaction, for example
    /// to emit conditions when no other spend can. The caller must be able to spend it.
    pub intermediate_puzzle_hash: Bytes32,
    /// The p2 puzzle hash that change and unsent singletons are sent to.
    pub change_puzzle_hash: Bytes32,
    /// The coins created so far, which are returned when the transaction is finished.
    pub outputs: Outputs,
    /// Conditions that aren't tied to a specific coin, which are attached to a single spend by
    /// [`Spends::prepare`].
    pub conditions: ConditionConfig,
    /// The transaction-wide conditions that [`Spends::prepare`] couldn't attach to any spend, when
    /// [`ConditionConfig::return_unplaced_conditions`] is set. The caller must emit them from a
    /// spend of its own, such as a vault's delegated spend. This is empty until the spends are
    /// prepared.
    pub unplaced_conditions: Conditions,
    /// The CHIP-0057 silent payments that have been applied, which
    /// [`Spends::prepare_with_silent_payment_keys`] derives the outputs of.
    #[cfg(feature = "chip-0057")]
    pub silent_payments: crate::silent_payments::SilentPaymentState,
    _state: S,
}

/// Conditions that aren't tied to a specific coin, such as the assertions of an offer's requested
/// payments.
#[derive(Debug, Default, Clone)]
pub struct ConditionConfig {
    /// Conditions that are only included if there's already a spend that can emit them.
    pub optional: Conditions,
    /// Conditions that must be included, even if an intermediate coin has to be created to emit them.
    pub required: Conditions,
    /// Skips the assertions of payments made by settlement coins. This is only safe if something
    /// else ties the settlement spends to the transaction, since otherwise they could be removed.
    pub disable_settlement_assertions: bool,
    /// If there is no spend that can emit the transaction-wide conditions, returns them in
    /// [`Spends::unplaced_conditions`] rather than creating an intermediate coin to emit them.
    ///
    /// This is for callers that make another spend in the same transaction which isn't part of the
    /// [`Spends`] (such as the delegated spend of a vault), and can emit the conditions from it. For
    /// example, when a vault takes an offer without spending any of its own coins, it can assert the
    /// payments made by the settlement coins itself, rather than spending an intermediate coin. The
    /// returned conditions include the optional conditions, since there is a spend to emit them.
    pub return_unplaced_conditions: bool,
}

/// The coins created by a transaction.
#[derive(Debug, Default, Clone)]
pub struct Outputs {
    /// The XCH coins created by the actions, and the change. Intermediate coins aren't included.
    pub xch: Vec<Coin>,
    /// The CAT coins created by the actions, and the change. Intermediate coins aren't included.
    pub cats: IndexMap<Id, Vec<Cat>>,
    /// The final coin of each DID that wasn't melted.
    pub dids: IndexMap<Id, Did>,
    /// The final coin of each NFT.
    pub nfts: IndexMap<Id, Nft>,
    /// The final coin of each option contract that wasn't exercised.
    pub options: IndexMap<Id, OptionContract>,
    /// The total fee paid by [`Action::fee`](crate::Action::fee).
    pub fee: u64,
    /// The part of the fee that is asserted with a `RESERVE_FEE` condition.
    pub reserved_fee: u64,
    /// The notarized payments of the silent payments the transaction requests from a
    /// counterparty's settlement coins ([`Action::silent_payment_requested`](crate::Action::silent_payment_requested)),
    /// one per asset, for the requested payments of an offer built with
    /// [`Offer::from_input_spend_bundle`](crate::Offer::from_input_spend_bundle). The transaction
    /// already asserts their announcements.
    #[cfg(feature = "chip-0057")]
    pub silent_payment_requests: crate::RequestedPayments,
    /// The asset information of [`Outputs::silent_payment_requests`]: the hidden puzzle hash of
    /// each requested CAT (`None` for a CAT that is not revocable), so that the offer file lists
    /// the settlement coin the transaction's assertion names.
    #[cfg(feature = "chip-0057")]
    pub silent_payment_asset_info: crate::AssetInfo,
}

/// The position of a spend in [`Spends`]: its asset and its index in that asset's items (or
/// lineage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SpendPosition {
    Xch(usize),
    Cat(Id, usize),
    Did(Id, usize),
    Nft(Id, usize),
    OptionContract(Id, usize),
}

/// The state of [`Spends`] while actions are being applied.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unfinished;

/// The state of [`Spends`] after [`Spends::prepare`], when only the p2 spends remain to be provided.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Finished;

impl Spends<Unfinished> {
    /// Creates empty spends that send change (and unsent singletons) to the puzzle hash, and also
    /// use it for intermediate coins.
    pub fn new(change_puzzle_hash: Bytes32) -> Self {
        Self::with_separate_change_puzzle_hash(change_puzzle_hash, change_puzzle_hash)
    }

    /// Creates the spends with a separate p2 puzzle hash for intermediate coins, which are created
    /// and spent within the transaction. The caller must be able to spend both puzzle hashes.
    pub fn with_separate_change_puzzle_hash(
        intermediate_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            xch: FungibleSpends::new(),
            cats: IndexMap::new(),
            dids: IndexMap::new(),
            nfts: IndexMap::new(),
            options: IndexMap::new(),
            intermediate_puzzle_hash,
            change_puzzle_hash,
            outputs: Outputs::default(),
            conditions: ConditionConfig::default(),
            unplaced_conditions: Conditions::new(),
            #[cfg(feature = "chip-0057")]
            silent_payments: crate::silent_payments::SilentPaymentState::default(),
            _state: Unfinished,
        }
    }

    /// Adds a coin to be spent with its p2 puzzle. Coins with the settlement payments puzzle as their
    /// p2 puzzle (for example, when taking an offer) are spent as settlement spends.
    pub fn add(&mut self, asset: impl AddAsset) {
        asset.add(self);
    }

    /// Adds a revocable CAT to be spent with its hidden puzzle (ie, revoked by the issuer), rather
    /// than its p2 puzzle. The spend must be authorized by the hidden puzzle hash, which is
    /// reported as the p2 puzzle hash of [`SpendableAsset::RevokedCat`].
    ///
    /// Everything created by the revocation spend (payments, change, and intermediate coins) is
    /// wrapped in the same revocation layer and hinted with its p2 puzzle hash, so the outputs
    /// remain revocable. Any value that isn't sent elsewhere is returned to the change puzzle hash.
    ///
    /// Returns [`DriverError::NotRevocable`] if the CAT doesn't have a hidden puzzle.
    pub fn add_for_revocation(&mut self, cat: Cat) -> Result<(), DriverError> {
        let spend = FungibleSpend::revocation(cat)?;

        self.cats
            .entry(Id::Existing(cat.info.asset_id))
            .or_default()
            .items
            .push(spend);

        Ok(())
    }

    /// Applies each action in order, and returns the [`Deltas`] of the actions for
    /// [`Spends::prepare`].
    ///
    /// Returns [`DriverError::DuplicateSettlementPayment`] if two settlement coins with the same
    /// puzzle hash make the same notarized payment.
    pub fn apply(
        &mut self,
        ctx: &mut SpendContext,
        actions: &[Action],
    ) -> Result<Deltas, DriverError> {
        let deltas = Deltas::from_actions(actions);
        for (index, action) in actions.iter().enumerate() {
            action.spend(ctx, self, index)?;
        }

        self.check_unique_settlement_payments()?;

        Ok(deltas)
    }

    /// Settlement coins can be spent by anyone, so the payments they make are asserted by a
    /// conditions spend. If two settlement coins with the same puzzle hash made the same notarized
    /// payment, their announcements would be identical and a single assertion would be satisfied by
    /// either of them, so one of the spends could be removed from the transaction.
    fn check_unique_settlement_payments(&self) -> Result<(), DriverError> {
        let mut announcement_ids = HashSet::new();

        for assertion in self.payment_assertions() {
            if !announcement_ids.insert(assertion.announcement_id) {
                return Err(DriverError::DuplicateSettlementPayment);
            }
        }

        Ok(())
    }

    pub(crate) fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
    ) -> Result<(), DriverError> {
        let change = self.xch.create_change(
            ctx,
            deltas.get(&Id::Xch).unwrap_or(&Delta::default()),
            self.change_puzzle_hash,
        )?;
        self.outputs.xch.extend(change);

        for (&id, cat) in &mut self.cats {
            let change = cat.create_change(
                ctx,
                deltas.get(&id).unwrap_or(&Delta::default()),
                self.change_puzzle_hash,
            )?;

            if !change.is_empty() {
                self.outputs.cats.entry(id).or_default().extend(change);
            }
        }

        for (&id, did) in &mut self.dids {
            if let Some(change) =
                did.finalize(ctx, self.intermediate_puzzle_hash, self.change_puzzle_hash)?
            {
                self.outputs.dids.insert(id, change);
            }
        }

        for (&id, nft) in &mut self.nfts {
            if let Some(change) =
                nft.finalize(ctx, self.intermediate_puzzle_hash, self.change_puzzle_hash)?
            {
                self.outputs.nfts.insert(id, change);
            }
        }

        for (&id, option) in &mut self.options {
            if let Some(change) =
                option.finalize(ctx, self.intermediate_puzzle_hash, self.change_puzzle_hash)?
            {
                self.outputs.options.insert(id, change);
            }
        }

        Ok(())
    }

    fn payment_assertions(&self) -> Vec<AssertPuzzleAnnouncement> {
        let mut payment_assertions = self.xch.payment_assertions.clone();

        for cat in self.cats.values() {
            payment_assertions.extend_from_slice(&cat.payment_assertions);
        }

        for did in self.dids.values() {
            for item in &did.lineage {
                payment_assertions.extend_from_slice(&item.payment_assertions);
            }
        }

        for nft in self.nfts.values() {
            for item in &nft.lineage {
                payment_assertions.extend_from_slice(&item.payment_assertions);
            }
        }

        for option in self.options.values() {
            for item in &option.lineage {
                payment_assertions.extend_from_slice(&item.payment_assertions);
            }
        }

        payment_assertions
    }

    /// The spends that can emit conditions and that [`Spends::prepare`] uses: the first of them
    /// emits the transaction-wide conditions, and the [`Relation`] links all of them. They are,
    /// in order, every XCH conditions spend, then the first conditions spend of each CAT asset
    /// (the other spends of an asset are tied to it by the CAT ring), then the last conditions
    /// spend of each DID, NFT and option lineage.
    ///
    /// This is the one place the selection is made. With the `chip-0057` feature, the silent
    /// payment spend group is predicted from it, and checked against it once the transaction is
    /// complete, so the group is always exactly the coins `Relation::AssertConcurrent` binds.
    pub(crate) fn conditions_spend_positions(&self) -> Vec<SpendPosition> {
        let mut positions = Vec::new();

        for (index, item) in self.xch.items.iter().enumerate() {
            if item.kind.is_conditions() {
                positions.push(SpendPosition::Xch(index));
            }
        }

        for (&id, cat) in &self.cats {
            if let Some(index) = cat.items.iter().position(|item| item.kind.is_conditions()) {
                positions.push(SpendPosition::Cat(id, index));
            }
        }

        for (&id, did) in &self.dids {
            if let Some(index) = did
                .lineage
                .iter()
                .rposition(|item| item.kind.is_conditions())
            {
                positions.push(SpendPosition::Did(id, index));
            }
        }

        for (&id, nft) in &self.nfts {
            if let Some(index) = nft
                .lineage
                .iter()
                .rposition(|item| item.kind.is_conditions())
            {
                positions.push(SpendPosition::Nft(id, index));
            }
        }

        for (&id, option) in &self.options {
            if let Some(index) = option
                .lineage
                .iter()
                .rposition(|item| item.kind.is_conditions())
            {
                positions.push(SpendPosition::OptionContract(id, index));
            }
        }

        positions
    }

    /// The spends selected by [`Spends::conditions_spend_positions`], in its order, with the coin
    /// being spent.
    pub(crate) fn iter_conditions_spends(
        &mut self,
    ) -> impl Iterator<Item = (Coin, &mut ConditionsSpend)> {
        let selected: HashSet<SpendPosition> =
            self.conditions_spend_positions().into_iter().collect();

        // Every spend in the order the positions are listed in, so filtering keeps their order.
        let xch = self
            .xch
            .items
            .iter_mut()
            .enumerate()
            .map(|(index, item)| (SpendPosition::Xch(index), item.asset, &mut item.kind));
        let cats = self.cats.iter_mut().flat_map(|(&id, cat)| {
            cat.items.iter_mut().enumerate().map(move |(index, item)| {
                (
                    SpendPosition::Cat(id, index),
                    item.asset.coin,
                    &mut item.kind,
                )
            })
        });
        let dids = self.dids.iter_mut().flat_map(|(&id, did)| {
            did.lineage
                .iter_mut()
                .enumerate()
                .map(move |(index, item)| {
                    (
                        SpendPosition::Did(id, index),
                        item.asset.coin,
                        &mut item.kind,
                    )
                })
        });
        let nfts = self.nfts.iter_mut().flat_map(|(&id, nft)| {
            nft.lineage
                .iter_mut()
                .enumerate()
                .map(move |(index, item)| {
                    (
                        SpendPosition::Nft(id, index),
                        item.asset.coin,
                        &mut item.kind,
                    )
                })
        });
        let options = self.options.iter_mut().flat_map(|(&id, option)| {
            option
                .lineage
                .iter_mut()
                .enumerate()
                .map(move |(index, item)| {
                    (
                        SpendPosition::OptionContract(id, index),
                        item.asset.coin,
                        &mut item.kind,
                    )
                })
        });

        xch.chain(cats)
            .chain(dids)
            .chain(nfts)
            .chain(options)
            .filter_map(move |(position, coin, kind)| {
                if !selected.contains(&position) {
                    return None;
                }
                match kind {
                    SpendKind::Conditions(spend) => Some((coin, spend)),
                    SpendKind::Settlement(_) => None,
                }
            })
    }

    /// Attaches the transaction-wide conditions (required and optional conditions, settlement
    /// payment assertions, and the reserved fee) to the first spend that can emit conditions.
    ///
    /// If there is no such spend (for example, when only settlement coins are being spent) and
    /// there are conditions that must be included, an ephemeral coin is created with the
    /// intermediate puzzle hash so that it can be spent to emit them. If
    /// [`ConditionConfig::return_unplaced_conditions`] is set, they're returned in
    /// [`Spends::unplaced_conditions`] instead.
    fn emit_conditions(&mut self, ctx: &mut SpendContext) -> Result<(), DriverError> {
        self.unplaced_conditions = Conditions::new();

        let mut conditions = self.conditions.required.clone().extend(
            if self.conditions.disable_settlement_assertions {
                vec![]
            } else {
                self.payment_assertions()
            },
        );

        let required = !conditions.is_empty() || self.outputs.reserved_fee > 0;

        conditions = conditions.extend(self.conditions.optional.clone());

        if self.outputs.reserved_fee > 0 {
            conditions = conditions.reserve_fee(self.outputs.reserved_fee);
        }

        if let Some((_, spend)) = self.iter_conditions_spends().next() {
            spend.add_conditions(conditions);
            return Ok(());
        }

        if self.conditions.return_unplaced_conditions {
            self.unplaced_conditions = conditions;
            return Ok(());
        }

        if conditions.is_empty() || !required {
            return Ok(());
        }

        let intermediate_puzzle_hash = self.intermediate_puzzle_hash;

        if let Some(index) = self
            .xch
            .intermediate_conditions_source(ctx, intermediate_puzzle_hash)?
            && try_add_conditions(&mut self.xch.items[index].kind, &mut conditions)
        {
            return Ok(());
        }

        for cat in self.cats.values_mut() {
            if let Some(index) =
                cat.intermediate_conditions_source(ctx, intermediate_puzzle_hash)?
                && try_add_conditions(&mut cat.items[index].kind, &mut conditions)
            {
                return Ok(());
            }
        }

        for did in self.dids.values_mut() {
            if let Some(mut item) =
                did.intermediate_fungible_xch_spend(ctx, intermediate_puzzle_hash)?
            {
                let emitted = try_add_conditions(&mut item.kind, &mut conditions);
                self.xch.items.push(item);
                if emitted {
                    return Ok(());
                }
            }
        }

        for nft in self.nfts.values_mut() {
            if let Some(mut item) =
                nft.intermediate_fungible_xch_spend(ctx, intermediate_puzzle_hash)?
            {
                let emitted = try_add_conditions(&mut item.kind, &mut conditions);
                self.xch.items.push(item);
                if emitted {
                    return Ok(());
                }
            }
        }

        for option in self.options.values_mut() {
            if let Some(mut item) =
                option.intermediate_fungible_xch_spend(ctx, intermediate_puzzle_hash)?
            {
                let emitted = try_add_conditions(&mut item.kind, &mut conditions);
                self.xch.items.push(item);
                if emitted {
                    return Ok(());
                }
            }
        }

        Err(DriverError::CannotEmitConditions)
    }

    fn emit_relation(&mut self, relation: Relation) {
        if relation == Relation::None {
            return;
        }

        let coin_ids: Vec<Bytes32> = self
            .iter_conditions_spends()
            .map(|(coin, _)| coin.coin_id())
            .collect();

        if coin_ids.len() <= 1 {
            return;
        }

        let len = coin_ids.len();

        self.iter_conditions_spends()
            .enumerate()
            .for_each(|(i, (_, spend))| {
                let conditions = match relation {
                    Relation::None => Conditions::new(),
                    Relation::AssertConcurrent => {
                        Conditions::new().assert_concurrent_spend(coin_ids[(i + len - 1) % len])
                    }
                    Relation::CoinAnnouncementRing => Conditions::new()
                        .create_coin_announcement(Bytes::default())
                        .assert_coin_announcement(announcement_id(
                            coin_ids[(i + 1) % len],
                            Bytes::default(),
                        )),
                    Relation::CoinAnnouncementHub => {
                        if i == 0 {
                            Conditions::new().create_coin_announcement(Bytes::default())
                        } else {
                            Conditions::new().assert_coin_announcement(announcement_id(
                                coin_ids[0],
                                Bytes::default(),
                            ))
                        }
                    }
                };

                spend.add_conditions(conditions);
            });
    }

    fn wrap_revocation_outputs(&mut self, ctx: &mut SpendContext) -> Result<(), DriverError> {
        for cat in self.cats.values_mut() {
            for item in &mut cat.items {
                if !item.revoke {
                    continue;
                }

                let (Some(hidden_puzzle_hash), SpendKind::Conditions(spend)) =
                    (item.asset.info.hidden_puzzle_hash, &mut item.kind)
                else {
                    continue;
                };

                spend.wrap_for_revocation(ctx, hidden_puzzle_hash)?;
            }
        }

        Ok(())
    }

    /// The puzzle hashes of every puzzle that the caller needs to be able to spend, including the
    /// intermediate puzzle hash.
    pub fn p2_puzzle_hashes(&self) -> Vec<Bytes32> {
        let mut p2_puzzle_hashes = vec![self.intermediate_puzzle_hash];

        for item in &self.xch.items {
            p2_puzzle_hashes.push(item.p2_puzzle_hash());
        }

        for (_, cat) in &self.cats {
            for item in &cat.items {
                p2_puzzle_hashes.push(item.p2_puzzle_hash());
            }
        }

        for (_, did) in &self.dids {
            for item in &did.lineage {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        for (_, nft) in &self.nfts {
            for item in &nft.lineage {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        for (_, option) in &self.options {
            for item in &option.lineage {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
            }
        }

        p2_puzzle_hashes
    }

    /// The ids of the coins that are spent with conditions, rather than as settlement coins.
    pub fn non_settlement_coin_ids(&self) -> Vec<Bytes32> {
        let mut coin_ids = Vec::new();

        for item in &self.xch.items {
            if item.kind.is_conditions() {
                coin_ids.push(item.asset.coin_id());
            }
        }

        for (_, cat) in &self.cats {
            for item in &cat.items {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        for (_, did) in &self.dids {
            for item in &did.lineage {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        for (_, nft) in &self.nfts {
            for item in &nft.lineage {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        for (_, option) in &self.options {
            for item in &option.lineage {
                if item.kind.is_conditions() {
                    coin_ids.push(item.asset.coin_id());
                }
            }
        }

        coin_ids
    }

    /// Creates change for every asset, attaches the transaction-wide conditions, links the spends
    /// together according to the [`Relation`], and wraps the outputs of revocation spends.
    ///
    /// If [`ConditionConfig::return_unplaced_conditions`] is set and no spend can emit the
    /// transaction-wide conditions, they're left in [`Spends::unplaced_conditions`] for the caller.
    ///
    /// Returns [`DriverError::InsufficientFunds`] if the selected coins don't cover the deltas, or
    /// [`DriverError::DuplicateSettlementPayment`] if two settlement coins with the same puzzle hash
    /// make the same notarized payment.
    pub fn prepare(
        mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
    ) -> Result<Spends<Finished>, DriverError> {
        // Silent payment outputs are derived from the sender's secret keys, which
        // `Spends::prepare_with_silent_payment_keys` takes.
        #[cfg(feature = "chip-0057")]
        if self.silent_payments.has_pending() {
            return Err(DriverError::SilentPaymentKeysMissing);
        }

        self.prepare_unfinished(ctx, deltas, relation)?;
        Ok(self.into_finished())
    }

    /// The steps of [`Spends::prepare`], leaving the spends in the unfinished state.
    pub(crate) fn prepare_unfinished(
        &mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
    ) -> Result<(), DriverError> {
        self.create_change(ctx, deltas)?;
        self.check_unique_settlement_payments()?;
        self.emit_conditions(ctx)?;
        self.emit_relation(relation);
        self.wrap_revocation_outputs(ctx)
    }

    pub(crate) fn into_finished(self) -> Spends<Finished> {
        Spends {
            xch: self.xch,
            cats: self.cats,
            dids: self.dids,
            nfts: self.nfts,
            options: self.options,
            intermediate_puzzle_hash: self.intermediate_puzzle_hash,
            change_puzzle_hash: self.change_puzzle_hash,
            outputs: self.outputs,
            conditions: self.conditions,
            unplaced_conditions: self.unplaced_conditions,
            #[cfg(feature = "chip-0057")]
            silent_payments: self.silent_payments,
            _state: Finished,
        }
    }

    /// Prepares the spends, and spends every coin with the standard puzzle (using the synthetic key
    /// for its p2 puzzle hash), or with the settlement payments puzzle for settlement coins.
    /// Returns [`DriverError::MissingKey`] if a key is missing, or
    /// [`DriverError::CannotEmitConditions`] if there are [unplaced conditions](Spends::unplaced_conditions),
    /// since there's no other spend to emit them from.
    pub fn finish_with_keys(
        self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
    ) -> Result<Outputs, DriverError> {
        self.prepare(ctx, deltas, relation)?
            .spend_with_keys(ctx, synthetic_keys)
    }
}

impl Spends<Finished> {
    /// Spends every coin with the standard puzzle (using the synthetic key for its p2 puzzle
    /// hash), or with the settlement payments puzzle for settlement coins.
    pub(crate) fn spend_with_keys(
        self,
        ctx: &mut SpendContext,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
    ) -> Result<Outputs, DriverError> {
        let spends = self;

        if !spends.unplaced_conditions.is_empty() {
            return Err(DriverError::CannotEmitConditions);
        }

        let mut coin_spends = HashMap::new();

        for (asset, kind) in spends.unspent() {
            match kind {
                SpendKind::Conditions(spend) => {
                    let Some(&synthetic_key) = synthetic_keys.get(&asset.p2_puzzle_hash()) else {
                        return Err(DriverError::MissingKey);
                    };
                    coin_spends.insert(
                        asset.coin().coin_id(),
                        StandardLayer::new(synthetic_key)
                            .spend_with_conditions(ctx, spend.finish())?,
                    );
                }
                SpendKind::Settlement(spend) => {
                    coin_spends.insert(
                        asset.coin().coin_id(),
                        SettlementLayer.construct_spend(
                            ctx,
                            SettlementPaymentsSolution::new(spend.finish()),
                        )?,
                    );
                }
            }
        }

        spends.spend(ctx, coin_spends)
    }
}

impl Spends<Finished> {
    /// Every coin that the caller needs to provide the p2 spend for, along with what the p2 puzzle
    /// must output.
    pub fn unspent(&self) -> Vec<(SpendableAsset, SpendKind)> {
        let mut result = Vec::new();

        for item in &self.xch.items {
            result.push((SpendableAsset::Xch(item.asset), item.kind.clone()));
        }

        for cat in self.cats.values() {
            for item in &cat.items {
                let asset = if item.revoke {
                    SpendableAsset::RevokedCat(item.asset)
                } else {
                    SpendableAsset::Cat(item.asset)
                };
                result.push((asset, item.kind.clone()));
            }
        }

        for did in self.dids.values() {
            for item in &did.lineage {
                result.push((SpendableAsset::Did(item.asset), item.kind.clone()));
            }
        }

        for nft in self.nfts.values() {
            for item in &nft.lineage {
                result.push((SpendableAsset::Nft(item.asset), item.kind.clone()));
            }
        }

        for option in self.options.values() {
            for item in &option.lineage {
                result.push((SpendableAsset::Option(item.asset), item.kind.clone()));
            }
        }

        result
    }

    /// Spends every coin with the p2 spends, keyed by coin id, and returns the [`Outputs`].
    /// Returns [`DriverError::MissingSpend`] if a p2 spend is missing.
    ///
    /// This doesn't emit the [unplaced conditions](Spends::unplaced_conditions), so the caller must
    /// emit them from another spend in the same transaction.
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        mut coin_spends: HashMap<Bytes32, Spend>,
    ) -> Result<Outputs, DriverError> {
        for item in self.xch.items {
            let spend = coin_spends
                .remove(&item.asset.coin_id())
                .ok_or(DriverError::MissingSpend)?;
            ctx.spend(item.asset, spend)?;
        }

        for cat in self.cats.into_values() {
            let mut cat_spends = Vec::new();
            for item in cat.items {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                cat_spends.push(if item.revoke {
                    CatSpend::revoke(item.asset, spend)
                } else {
                    CatSpend::new(item.asset, spend)
                });
            }
            Cat::spend_all(ctx, &cat_spends)?;
        }

        for did in self.dids.into_values() {
            for item in did.lineage {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                item.asset.spend(ctx, spend)?;
            }
        }

        for nft in self.nfts.into_values() {
            for item in nft.lineage {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                let _nft = item.asset.spend(ctx, spend)?;
            }
        }

        for option in self.options.into_values() {
            for item in option.lineage {
                let spend = coin_spends
                    .remove(&item.asset.coin_id())
                    .ok_or(DriverError::MissingSpend)?;
                let _option = item.asset.spend(ctx, spend)?;
            }
        }

        Ok(self.outputs)
    }
}

fn try_add_conditions(kind: &mut SpendKind, conditions: &mut Conditions) -> bool {
    match kind {
        SpendKind::Conditions(spend) => {
            spend.add_conditions(mem::take(conditions));
            true
        }
        SpendKind::Settlement(_) => false,
    }
}

/// An asset that can be added to [`Spends`] with [`Spends::add`].
pub trait AddAsset {
    /// Adds the coin to the spends, to be spent with its p2 puzzle. Adding a singleton that is
    /// already in the spends replaces it.
    fn add(self, spends: &mut Spends);
}

impl AddAsset for Coin {
    fn add(self, spends: &mut Spends) {
        spends.xch.items.push(FungibleSpend::new(self, false));
    }
}

impl AddAsset for Cat {
    fn add(self, spends: &mut Spends) {
        spends
            .cats
            .entry(Id::Existing(self.info.asset_id))
            .or_default()
            .items
            .push(FungibleSpend::new(self, false));
    }
}

impl AddAsset for Did {
    fn add(self, spends: &mut Spends) {
        spends.dids.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}

impl AddAsset for Nft {
    fn add(self, spends: &mut Spends) {
        spends.nfts.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}

impl AddAsset for OptionContract {
    fn add(self, spends: &mut Spends) {
        spends.options.insert(
            Id::Existing(self.info.launcher_id),
            SingletonSpends::new(self, false),
        );
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Condition;

    use crate::{Action, Id, Relation, SpendContext, SpendKind, Spends};

    /// Pinning test for `Relation::AssertConcurrent` — verifies the closed
    /// `ASSERT_CONCURRENT_SPEND` cycle that CHIP-0057 scanners rely on to form
    /// multi-input spend groups ("Inputs for Shared Secret Derivation": each
    /// coin outputs exactly one such condition, naming its predecessor). If
    /// `emit_relation` drifted away from the closed cycle, multi-input silent
    /// payments would stop being detected.
    ///
    /// NOT `#[cfg(feature = "chip-0057")]` gated: `Relation` is general-
    /// purpose; SP is one consumer.
    fn assert_concurrent_cycle_for_n(n: usize) -> Result<()> {
        assert!(n >= 2, "pinning test only meaningful for n >= 2");

        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        // Allocate N independently-funded XCH coins.
        let coins: Vec<_> = (0..n).map(|_| sim.bls(1)).collect();

        // Build Spends with N coins; intermediate puzzle hash defaults to
        // coins[0]'s puzzle hash (the canonical change destination).
        let mut spends = Spends::new(coins[0].puzzle_hash);
        for c in &coins {
            spends.add(c.coin);
        }

        // Apply a conditions-producing action on each xch item so each
        // SpendKind is ConditionsSpend (rather than Settlement). The
        // standard send-XCH action emits a CreateCoin condition on the
        // chosen input — sufficient to keep every item.kind as
        // SpendKind::Conditions before prepare() runs emit_relation.
        //
        // Burn destination: any 32-byte puzzle hash literal works; the test
        // does not submit the bundle anywhere.
        let burn_ph: chia_protocol::Bytes32 = [0x77u8; 32].into();
        let deltas = spends.apply(&mut ctx, &[Action::send(Id::Xch, burn_ph, 1, Memos::None)])?;

        // Drive Spends<Unfinished> -> Spends<Finished>; emit_relation runs
        // inside prepare().
        let finished = spends.prepare(&mut ctx, &deltas, Relation::AssertConcurrent)?;

        // Collect the coin_ids in iteration order.
        let coin_ids: Vec<chia_protocol::Bytes32> = finished
            .xch
            .items
            .iter()
            .map(|i| i.asset.coin_id())
            .collect();
        assert_eq!(coin_ids.len(), n);

        // For each item, assert exactly one AssertConcurrentSpend with the
        // expected predecessor coin_id (coin 0 -> coin N-1; coin i -> coin i-1).
        for (i, item) in finished.xch.items.iter().enumerate() {
            let SpendKind::Conditions(spend) = &item.kind else {
                panic!("xch item {i} not SpendKind::Conditions; cannot inspect");
            };
            let conds = spend.conditions_ref();
            let expected_predecessor = if i == 0 {
                coin_ids[n - 1]
            } else {
                coin_ids[i - 1]
            };
            let mut count = 0;
            let mut last_observed_target: Option<chia_protocol::Bytes32> = None;
            for cond in conds.iter() {
                if let Condition::AssertConcurrentSpend(a) = cond {
                    count += 1;
                    last_observed_target = Some(a.coin_id);
                }
            }
            assert_eq!(
                count, 1,
                "coin {i} of {n}: expected exactly 1 AssertConcurrentSpend, got {count}"
            );
            assert_eq!(
                last_observed_target,
                Some(expected_predecessor),
                "coin {i} of {n}: AssertConcurrentSpend target mismatch (expected predecessor {})",
                hex::encode(expected_predecessor)
            );
        }

        Ok(())
    }

    #[test]
    fn assert_concurrent_relation_emits_cycle_for_n_coins_2() -> Result<()> {
        assert_concurrent_cycle_for_n(2)
    }

    #[test]
    fn assert_concurrent_relation_emits_cycle_for_n_coins_3() -> Result<()> {
        assert_concurrent_cycle_for_n(3)
    }

    #[test]
    fn assert_concurrent_relation_emits_cycle_for_n_coins_4() -> Result<()> {
        assert_concurrent_cycle_for_n(4)
    }
}
