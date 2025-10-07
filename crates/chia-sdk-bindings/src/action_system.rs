use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{offer::SettlementPaymentsSolution, Memos};
use chia_sdk_driver::{
    self as sdk, Cat, Delta, Layer, Relation, SettlementLayer, SpendContext, SpendKind,
};
use chia_sdk_types::Condition;
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;

use crate::{AsProgram, Clvm, Did, Nft, OptionContract, Program, Spend};

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

    pub fn spend(&self) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let spends = self.spends.lock().unwrap().clone();
        let finished = self.finished.lock().unwrap().clone();

        spends.spend(&mut ctx, finished)?;

        Ok(())
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
    pub fn send_xch(puzzle_hash: Bytes32, amount: u64) -> Result<Self> {
        Ok(Self(sdk::Action::send(
            sdk::Id::Xch,
            puzzle_hash,
            amount,
            Memos::None,
        )))
    }

    pub fn fee(amount: u64) -> Result<Self> {
        Ok(Self(sdk::Action::fee(amount)))
    }
}

#[derive(Clone)]
pub struct Deltas(sdk::Deltas);

impl Deltas {
    pub fn xch(&self) -> Result<Option<Delta>> {
        Ok(self.0.get(&sdk::Id::Xch).copied())
    }
}
