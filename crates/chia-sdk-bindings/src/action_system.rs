use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{Memos, offer::SettlementPaymentsSolution};
use chia_sdk_driver::{
    self as sdk, Cat, Delta, HashedPtr, Layer, OptionType, SettlementLayer, SpendContext, SpendKind,
};
use chia_sdk_types::{Condition, conditions::TradePrice};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;

use crate::{
    AsProgram, AsPtr, Clvm, Did, Nft, NotarizedPayment, Offer, OptionContract, Program, Spend,
};

/// Mirrors [`sdk::Relation`], but `None` is renamed since it's a reserved word in Python.
#[derive(Clone, Copy)]
pub enum Relation {
    Unrelated,
    AssertConcurrent,
    CoinAnnouncementRing,
    CoinAnnouncementHub,
}

impl From<Relation> for sdk::Relation {
    fn from(value: Relation) -> Self {
        match value {
            Relation::Unrelated => sdk::Relation::None,
            Relation::AssertConcurrent => sdk::Relation::AssertConcurrent,
            Relation::CoinAnnouncementRing => sdk::Relation::CoinAnnouncementRing,
            Relation::CoinAnnouncementHub => sdk::Relation::CoinAnnouncementHub,
        }
    }
}

#[derive(Clone)]
pub struct Spends {
    spends: Arc<Mutex<sdk::Spends>>,
    clvm: Arc<Mutex<SpendContext>>,
}

impl Spends {
    pub fn new(clvm: Clvm, change_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self {
            spends: Arc::new(Mutex::new(sdk::Spends::new(change_puzzle_hash))),
            clvm: clvm.0.clone(),
        })
    }

    pub fn with_separate_change_puzzle_hash(
        clvm: Clvm,
        intermediate_puzzle_hash: Bytes32,
        change_puzzle_hash: Bytes32,
    ) -> Result<Self> {
        Ok(Self {
            spends: Arc::new(Mutex::new(sdk::Spends::with_separate_change_puzzle_hash(
                intermediate_puzzle_hash,
                change_puzzle_hash,
            ))),
            clvm: clvm.0.clone(),
        })
    }

    pub fn add_xch(&self, coin: Coin) -> Result<()> {
        self.spends.lock().unwrap().add(coin);

        Ok(())
    }

    pub fn add_cat(&self, cat: Cat) -> Result<()> {
        self.spends.lock().unwrap().add(cat);

        Ok(())
    }

    pub fn add_cat_for_revocation(&self, cat: Cat) -> Result<()> {
        self.spends.lock().unwrap().add_for_revocation(cat)?;

        Ok(())
    }

    /// Adds the settlement coins offered by the maker, for the taker of an offer.
    pub fn add_offered_coins(&self, offer: Offer) -> Result<()> {
        self.spends
            .lock()
            .unwrap()
            .add(offer.inner.offered_coins().clone());

        Ok(())
    }

    pub fn add_did(&self, did: Did) -> Result<()> {
        let ctx = self.clvm.lock().unwrap();
        let sdk_did = did.as_ptr(&ctx);
        self.spends.lock().unwrap().add(sdk_did);

        Ok(())
    }

    pub fn add_nft(&self, nft: Nft) -> Result<()> {
        let ctx = self.clvm.lock().unwrap();
        let sdk_nft = nft.as_ptr(&ctx);
        self.spends.lock().unwrap().add(sdk_nft);

        Ok(())
    }

    pub fn add_option(&self, option: OptionContract) -> Result<()> {
        self.spends
            .lock()
            .unwrap()
            .add(sdk::OptionContract::from(option));

        Ok(())
    }

    pub fn p2_puzzle_hashes(&self) -> Result<Vec<Bytes32>> {
        Ok(self.spends.lock().unwrap().p2_puzzle_hashes())
    }

    pub fn non_settlement_coin_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.spends.lock().unwrap().non_settlement_coin_ids())
    }

    pub fn add_optional_condition(&self, condition: Program) -> Result<()> {
        let ctx = self.clvm.lock().unwrap();
        let condition = Condition::<NodePtr>::from_clvm(&ctx, condition.1)?;

        self.spends
            .lock()
            .unwrap()
            .conditions
            .optional
            .push(condition);

        Ok(())
    }

    pub fn add_required_condition(&self, condition: Program) -> Result<()> {
        let ctx = self.clvm.lock().unwrap();
        let condition = Condition::<NodePtr>::from_clvm(&ctx, condition.1)?;

        self.spends
            .lock()
            .unwrap()
            .conditions
            .required
            .push(condition);

        Ok(())
    }

    pub fn disable_settlement_assertions(&self) -> Result<()> {
        self.spends
            .lock()
            .unwrap()
            .conditions
            .disable_settlement_assertions = true;

        Ok(())
    }

    pub fn selected_xch_amount(&self) -> Result<u128> {
        Ok(self.spends.lock().unwrap().xch.selected_amount())
    }

    pub fn selected_asset_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self
            .spends
            .lock()
            .unwrap()
            .cats
            .values()
            .filter_map(|cat| Some(cat.items.first()?.asset.info.asset_id))
            .collect())
    }

    pub fn selected_cat_amount(&self, asset_id: Bytes32) -> Result<u128> {
        Ok(self
            .spends
            .lock()
            .unwrap()
            .cats
            .values()
            .find_map(|cat| {
                if cat.items.first()?.asset.info.asset_id == asset_id {
                    Some(cat.selected_amount())
                } else {
                    None
                }
            })
            .unwrap_or(0))
    }

    pub fn apply(&self, actions: Vec<Action>) -> Result<Deltas> {
        let mut ctx = self.clvm.lock().unwrap();

        let deltas = self.spends.lock().unwrap().apply(
            &mut ctx,
            &actions.into_iter().map(|a| a.0).collect::<Vec<_>>(),
        )?;

        Ok(Deltas(deltas))
    }

    pub fn prepare(&self, deltas: Deltas, relation: Option<Relation>) -> Result<FinishedSpends> {
        let mut spends = self.spends.lock().unwrap();

        let empty = sdk::Spends::with_separate_change_puzzle_hash(
            spends.intermediate_puzzle_hash,
            spends.change_puzzle_hash,
        );
        let spends = std::mem::replace(&mut *spends, empty);

        let mut ctx = self.clvm.lock().unwrap();

        let spends = spends.prepare(
            &mut ctx,
            &deltas.0,
            relation.map_or(sdk::Relation::None, Into::into),
        )?;

        let mut finished = HashMap::new();

        for (asset, kind) in spends.unspent() {
            let SpendKind::Settlement(settlement) = kind else {
                continue;
            };

            finished.insert(
                asset.coin().coin_id(),
                SettlementLayer.construct_spend(
                    &mut ctx,
                    SettlementPaymentsSolution::new(settlement.finish()),
                )?,
            );
        }

        Ok(FinishedSpends {
            spends: Arc::new(Mutex::new(spends)),
            clvm: self.clvm.clone(),
            finished: Arc::new(Mutex::new(finished)),
        })
    }
}

#[derive(Clone)]
pub struct FinishedSpends {
    spends: Arc<Mutex<sdk::Spends<sdk::Finished>>>,
    clvm: Arc<Mutex<SpendContext>>,
    finished: Arc<Mutex<HashMap<Bytes32, sdk::Spend>>>,
}

impl FinishedSpends {
    pub fn pending_spends(&self) -> Result<Vec<PendingSpend>> {
        let mut ctx = self.clvm.lock().unwrap();
        let mut pending = Vec::new();

        let spends = self.spends.lock().unwrap();

        for (asset, kind) in spends.unspent() {
            let SpendKind::Conditions(spend) = kind else {
                continue;
            };

            let mut conditions = Vec::new();

            for condition in spend.clone().finish() {
                conditions.push(Program(self.clvm.clone(), condition.to_clvm(&mut ctx)?));
            }

            pending.push(PendingSpend {
                asset,
                conditions,
                clvm: self.clvm.clone(),
            });
        }

        Ok(pending)
    }

    pub fn insert(&self, coin_id: Bytes32, spend: Spend) -> Result<()> {
        self.finished
            .lock()
            .unwrap()
            .insert(coin_id, sdk::Spend::new(spend.puzzle.1, spend.solution.1));

        Ok(())
    }

    pub fn spend(&self) -> Result<Outputs> {
        let mut ctx = self.clvm.lock().unwrap();

        let spends = self.spends.lock().unwrap().clone();
        let finished = self.finished.lock().unwrap().clone();

        let outputs = spends.spend(&mut ctx, finished)?;
        Ok(Outputs {
            inner: outputs,
            clvm: self.clvm.clone(),
        })
    }
}

#[derive(Clone)]
pub struct PendingSpend {
    asset: sdk::SpendableAsset,
    conditions: Vec<Program>,
    clvm: Arc<Mutex<SpendContext>>,
}

impl PendingSpend {
    pub fn p2_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.asset.p2_puzzle_hash())
    }

    pub fn coin(&self) -> Result<Coin> {
        Ok(self.asset.coin())
    }

    pub fn conditions(&self) -> Result<Vec<Program>> {
        Ok(self.conditions.clone())
    }

    pub fn as_xch(&self) -> Result<Option<Coin>> {
        match self.asset {
            sdk::SpendableAsset::Xch(coin) => Ok(Some(coin)),
            _ => Ok(None),
        }
    }

    pub fn as_cat(&self) -> Result<Option<Cat>> {
        match self.asset {
            sdk::SpendableAsset::Cat(cat) | sdk::SpendableAsset::RevokedCat(cat) => Ok(Some(cat)),
            _ => Ok(None),
        }
    }

    pub fn is_revocation(&self) -> Result<bool> {
        Ok(matches!(self.asset, sdk::SpendableAsset::RevokedCat(_)))
    }

    pub fn as_did(&self) -> Result<Option<Did>> {
        match self.asset {
            sdk::SpendableAsset::Did(did) => Ok(Some(did.as_program(&self.clvm))),
            _ => Ok(None),
        }
    }

    pub fn as_nft(&self) -> Result<Option<Nft>> {
        match self.asset {
            sdk::SpendableAsset::Nft(nft) => Ok(Some(nft.as_program(&self.clvm))),
            _ => Ok(None),
        }
    }

    pub fn as_option(&self) -> Result<Option<OptionContract>> {
        match self.asset {
            sdk::SpendableAsset::Option(option) => Ok(Some(option.into())),
            _ => Ok(None),
        }
    }
}

#[derive(Clone)]
pub struct Action(sdk::Action);

impl From<sdk::Action> for Action {
    fn from(action: sdk::Action) -> Self {
        Self(action)
    }
}

impl Action {
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Option<Program>) -> Result<Self> {
        Ok(Self(sdk::Action::send(
            id.0,
            puzzle_hash,
            amount,
            memos.map_or(Memos::None, |memos| Memos::Some(memos.1)),
        )))
    }

    pub fn burn(id: Id, amount: u64, memos: Option<Program>) -> Result<Self> {
        Ok(Self(sdk::Action::burn(
            id.0,
            amount,
            memos.map_or(Memos::None, |memos| Memos::Some(memos.1)),
        )))
    }

    pub fn settle(id: Id, notarized_payment: NotarizedPayment) -> Result<Self> {
        Ok(Self(sdk::Action::settle(id.0, notarized_payment.into())))
    }

    pub fn settle_royalty(
        clvm: Clvm,
        id: Id,
        launcher_id: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_amount: u64,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();

        Ok(Self(sdk::Action::settle_royalty(
            &mut ctx,
            id.0,
            launcher_id,
            royalty_puzzle_hash,
            royalty_amount,
        )?))
    }

    pub fn create_did(
        metadata: Program,
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        amount: u64,
    ) -> Result<Self> {
        let ctx = metadata.0.lock().unwrap();

        Ok(Self(sdk::Action::create_did(
            recovery_list_hash,
            num_verifications_required,
            metadata.as_ptr(&ctx),
            amount,
        )))
    }

    pub fn create_empty_did() -> Result<Self> {
        Ok(Self(sdk::Action::create_empty_did()))
    }

    /// Fields that are `None` are left unchanged. The recovery list hash can't be set and removed
    /// at the same time.
    pub fn update_did(
        id: Id,
        new_metadata: Option<Program>,
        new_recovery_list_hash: Option<Bytes32>,
        new_num_verifications_required: Option<u64>,
        remove_recovery_list_hash: Option<bool>,
    ) -> Result<Self> {
        let new_recovery_list_hash = match (
            new_recovery_list_hash,
            remove_recovery_list_hash.unwrap_or(false),
        ) {
            (Some(_), true) => {
                return Err(bindy::Error::Custom(
                    "cannot both set and remove the recovery list hash".to_string(),
                ));
            }
            (Some(hash), false) => Some(Some(hash)),
            (None, true) => Some(None),
            (None, false) => None,
        };

        let new_metadata = new_metadata.map(|metadata| {
            let ctx = metadata.0.lock().unwrap();
            metadata.as_ptr(&ctx)
        });

        Ok(Self(sdk::Action::update_did(
            id.0,
            new_recovery_list_hash,
            new_num_verifications_required,
            new_metadata,
        )))
    }

    pub fn mint_option(
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        underlying_id: Id,
        underlying_amount: u64,
        strike_type: OptionType,
        amount: u64,
    ) -> Result<Self> {
        Ok(Self(sdk::Action::mint_option(
            creator_puzzle_hash,
            seconds,
            underlying_id.0,
            underlying_amount,
            strike_type,
            amount,
        )))
    }

    pub fn melt_singleton(id: Id, amount: u64) -> Result<Self> {
        Ok(Self(sdk::Action::melt_singleton(id.0, amount)))
    }

    pub fn issue_cat(
        tail_spend: Spend,
        hidden_puzzle_hash: Option<Bytes32>,
        amount: u64,
    ) -> Result<Self> {
        Ok(Self(sdk::Action::issue_cat(
            tail_spend.into(),
            hidden_puzzle_hash,
            amount,
        )))
    }

    pub fn single_issue_cat(hidden_puzzle_hash: Option<Bytes32>, amount: u64) -> Result<Self> {
        Ok(Self(sdk::Action::single_issue_cat(
            hidden_puzzle_hash,
            amount,
        )))
    }

    pub fn run_tail(id: Id, tail_spend: Spend, supply_delta: Delta) -> Result<Self> {
        Ok(Self(sdk::Action::run_tail(
            id.0,
            tail_spend.into(),
            supply_delta,
        )))
    }

    pub fn fee(amount: u64) -> Result<Self> {
        Ok(Self(sdk::Action::fee(amount)))
    }

    pub fn mint_nft(
        clvm: Clvm,
        metadata: Program,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        amount: u64,
        parent_id: Option<Id>,
    ) -> Result<Self> {
        let ctx = clvm.0.lock().unwrap();
        let hashed: HashedPtr = metadata.as_ptr(&ctx);

        let sdk_action = match &parent_id {
            Some(parent) => sdk::Action::mint_nft_from_did(
                parent.0,
                hashed,
                metadata_updater_puzzle_hash,
                royalty_puzzle_hash,
                royalty_basis_points,
                amount,
            ),
            None => sdk::Action::mint_nft(
                hashed,
                metadata_updater_puzzle_hash,
                royalty_puzzle_hash,
                royalty_basis_points,
                amount,
            ),
        };

        Ok(Self(sdk_action))
    }

    pub fn update_nft(
        id: Id,
        metadata_update_spends: Vec<Spend>,
        transfer: Option<TransferNftById>,
    ) -> Result<Self> {
        let sdk_spends: Vec<sdk::Spend> =
            metadata_update_spends.into_iter().map(Into::into).collect();

        let transfer =
            transfer.map(|t| sdk::TransferNftById::new(t.owner_id.map(|o| o.0), t.trade_prices));

        Ok(Self(sdk::Action::update_nft(id.0, sdk_spends, transfer)))
    }
}

#[derive(Clone)]
pub struct Deltas(sdk::Deltas);

impl Deltas {
    pub fn from_actions(actions: Vec<Action>) -> Result<Deltas> {
        let sdk_actions: Vec<sdk::Action> = actions.into_iter().map(|a| a.0).collect();
        let deltas = sdk::Deltas::from_actions(&sdk_actions);
        Ok(Deltas(deltas))
    }

    pub fn get(&self, id: Id) -> Result<Option<Delta>> {
        Ok(self.0.get(&id.0).copied())
    }

    pub fn is_needed(&self, id: Id) -> Result<bool> {
        Ok(self.0.is_needed(&id.0))
    }

    pub fn ids(&self) -> Result<Vec<Id>> {
        Ok(self.0.ids().copied().map(Id).collect())
    }
}

#[derive(Clone, Debug)]
pub struct Id(sdk::Id);

impl Id {
    pub fn xch() -> Result<Self> {
        Ok(Self(sdk::Id::Xch))
    }

    pub fn existing(asset_id: Bytes32) -> Result<Self> {
        Ok(Self(sdk::Id::Existing(asset_id)))
    }

    pub fn new(index: usize) -> Result<Self> {
        Ok(Self(sdk::Id::New(index)))
    }

    pub fn is_xch(&self) -> Result<bool> {
        Ok(self.0 == sdk::Id::Xch)
    }

    pub fn as_existing(&self) -> Result<Option<Bytes32>> {
        Ok(match self.0 {
            sdk::Id::Existing(asset_id) => Some(asset_id),
            _ => None,
        })
    }

    pub fn as_new(&self) -> Result<Option<usize>> {
        Ok(match self.0 {
            sdk::Id::New(index) => Some(index),
            _ => None,
        })
    }

    pub fn equals(&self, id: Id) -> Result<bool> {
        Ok(self.0 == id.0)
    }
}

#[derive(Clone)]
pub struct Outputs {
    inner: sdk::Outputs,
    clvm: Arc<Mutex<SpendContext>>,
}

impl Outputs {
    pub fn xch(&self) -> Result<Vec<Coin>> {
        Ok(self.inner.xch.clone())
    }

    pub fn cats(&self) -> Result<Vec<Id>> {
        Ok(self.inner.cats.keys().copied().map(Id).collect())
    }

    pub fn cat(&self, id: Id) -> Result<Vec<Cat>> {
        Ok(self.inner.cats.get(&id.0).cloned().unwrap_or_default())
    }

    pub fn nfts(&self) -> Result<Vec<Id>> {
        Ok(self.inner.nfts.keys().copied().map(Id).collect())
    }

    pub fn nft(&self, id: Id) -> Result<Nft> {
        let sdk_nft = self
            .inner
            .nfts
            .get(&id.0)
            .copied()
            .ok_or_else(|| bindy::Error::Custom("NFT not found in outputs".to_string()))?;
        Ok(sdk_nft.as_program(&self.clvm))
    }

    pub fn dids(&self) -> Result<Vec<Id>> {
        Ok(self.inner.dids.keys().copied().map(Id).collect())
    }

    pub fn did(&self, id: Id) -> Result<Did> {
        let sdk_did = self
            .inner
            .dids
            .get(&id.0)
            .copied()
            .ok_or_else(|| bindy::Error::Custom("DID not found in outputs".to_string()))?;
        Ok(sdk_did.as_program(&self.clvm))
    }

    pub fn options(&self) -> Result<Vec<Id>> {
        Ok(self.inner.options.keys().copied().map(Id).collect())
    }

    pub fn option(&self, id: Id) -> Result<OptionContract> {
        let sdk_option = self
            .inner
            .options
            .get(&id.0)
            .copied()
            .ok_or_else(|| bindy::Error::Custom("option not found in outputs".to_string()))?;
        Ok(sdk_option.into())
    }

    pub fn fee(&self) -> Result<u64> {
        Ok(self.inner.fee)
    }

    pub fn reserved_fee(&self) -> Result<u64> {
        Ok(self.inner.reserved_fee)
    }
}

#[derive(Clone)]
pub struct TransferNftById {
    pub owner_id: Option<Id>,
    pub trade_prices: Vec<TradePrice>,
}
