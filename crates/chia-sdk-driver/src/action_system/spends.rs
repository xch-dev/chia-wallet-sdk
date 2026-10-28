use std::{
    collections::{HashMap, HashSet},
    mem,
};

use chia_bls::PublicKey;
#[cfg(feature = "chip-0057")]
use chia_bls::SecretKey;
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
    pub xch: FungibleSpends<Coin>,
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    pub dids: IndexMap<Id, SingletonSpends<Did>>,
    pub nfts: IndexMap<Id, SingletonSpends<Nft>>,
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
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_counters: std::collections::HashMap<[u8; 48], u32>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payments_pending: Vec<crate::silent_payments::SilentPaymentPending>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_settled_others: Vec<crate::silent_payments::SettledOtherPayment>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_synthetic_pks: Option<IndexMap<Bytes32, PublicKey>>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_synthetic_sks:
        Option<crate::silent_payments::SilentPaymentSecretKeys>,
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
enum SpendPosition {
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
    /// Uses the change puzzle hash for intermediate coins as well.
    pub fn new(change_puzzle_hash: Bytes32) -> Self {
        Self::with_separate_change_puzzle_hash(change_puzzle_hash, change_puzzle_hash)
    }

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
            #[cfg(feature = "chip-0057")]
            silent_payment_counters: std::collections::HashMap::new(),
            #[cfg(feature = "chip-0057")]
            silent_payments_pending: Vec::new(),
            #[cfg(feature = "chip-0057")]
            silent_payment_settled_others: Vec::new(),
            #[cfg(feature = "chip-0057")]
            silent_payment_synthetic_pks: None,
            #[cfg(feature = "chip-0057")]
            silent_payment_synthetic_sks: None,
            _state: Unfinished,
        }
    }

    /// Adds a coin to be spent with its p2 puzzle. Coins with the settlement payments puzzle as their
    /// p2 puzzle (for example, when taking an offer) are spent as settlement spends.
    pub fn add(&mut self, asset: impl AddAsset) {
        asset.add(self);
    }

    /// Register the synthetic keys from which [`Spends::prepare`] derives the outputs of the
    /// silent payment sends that have been applied.
    ///
    /// The maps are keyed by p2 puzzle hash and must cover every coin of the spend group: every
    /// XCH coin the transaction spends with the standard puzzle, and the first CAT coin of each
    /// asset, keyed by the puzzle hash of its inner standard puzzle. That includes intermediate
    /// coins, which have the puzzle hash of the coin that creates them, so the keys of the
    /// selected coins are normally enough.
    ///
    /// The values are [`crate::silent_payments::SyntheticPublicKey`] /
    /// [`crate::silent_payments::SyntheticSecretKey`], newtypes that make passing a raw wallet
    /// key a compile error. Construct them with `from_raw` (which derives the synthetic key) or
    /// with `from_synthetic_unchecked` when the key is already synthetic. Either way,
    /// [`Spends::prepare`] checks every key against its coin
    /// (`curry_tree_hash(pk) == p2_puzzle_hash` and `sk.public_key() == pk`) and returns
    /// [`DriverError::SilentPaymentKeyNotSynthetic`] on a mismatch.
    ///
    /// `secret_keys` carries secret key material, which is held by `Spends`
    /// (and by every clone of it) until the outputs have been derived in
    /// [`Spends::prepare`], and is dropped then. `Debug` on `Spends` does not
    /// print the keys. They are not zeroized on drop, because
    /// `chia_bls::SecretKey` cannot be.
    #[cfg(feature = "chip-0057")]
    pub fn with_silent_payment_keys(
        &mut self,
        synthetic_pks: IndexMap<Bytes32, crate::silent_payments::SyntheticPublicKey>,
        secret_keys: IndexMap<Bytes32, crate::silent_payments::SyntheticSecretKey>,
    ) -> &mut Self {
        self.silent_payment_synthetic_pks = Some(
            synthetic_pks
                .into_iter()
                .map(|(ph, k)| (ph, k.into_inner()))
                .collect(),
        );
        self.silent_payment_synthetic_sks = Some(crate::silent_payments::SilentPaymentSecretKeys(
            secret_keys
                .into_iter()
                .map(|(ph, k)| (ph, k.into_inner()))
                .collect(),
        ));
        self
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

    fn create_change(
        &mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
    ) -> Result<(), DriverError> {
        if let Some(change) = self.xch.create_change(
            ctx,
            deltas.get(&Id::Xch).unwrap_or(&Delta::default()),
            self.change_puzzle_hash,
        )? {
            self.outputs.xch.push(change);
        }

        for (&id, cat) in &mut self.cats {
            if let Some(change) = cat.create_change(
                ctx,
                deltas.get(&id).unwrap_or(&Delta::default()),
                self.change_puzzle_hash,
            )? {
                self.outputs.cats.entry(id).or_default().push(change);
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
    fn conditions_spend_positions(&self) -> Vec<SpendPosition> {
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
    fn iter_conditions_spends(&mut self) -> impl Iterator<Item = (Coin, &mut ConditionsSpend)> {
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
    /// intermediate puzzle hash so that it can be spent to emit them.
    fn emit_conditions(&mut self, ctx: &mut SpendContext) -> Result<(), DriverError> {
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
    /// Returns [`DriverError::InsufficientFunds`] if the selected coins don't cover the deltas, or
    /// [`DriverError::DuplicateSettlementPayment`] if two settlement coins with the same puzzle hash
    /// make the same notarized payment.
    pub fn prepare(
        mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
    ) -> Result<Spends<Finished>, DriverError> {
        // CHIP-0057: derive and emit the silent payment outputs before the change is created, so
        // that the outputs keep the position a normal payment would have. This is a no-op unless
        // an `Action::silent_payment_send` or a settled silent payment action has been applied.
        #[cfg(feature = "chip-0057")]
        let silent_payment_group = if self.silent_payments_pending.is_empty()
            && self.silent_payment_settled_others.is_empty()
        {
            None
        } else {
            Some(sp_finish_branch(ctx, &mut self, deltas, relation)?)
        };

        self.create_change(ctx, deltas)?;
        self.check_unique_settlement_payments()?;
        self.emit_conditions(ctx)?;
        self.emit_relation(relation);

        // CHIP-0057: the outputs above were derived from a predicted spend group. If the coins
        // that `emit_relation` actually bound differ from it, a scanner would not find the
        // payment, so fail instead of returning an undetectable one.
        #[cfg(feature = "chip-0057")]
        if let Some(mut expected) = silent_payment_group {
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

        self.wrap_revocation_outputs(ctx)?;

        Ok(Spends {
            xch: self.xch,
            cats: self.cats,
            dids: self.dids,
            nfts: self.nfts,
            options: self.options,
            intermediate_puzzle_hash: self.intermediate_puzzle_hash,
            change_puzzle_hash: self.change_puzzle_hash,
            outputs: self.outputs,
            conditions: self.conditions,
            #[cfg(feature = "chip-0057")]
            silent_payment_counters: self.silent_payment_counters,
            #[cfg(feature = "chip-0057")]
            silent_payments_pending: self.silent_payments_pending,
            #[cfg(feature = "chip-0057")]
            silent_payment_settled_others: self.silent_payment_settled_others,
            #[cfg(feature = "chip-0057")]
            silent_payment_synthetic_pks: self.silent_payment_synthetic_pks,
            #[cfg(feature = "chip-0057")]
            silent_payment_synthetic_sks: self.silent_payment_synthetic_sks,
            _state: Finished,
        })
    }

    /// Prepares the spends, and spends every coin with the standard puzzle (using the synthetic key
    /// for its p2 puzzle hash), or with the settlement payments puzzle for settlement coins.
    /// Returns [`DriverError::MissingKey`] if a key is missing.
    ///
    /// With the `chip-0057` feature, if a silent payment send has been applied, its outputs are
    /// derived and emitted by [`Spends::prepare`] from the keys registered with
    /// `Spends::with_silent_payment_keys`.
    pub fn finish_with_keys(
        self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
        synthetic_keys: &IndexMap<Bytes32, PublicKey>,
    ) -> Result<Outputs, DriverError> {
        let spends = self.prepare(ctx, deltas, relation)?;
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

/// CHIP-0057 silent payment derivation, run by [`Spends::prepare`] when at least one
/// `Action::silent_payment_send` or `Action::silent_payment_send_asset` has been applied.
///
/// The one-time puzzle hashes are derived from the *spend group* a scanner will reconstruct
/// (CHIP-0057, "Inputs for Shared Secret Derivation" and "Spend Groups across Assets"): the coins
/// that [`Spends::conditions_spend_positions`] selects once the transaction is complete, which
/// are every XCH coin spent with the standard puzzle (intermediate coins included) and the first
/// coin of each CAT asset (a CAT, or a revocable CAT on the holder's path, held in the standard
/// puzzle). With two or more such coins, [`Relation::AssertConcurrent`] binds exactly them into
/// one `ASSERT_CONCURRENT_SPEND` cycle, and the key sum and coin id set used here cover that
/// cycle, with one term per coin, whatever its asset. One `k` counter per scan key runs across
/// the assets.
///
/// Checks, in order, all before any key is used:
/// 1. [`DriverError::SilentPaymentIneligibleAssetBundle`] if a DID, NFT or option is spent.
/// 2. [`DriverError::SilentPaymentIssuerPathSpend`] if a revocable CAT is spent through the
///    issuer's hidden puzzle.
/// 3. [`DriverError::SilentPaymentNewCatUnsupported`] if a CAT is issued in the transaction.
/// 4. [`DriverError::SilentPaymentRequiresInputBinding`] if the group has two or more coins and
///    the relation is not [`Relation::AssertConcurrent`].
/// 5. [`DriverError::SilentPaymentKeysNotRegistered`] if no keys were registered.
/// 6. [`DriverError::SilentPaymentNoXchInputs`] if the group is empty (no eligible spend).
/// 7. [`DriverError::SilentPaymentMultiPartyUnsupported`] if a selected coin has no secret key,
///    or [`DriverError::SilentPaymentIntermediateKeyMissing`] if an intermediate coin has none.
/// 8. [`DriverError::SilentPaymentKeyNotSynthetic`] if a registered key is not the synthetic key
///    of its coin's (inner) standard puzzle.
/// 9. [`DriverError::SilentPaymentParentNotEligible`] if an output would be created by a coin
///    outside the group, or by a coin of another asset. A settled payment is created by a
///    settlement coin, which is never in the group, so it is not checked here.
/// 10. [`DriverError::SilentPaymentZeroKeySum`] if the secret keys of the group sum to zero, and
///     [`DriverError::SilentPaymentZeroInputHash`] / [`DriverError::SilentPaymentZeroTweak`] if
///     one of the derived scalars is zero.
///
/// Settled payments ([`crate::Action::silent_payment_requested`] and
/// [`crate::Action::silent_payment_settle`], CHIP-0057 "Payments through Settlement Coins") are
/// derived in the same pass, with the `k` each took when its action was applied. Then, per asset
/// in order of first appearance, the asset's silent payments in the order of the actions and its
/// other payments make one notarized payment under the party's nonce, and its announcement is
/// asserted as a required condition, which [`Spends::emit_conditions`] attaches to the first
/// coin of the group. A requested payment is published in [`Outputs::silent_payment_requests`],
/// and a payment taken from a settlement coin the transaction spends is added to its solution.
/// An asset with other payments only gets its own notarized payment and assertion, after the
/// assets of the silent payments, as [`crate::derive_settlement_outputs`] gives it (CHIP-0057
/// `DeriveSettlementOutputs`). The settled check: [`DriverError::SilentPaymentSettlementMismatch`]
/// if two settled entries carry different nonces or the entries of one asset have different
/// payers.
///
/// Returns the coin ids of the spend group, which [`Spends::prepare`] compares with the coins
/// that were actually bound together once the transaction is complete.
#[cfg(feature = "chip-0057")]
fn sp_finish_branch(
    ctx: &mut SpendContext,
    spends: &mut Spends,
    deltas: &Deltas,
    relation: Relation,
) -> Result<Vec<Bytes32>, DriverError> {
    use chia_puzzle_types::standard::StandardArgs;
    use chia_sdk_types::conditions::CreateCoin;

    use crate::silent_payments::{
        SilentPaymentSource, aggregate_sender_sks, compute_input_hash, derive_one_time_puzzle_hash,
        encoded_payment,
    };

    // The closed list of eligible spends of version 0 (CHIP-0057 "Eligible Spends") is XCH, CAT
    // and holder-path revocable CAT coins held in the standard puzzle. A cycle that passes
    // through any other spend would not bind the coins on either side of it, so these are
    // rejected outright, before any key is touched.
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

    // The secret keys are taken out of `spends` before anything is copied, so that the trial
    // copy below holds none, they are dropped when this function returns, and the prepared
    // `Spends` no longer holds them.
    let secret_keys = spends.silent_payment_synthetic_sks.take();

    // The spend group is every coin `conditions_spend_positions` selects once the transaction is
    // complete. Creating the change can add one more intermediate coin (when every existing
    // spend of an asset already creates a coin identical to the change), so the change is
    // created on a copy first to learn the final set. Silent payment outputs have unique puzzle
    // hashes, so emitting them below does not alter what `create_change` does afterwards.
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

    // `emit_relation` binds every coin of the group (ephemeral ones included) into one cycle, but
    // only for `Relation::AssertConcurrent`. Without it, two or more coins would be separate
    // single-input groups, and outputs derived from their combined keys would be undetectable.
    if group.len() >= 2 && !matches!(relation, Relation::AssertConcurrent) {
        return Err(DriverError::SilentPaymentRequiresInputBinding);
    }

    let Some(secret_keys) = secret_keys else {
        return Err(DriverError::SilentPaymentKeysNotRegistered);
    };
    let secret_keys = secret_keys.0;
    let synthetic_pks = spends.silent_payment_synthetic_pks.as_ref();

    if group.is_empty() {
        return Err(DriverError::SilentPaymentNoXchInputs);
    }

    // One secret key and one coin id per coin in the group, even when coins share a key (an
    // intermediate coin has the puzzle hash, and therefore the key, of the coin that created it;
    // a CAT coin's key is the key of its inner standard puzzle).
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
        // The registered public key must curry to the coin's (inner) puzzle hash, which also
        // proves the coin is locked to the standard puzzle, and must belong to the registered
        // secret key.
        let Some(pk) = synthetic_pks.and_then(|m| m.get(&coin.p2_puzzle_hash)) else {
            return Err(DriverError::SilentPaymentKeyNotSynthetic);
        };
        if Bytes32::from(StandardArgs::curry_tree_hash(*pk)) != coin.p2_puzzle_hash
            || sk.public_key() != *pk
        {
            return Err(DriverError::SilentPaymentKeyNotSynthetic);
        }
        sender_sks.push(sk.clone());
        group_coin_ids.push(coin.coin_id);
    }

    // Every silent payment output must be created by a coin of the group of the output's own
    // asset (CHIP-0057 sender requirement 4, per asset).
    if spends.silent_payments_pending.iter().any(|pending| {
        !group.iter().any(|coin| {
            pending
                .parent_coin
                .is_some_and(|parent| parent.coin_id() == coin.coin_id)
                && match pending.source {
                    SilentPaymentSource::Xch(_) => coin.wrapper.is_none(),
                    SilentPaymentSource::Cat(id, _) => coin
                        .wrapper
                        .is_some_and(|wrapper| Id::Existing(wrapper.asset_id) == id),
                    // Created by a settlement coin, never by a coin of the group.
                    SilentPaymentSource::Settled { .. } => true,
                }
        }) && !matches!(pending.source, SilentPaymentSource::Settled { .. })
    }) {
        return Err(DriverError::SilentPaymentParentNotEligible);
    }

    // `aggregate_sender_sks` fails if the keys sum to zero mod r. The aggregated public key is
    // derived from the secret key sum rather than by adding the public keys; the two are equal.
    let aggregated_sender_sk = aggregate_sender_sks(&sender_sks)?;
    let agg_pk = aggregated_sender_sk.public_key();

    let input_hash = compute_input_hash(&group_coin_ids, &agg_pk);
    if input_hash.is_zero() {
        return Err(DriverError::SilentPaymentZeroInputHash);
    }

    // Take the pending outputs so that they can be iterated while `spends` is mutated.
    let pending = std::mem::take(&mut spends.silent_payments_pending);
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
                // The standard puzzle names the bare one-time puzzle hash, hinted with it
                // (CHIP-0057 "Outputs in CATs"); the CAT layer, and on the holder's path the
                // revocation layer, wrap the coin it creates to wrap(W, p).
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
                // (p amount ()) for XCH, (p amount (p)) for a CAT (CHIP-0057
                // `DeriveSettlementOutputs`).
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

    // The settled payments: one notarized payment and one assertion per asset.
    let others = std::mem::take(&mut spends.silent_payment_settled_others);
    if !settled.is_empty() || !others.is_empty() {
        settle_silent_payments(ctx, spends, settled, others)?;
    }

    // No silent-payment-specific binding is emitted. The `ASSERT_CONCURRENT_SPEND` cycle that
    // `emit_relation` adds for `Relation::AssertConcurrent` covers the coins
    // `conditions_spend_positions` selects, which is the group the values above were computed
    // over.
    Ok(group_coin_ids)
}

/// A settled silent payment of the party, encoded, in the order of the actions.
#[cfg(feature = "chip-0057")]
struct SettledPayment {
    asset: Option<crate::silent_payments::Wrapper>,
    nonce: Bytes32,
    payer: crate::SettlementPayer,
    settlement_index: Option<usize>,
    payment: chia_puzzle_types::offer::Payment,
}

/// The party's requested payments in one asset: one payer, and the payments in the order they
/// are listed (silent payments first, then other payments).
#[cfg(feature = "chip-0057")]
struct AssetSettlement {
    asset: Option<crate::silent_payments::Wrapper>,
    payer: crate::SettlementPayer,
    settlement_index: Option<usize>,
    payments: Vec<chia_puzzle_types::offer::Payment>,
}

/// Groups the party's settled payments per asset, in order of first appearance, into one
/// notarized payment each under the party's one nonce, asserts each announcement as a required
/// condition (emitted by a coin of the spend group, independent of
/// [`ConditionConfig::disable_settlement_assertions`]). The payments the counterparty's
/// settlement coins make are published in [`Outputs::silent_payment_requests`]; those a
/// settlement coin the transaction spends makes are added to that coin's solution as one
/// notarized payment, and the coins they create to [`Outputs`].
///
/// A settled payment never goes through [`SpendKind::create_coin_with_assertion`], which would
/// notarize each coin separately with its parent's coin ID as the nonce.
#[cfg(feature = "chip-0057")]
fn settle_silent_payments(
    ctx: &mut SpendContext,
    spends: &mut Spends,
    settled: Vec<SettledPayment>,
    others: Vec<crate::silent_payments::SettledOtherPayment>,
) -> Result<(), DriverError> {
    use chia_puzzle_types::offer::NotarizedPayment;

    use crate::{CatAssetInfo, SettlementPayer, silent_payments::settlement_assertion};

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

    // The assets in order of first appearance, the silent payments' first (CHIP-0057
    // `DeriveSettlementOutputs`, as `derive_settlement_outputs` orders them): an other payment
    // joins its asset after the asset's silent payments, and an asset with other payments only
    // is added after them, with its own notarized payment and assertion.
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
            // The asset's settlement coin that the transaction spends makes the payments, under
            // the party's nonce, in one notarized payment (not one per coin, as
            // `create_coin_with_assertion` would).
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
#[cfg(feature = "chip-0057")]
struct SilentPaymentGroupCoin {
    coin_id: Bytes32,
    /// The puzzle hash of the coin's standard puzzle (the inner puzzle of a CAT).
    p2_puzzle_hash: Bytes32,
    ephemeral: bool,
    /// `None` for an XCH coin, the coin's wrapper for a CAT coin.
    wrapper: Option<crate::silent_payments::Wrapper>,
}

#[cfg(feature = "chip-0057")]
impl Spends<Unfinished> {
    /// The spend at `position` as a coin of the silent payment spend group, or `None` for a
    /// position that is not an XCH or CAT spend (which [`sp_finish_branch`] has rejected).
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
                    wrapper: Some(crate::silent_payments::Wrapper {
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
