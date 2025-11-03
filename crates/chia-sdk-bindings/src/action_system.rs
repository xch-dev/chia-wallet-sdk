use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{Memos, offer::SettlementPaymentsSolution};
use chia_sdk_driver::{
    self as sdk, Cat, Delta, HashedPtr, Layer, Relation, SettlementLayer, SpendContext, SpendKind,
};
use chia_sdk_types::{Condition, conditions::TradePrice};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;

use crate::{AsProgram, AsPtr, Clvm, Did, Nft, NotarizedPayment, OptionContract, Program, Spend};

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

    pub fn add_xch(&self, coin: Coin) -> Result<()> {
        self.spends.lock().unwrap().add(coin);

        Ok(())
    }

    pub fn add_cat(&self, cat: Cat) -> Result<()> {
        self.spends.lock().unwrap().add(cat);

        Ok(())
    }

    pub fn add_nft(&self, nft: Nft) -> Result<()> {
        let ctx = self.clvm.lock().unwrap();
        let sdk_nft = nft.as_ptr(&ctx);
        self.spends.lock().unwrap().add(sdk_nft);

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

    pub fn selected_xch_amount(&self) -> Result<u64> {
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

    pub fn selected_cat_amount(&self, asset_id: Bytes32) -> Result<u64> {
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

    pub fn prepare(&self, deltas: Deltas) -> Result<FinishedSpends> {
        let mut spends = self.spends.lock().unwrap();

        let change_puzzle_hash = spends.change_puzzle_hash;
        let spends = std::mem::replace(&mut *spends, sdk::Spends::new(change_puzzle_hash));

        let mut ctx = self.clvm.lock().unwrap();

        let spends = spends.prepare(&mut ctx, &deltas.0, Relation::None)?;

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
            sdk::SpendableAsset::Cat(cat) => Ok(Some(cat)),
            _ => Ok(None),
        }
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

impl Action {
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Option<Program>) -> Result<Self> {
        Ok(Self(sdk::Action::send(
            id.0,
            puzzle_hash,
            amount,
            memos.map_or(Memos::None, |memos| Memos::Some(memos.1)),
        )))
    }

    pub fn settle(id: Id, notarized_payment: NotarizedPayment) -> Result<Self> {
        Ok(Self(sdk::Action::settle(id.0, notarized_payment.into())))
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

    pub fn mint_nft(clvm: Clvm, params: MintNftParams) -> Result<Self> {
        let ctx = clvm.0.lock().unwrap();
        let hashed: HashedPtr = params.metadata.as_ptr(&ctx);

        let sdk_action = match &params.parent_did_id {
            Some(parent) => sdk::Action::mint_nft_from_did(
                parent.0,
                hashed,
                params.metadata_updater_puzzle_hash,
                params.royalty_puzzle_hash,
                params.royalty_basis_points,
                params.amount,
            ),
            None => sdk::Action::mint_nft(
                hashed,
                params.metadata_updater_puzzle_hash,
                params.royalty_puzzle_hash,
                params.royalty_basis_points,
                params.amount,
            ),
        };

        Ok(Self(sdk_action))
    }

    pub fn update_nft(clvm: Clvm, id: Id, params: UpdateNftParams) -> Result<Self> {
        let updater_puzzle = clvm.nft_metadata_updater_default()?;

        let mut spends: Vec<Spend> = Vec::new();
        for (selector, uri_opt) in [
            ("u", &params.new_data_uri),
            ("mu", &params.new_metadata_uri),
            ("lu", &params.new_license_uri),
        ] {
            if let Some(uri) = uri_opt.as_ref() {
                let code = clvm.string(selector.to_string())?;
                let value = clvm.string(uri.clone())?;
                let solution = clvm.pair(code, value)?;
                spends.push(Spend { puzzle: updater_puzzle.clone(), solution });
            }
        }

        let transfer = if params.transfer_did_id.is_some() || !params.transfer_trade_prices.is_empty() {
            Some(sdk::TransferNftById::new(
                params.transfer_did_id.map(|d| d.0),
                params.transfer_trade_prices,
            ))
        } else {
            None
        };

        Ok(Self(sdk::Action::update_nft(
            id.0,
            spends.into_iter().map(Into::into).collect(),
            transfer,
        )))
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
            .cloned()
            .ok_or_else(|| bindy::Error::Custom("NFT not found in outputs".to_string()))?;
        Ok(sdk_nft.as_program(&self.clvm))
    }
}

#[derive(Clone)]
pub struct MintNftParams {
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub parent_did_id: Option<Id>,
    pub amount: u64,
}

#[derive(Clone)]
pub struct UpdateNftParams {
    pub new_data_uri: Option<String>,
    pub new_metadata_uri: Option<String>,
    pub new_license_uri: Option<String>,
    pub transfer_did_id: Option<Id>,
    pub transfer_trade_prices: Vec<TradePrice>,
}
