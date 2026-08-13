use std::{collections::HashMap, mem};

use chia_bls::PublicKey;
#[cfg(feature = "chip-0057")]
use chia_bls::SecretKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::offer::SettlementPaymentsSolution;
use chia_sdk_types::{Conditions, conditions::AssertPuzzleAnnouncement};
use indexmap::IndexMap;

use crate::{
    Action, Asset, Cat, CatSpend, ConditionsSpend, Delta, Deltas, Did, DriverError, FungibleSpend,
    FungibleSpends, Id, Layer, Nft, OptionContract, Relation, SettlementLayer, SingletonSpends,
    Spend, SpendAction, SpendContext, SpendKind, SpendWithConditions, SpendableAsset,
    StandardLayer,
};

#[derive(Debug, Clone)]
#[must_use]
pub struct Spends<S = Unfinished> {
    pub xch: FungibleSpends<Coin>,
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    pub dids: IndexMap<Id, SingletonSpends<Did>>,
    pub nfts: IndexMap<Id, SingletonSpends<Nft>>,
    pub options: IndexMap<Id, SingletonSpends<OptionContract>>,
    pub intermediate_puzzle_hash: Bytes32,
    pub change_puzzle_hash: Bytes32,
    pub outputs: Outputs,
    pub conditions: ConditionConfig,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_counters: std::collections::HashMap<[u8; 48], u32>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payments_pending: Vec<crate::silent_payments::SilentPaymentPending>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_synthetic_pks: Option<IndexMap<Bytes32, PublicKey>>,
    #[cfg(feature = "chip-0057")]
    pub(crate) silent_payment_synthetic_sks: Option<IndexMap<Bytes32, SecretKey>>,
    _state: S,
}

#[derive(Debug, Default, Clone)]
pub struct ConditionConfig {
    pub optional: Conditions,
    pub required: Conditions,
    pub disable_settlement_assertions: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Outputs {
    pub xch: Vec<Coin>,
    pub cats: IndexMap<Id, Vec<Cat>>,
    pub dids: IndexMap<Id, Did>,
    pub nfts: IndexMap<Id, Nft>,
    pub options: IndexMap<Id, OptionContract>,
    pub fee: u64,
    pub reserved_fee: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unfinished;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Finished;

impl Spends<Unfinished> {
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
            silent_payment_synthetic_pks: None,
            #[cfg(feature = "chip-0057")]
            silent_payment_synthetic_sks: None,
            _state: Unfinished,
        }
    }

    pub fn add(&mut self, asset: impl AddAsset) {
        asset.add(self);
    }

    /// Register the silent-payment synthetic key maps that
    /// [`Spends::finish_with_keys`]'s chip-0057 branch consumes to derive each
    /// pending one-time puzzle hash.
    ///
    /// The maps are keyed by each spent XCH coin's `p2_puzzle_hash` and the
    /// values are [`crate::silent_payments::SyntheticPublicKey`] /
    /// [`crate::silent_payments::SyntheticSecretKey`] — the newtype wrappers
    /// that make passing a raw wallet key a compile error. Construct them via
    /// `SyntheticSecretKey::from_raw` (synthesizes for you) or
    /// `from_synthetic_unchecked` when the key is already synthetic. The
    /// `from_synthetic_unchecked` escape hatch is covered at finish time by the
    /// runtime check in `sp_finish_branch`
    /// (`curry_tree_hash(pk) == coin p2_puzzle_hash` + `sk.public_key() == pk`),
    /// which rejects a mis-wrapped key before any signing.
    ///
    /// Chainable; matches the `add_*` builder precedent on `Spends`. The PK and
    /// SK maps are co-dependent (the SK map must cover every key in the PK map
    /// for the SP flow), so they are accepted together — splitting would invite
    /// mismatch.
    ///
    /// Privacy warning: `secret_keys` carries sensitive synthetic-secret-key
    /// material. Wallets must treat the map like the SKs themselves (zeroize on
    /// drop, do not log).
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
        self.silent_payment_synthetic_sks = Some(
            secret_keys
                .into_iter()
                .map(|(ph, k)| (ph, k.into_inner()))
                .collect(),
        );
        self
    }

    pub fn apply(
        &mut self,
        ctx: &mut SpendContext,
        actions: &[Action],
    ) -> Result<Deltas, DriverError> {
        let deltas = Deltas::from_actions(actions);
        for (index, action) in actions.iter().enumerate() {
            action.spend(ctx, self, index)?;
        }
        Ok(deltas)
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

    fn iter_conditions_spends(&mut self) -> impl Iterator<Item = (Coin, &mut ConditionsSpend)> {
        self.xch
            .items
            .iter_mut()
            .filter_map(|item| {
                if let SpendKind::Conditions(spend) = &mut item.kind {
                    Some((item.asset, spend))
                } else {
                    None
                }
            })
            .chain(self.cats.values_mut().filter_map(|cat| {
                cat.items.iter_mut().find_map(|item| {
                    if let SpendKind::Conditions(spend) = &mut item.kind {
                        Some((item.asset.coin, spend))
                    } else {
                        None
                    }
                })
            }))
            .chain(self.dids.values_mut().filter_map(|did| {
                did.lineage
                    .iter_mut()
                    .filter_map(|item| {
                        if let SpendKind::Conditions(spend) = &mut item.kind {
                            Some((item.asset.coin, spend))
                        } else {
                            None
                        }
                    })
                    .last()
            }))
            .chain(self.nfts.values_mut().filter_map(|nft| {
                nft.lineage
                    .iter_mut()
                    .filter_map(|item| {
                        if let SpendKind::Conditions(spend) = &mut item.kind {
                            Some((item.asset.coin, spend))
                        } else {
                            None
                        }
                    })
                    .last()
            }))
            .chain(self.options.values_mut().filter_map(|option| {
                option
                    .lineage
                    .iter_mut()
                    .filter_map(|item| {
                        if let SpendKind::Conditions(spend) = &mut item.kind {
                            Some((item.asset.coin, spend))
                        } else {
                            None
                        }
                    })
                    .last()
            }))
    }

    fn emit_conditions(&mut self, ctx: &mut SpendContext) -> Result<(), DriverError> {
        let mut conditions = self.conditions.required.clone().extend(
            if self.conditions.disable_settlement_assertions {
                vec![]
            } else {
                self.payment_assertions()
            },
        );

        let required = !conditions.is_empty();

        conditions = conditions.extend(self.conditions.optional.clone());

        if self.outputs.reserved_fee > 0 {
            conditions = conditions.reserve_fee(self.outputs.reserved_fee);
        }

        for (_, spend) in self.iter_conditions_spends() {
            spend.add_conditions(mem::take(&mut conditions));
        }

        if conditions.is_empty() || !required {
            return Ok(());
        }

        if let Some(index) = self
            .xch
            .intermediate_conditions_source(ctx, self.intermediate_puzzle_hash)?
        {
            match &mut self.xch.items[index].kind {
                SpendKind::Conditions(spend) => {
                    spend.add_conditions(mem::take(&mut conditions));
                }
                SpendKind::Settlement(_) => {}
            }
        }

        for cat in self.cats.values_mut() {
            if let Some(index) =
                cat.intermediate_conditions_source(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut cat.items[index].kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
            }
        }

        for did in self.dids.values_mut() {
            if let Some(mut item) =
                did.intermediate_fungible_xch_spend(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut item.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
                self.xch.items.push(item);
            }
        }

        for nft in self.nfts.values_mut() {
            if let Some(mut item) =
                nft.intermediate_fungible_xch_spend(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut item.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
                self.xch.items.push(item);
            }
        }

        for option in self.options.values_mut() {
            if let Some(mut item) =
                option.intermediate_fungible_xch_spend(ctx, self.intermediate_puzzle_hash)?
            {
                match &mut item.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(mem::take(&mut conditions));
                    }
                    SpendKind::Settlement(_) => {}
                }
                self.xch.items.push(item);
            }
        }

        if conditions.is_empty() {
            Ok(())
        } else {
            Err(DriverError::CannotEmitConditions)
        }
    }

    fn emit_relation(&mut self, relation: Relation) {
        match relation {
            Relation::None => {}
            Relation::AssertConcurrent => {
                let coin_ids: Vec<Bytes32> = self
                    .iter_conditions_spends()
                    .map(|(coin, _)| coin.coin_id())
                    .collect();

                if coin_ids.len() <= 1 {
                    return;
                }

                self.iter_conditions_spends()
                    .enumerate()
                    .for_each(|(i, (_, spend))| {
                        spend.add_conditions(Conditions::new().assert_concurrent_spend(
                            if i == 0 {
                                coin_ids[coin_ids.len() - 1]
                            } else {
                                coin_ids[i - 1]
                            },
                        ));
                    });
            }
        }
    }

    pub fn p2_puzzle_hashes(&self) -> Vec<Bytes32> {
        let mut p2_puzzle_hashes = vec![self.intermediate_puzzle_hash];

        for item in &self.xch.items {
            p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
        }

        for (_, cat) in &self.cats {
            for item in &cat.items {
                p2_puzzle_hashes.push(item.asset.p2_puzzle_hash());
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

    pub fn prepare(
        mut self,
        ctx: &mut SpendContext,
        deltas: &Deltas,
        relation: Relation,
    ) -> Result<Spends<Finished>, DriverError> {
        // chip-0057 silent-payment derivation branch — runs FIRST so the
        // emitted `CreateCoin` conditions land on the parents'
        // `payment_assertions` before `emit_conditions` fires below. No-op
        // when no `Action::silent_payment_send` has been applied.
        #[cfg(feature = "chip-0057")]
        if !self.silent_payments_pending.is_empty() {
            sp_finish_branch(ctx, &mut self, relation)?;
        }

        self.create_change(ctx, deltas)?;
        self.emit_conditions(ctx)?;
        self.emit_relation(relation);

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
            silent_payment_synthetic_pks: self.silent_payment_synthetic_pks,
            #[cfg(feature = "chip-0057")]
            silent_payment_synthetic_sks: self.silent_payment_synthetic_sks,
            _state: Finished,
        })
    }

    /// Finish the spend with synthetic public keys, producing the final
    /// [`Outputs`].
    ///
    /// Privacy warning: under chip-0057, when `silent_payments_pending` is non-empty
    /// (i.e. at least one `Action::silent_payment_send` has been applied), the
    /// chip-0057 SP branch runs inside
    /// [`Spends::prepare`] (called below) so the derived `CreateCoin`
    /// conditions feed into the parents' `payment_assertions` before
    /// `emit_conditions`. The branch consumes
    /// `Spends::silent_payment_synthetic_sks` (registered via
    /// [`Spends::with_silent_payment_keys`]) and emits the recipient's one-time
    /// puzzle hash on the recorded parent. Memos travel in `CreateCoin.memos`
    /// in plaintext, visible to anyone holding the recipient's scan key. The
    /// 32-byte first-memo hint guard fired at apply time
    /// (`DriverError::SilentPaymentMemoHintForbidden`) — no further memo guard
    /// fires here.
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

/// Chip-0057 finish-time SP branch: runs the one-time-puzzle-hash derivation
/// pipeline for the silent-payment outputs recorded at apply time. Invoked from
/// [`Spends::finish_with_keys`].
///
/// Gate ordering (cheapest / most fundamental first):
/// 1. [`DriverError::SilentPaymentRequiresInputBinding`] — fires first on `≥2`
///    non-ephemeral XCH inputs with `Relation != AssertConcurrent`.
/// 2. [`DriverError::SilentPaymentKeysNotRegistered`] — fires if
///    `with_silent_payment_keys` was not called.
/// 3. [`DriverError::SilentPaymentMultiPartyUnsupported`] — SK-coverage check.
/// 4. Per-input synthetic-key check — [`DriverError::SilentPaymentKeyNotSynthetic`]
///    if `StandardArgs::curry_tree_hash(registered_pk) != ph` or
///    `sk.public_key() != registered_pk` (runs for single-input too).
/// 5. [`DriverError::SilentPaymentNoXchInputs`] — collected SK set empty.
///
/// After gates pass: aggregate sender SKs, recover aggregated PK, compute
/// `input_hash`, per-pending derive one-time puzzle hash + push `CreateCoin`
/// via `create_coin_with_assertion` onto the recorded parent's
/// `payment_assertions`, push the resulting Coin to `outputs.xch`.
#[cfg(feature = "chip-0057")]
fn sp_finish_branch(
    ctx: &mut SpendContext,
    spends: &mut Spends,
    relation: Relation,
) -> Result<(), DriverError> {
    use chia_puzzle_types::standard::StandardArgs;
    use chia_sdk_types::conditions::CreateCoin;

    use crate::silent_payments::{
        aggregate_sender_sks, compute_input_hash, derive_one_time_puzzle_hash,
    };

    // GATE 0 (XCH-only invariant): silent-payment send bundles must not co-spend
    // any non-XCH asset. Fires UNIFORMLY regardless of XCH input count (a
    // single-input SP + CAT is technically detectable via the receiver's Pass-1
    // singleton, but the invariant is uniform — mixed-asset SP bundles are
    // unsupported and fail loudly). This is the FIRST check, before any
    // derivation or key work. The four non-XCH spend maps must all be empty.
    if !spends.cats.is_empty()
        || !spends.dids.is_empty()
        || !spends.nfts.is_empty()
        || !spends.options.is_empty()
    {
        return Err(DriverError::SilentPaymentMixedAssetBundle);
    }

    // GATE 1: SilentPaymentRequiresInputBinding fires first among the input
    // gates; multi-input atomic-binding is more fundamental than
    // key-registration. With GATE 0 above, cats/dids/nfts/options are guaranteed
    // empty for any SP finish that reaches here, so the AssertConcurrent cycle
    // (built later in `prepare` over `iter_conditions_spends`) threads only XCH
    // conditions-spends — binding exactly the non-ephemeral XCH input set the
    // receiver reconstructs.
    let non_ephemeral_xch_count = spends.xch.items.iter().filter(|i| !i.ephemeral).count();
    if non_ephemeral_xch_count >= 2 && !matches!(relation, Relation::AssertConcurrent) {
        return Err(DriverError::SilentPaymentRequiresInputBinding);
    }

    // GATE 2: keys must be registered.
    let Some(secret_keys) = spends.silent_payment_synthetic_sks.as_ref() else {
        return Err(DriverError::SilentPaymentKeysNotRegistered);
    };
    // The synthetic-key check needs the registered PK map alongside the SK map;
    // bind it once here (a second immutable borrow of a distinct field) so the
    // per-input synthetic-ness check below does not re-borrow `spends` while
    // `secret_keys` is live.
    let synthetic_pks = spends.silent_payment_synthetic_pks.as_ref();

    // Step 2 + 3: collect XCH input coin ids + verify SK coverage.
    // Iterating non-ephemeral xch.items only: ephemeral items are intermediate
    // coins created within this spend group and are not wallet-controlled inputs
    // whose SKs the sender holds.
    let mut xch_input_ids: Vec<Bytes32> = Vec::with_capacity(spends.xch.items.len());
    let mut sender_sks: Vec<SecretKey> = Vec::with_capacity(spends.xch.items.len());
    for item in spends.xch.items.iter().filter(|i| !i.ephemeral) {
        let ph = item.asset.p2_puzzle_hash();
        let Some(sk) = secret_keys.get(&ph) else {
            return Err(DriverError::SilentPaymentMultiPartyUnsupported);
        };
        // Synthetic-key check: reject raw (un-synthesized) or inconsistent keys BEFORE signing.
        // The IndexMap key `ph` is the coin's p2_puzzle_hash; for a correctly-synthetic
        // registered pk, curry_tree_hash(pk) == ph by construction (validates against the
        // ACTUAL coin, so default AND custom-hidden synthetic keys pass, raw keys fail).
        // sk.public_key() == pk pins sk/pk map consistency. Runs for every non-ephemeral
        // XCH input, single-input included.
        let Some(pk) = synthetic_pks.and_then(|m| m.get(&ph)) else {
            return Err(DriverError::SilentPaymentKeyNotSynthetic);
        };
        if Bytes32::from(StandardArgs::curry_tree_hash(*pk)) != ph || sk.public_key() != *pk {
            return Err(DriverError::SilentPaymentKeyNotSynthetic);
        }
        sender_sks.push(sk.clone());
        xch_input_ids.push(item.asset.coin_id());
    }

    // Step 4: no-inputs guard. Only fires if every XCH item is ephemeral.
    if sender_sks.is_empty() {
        return Err(DriverError::SilentPaymentNoXchInputs);
    }

    // Step 5 + 6: aggregate + recover the aggregated PK.
    // The aggregated PK is recovered via SecretKey::from_bytes round-trip on
    // the ScalarField bytes, NOT by hand-summing the input PKs (which would
    // diverge from the SK sum on mod-r wraparound). The .expect is acceptable
    // because ScalarField guarantees the bytes are < r and the zero-aggregate
    // probability is ~ 2^-255.
    let aggregated_sender_sk = aggregate_sender_sks(&sender_sks);
    let agg_pk = SecretKey::from_bytes(aggregated_sender_sk.as_bytes())
        .expect("ScalarField guarantees < r; zero aggregate has vanishing probability")
        .public_key();

    // Step 7: input_hash binding over lex-min coin_id + aggregated PK.
    let input_hash = compute_input_hash(&xch_input_ids, &agg_pk);

    // Borrow-checker workaround: take ownership of the pending Vec so the
    // per-pending loop can iterate it while mutating spends.xch.items and
    // spends.outputs.xch freely. After this take, silent_payments_pending is
    // an empty Vec; prepare() does not re-read it.
    let pending = std::mem::take(&mut spends.silent_payments_pending);

    // Step 8: per-pending derivation + CreateCoin emission.
    for p in &pending {
        let ph = derive_one_time_puzzle_hash(
            &p.scan_pk,
            &p.spend_pk,
            &aggregated_sender_sk,
            &input_hash,
            p.k,
        );

        let create_coin = CreateCoin::new(ph, p.amount, p.memos);

        // Emit the CreateCoin condition on the recorded parent. The
        // p.parent_puzzle_hash was captured at apply time via
        // parent.asset.full_puzzle_hash().
        let parent = &mut spends.xch.items[p.parent_xch_index];
        parent.kind.create_coin_with_assertion(
            ctx,
            p.parent_puzzle_hash,
            &mut spends.xch.payment_assertions,
            create_coin,
        );

        // Record the resulting output coin (parent_coin_id was captured at
        // apply time when the parent was selected, before any intermediate
        // ephemeral coins could shift indices).
        spends
            .outputs
            .xch
            .push(Coin::new(p.parent_coin_id, ph, p.amount));
    }

    // No SP-specific concurrent binding is emitted here. The XCH-only invariant
    // (GATE 0) guarantees the bundle contains only XCH conditions-spends, so the
    // general `emit_relation` AssertConcurrent cycle (run later in `prepare`)
    // binds exactly the non-ephemeral XCH input set that `compute_input_hash`
    // above hashed — which is precisely the strongly connected component the
    // receiver reconstructs over standard-puzzle spends.
    Ok(())
}

impl Spends<Finished> {
    pub fn unspent(&self) -> Vec<(SpendableAsset, SpendKind)> {
        let mut result = Vec::new();

        for item in &self.xch.items {
            result.push((SpendableAsset::Xch(item.asset), item.kind.clone()));
        }

        for cat in self.cats.values() {
            for item in &cat.items {
                result.push((SpendableAsset::Cat(item.asset), item.kind.clone()));
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
                cat_spends.push(CatSpend::new(item.asset, spend));
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

    /// Pinning test for `Relation::AssertConcurrent` — verifies the exact
    /// closed-cycle opcode-64 emission shape that CHIP-0057 Pass 2b scanners
    /// depend on. Drift in `emit_relation`'s implementation away from the
    /// closed cycle will silently break SP scanner detection for cross-
    /// derivation-index multi-input sends; this test fires before any such
    /// regression can ship.
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
