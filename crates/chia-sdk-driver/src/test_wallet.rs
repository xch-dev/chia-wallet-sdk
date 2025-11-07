use std::{collections::HashMap, slice};

use anyhow::{Result, anyhow};
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{
    EveProof, LineageProof, Memos, Proof, cat::CatArgs, offer::SettlementPaymentsSolution,
    singleton::SingletonArgs,
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_test::Simulator;
use chia_sdk_types::{
    Conditions, MessageFlags, MessageSide, Mod,
    conditions::{CreateCoin, SendMessage},
    puzzles::{BlsMemberPuzzleAssert, SingletonMember, SingletonMemberSolution},
};
use chia_sdk_utils::select_coins;
use clvm_traits::ToClvm;
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    Action, Cat, Deltas, Id, InnerPuzzleSpend, Launcher, Layer, MipsSpend, Outputs, Puzzle,
    Relation, SettlementLayer, Spend, SpendContext, SpendKind, Spends, StandardLayer, Vault,
    VaultInfo, mips_puzzle_hash,
};

#[derive(Debug, Clone)]
pub struct TransactionData {
    pub outputs: Outputs,
    pub delegated_spend: Spend,
    pub coin_spends: Vec<CoinSpend>,
    pub vault_spend: CoinSpend,
}

#[derive(Debug, Clone)]
pub struct TestVault {
    info: VaultInfo,
    puzzle_hash: Bytes32,
    secret_key: SecretKey,
}

impl TestVault {
    pub fn mint(sim: &mut Simulator, ctx: &mut SpendContext, balance: u64) -> Result<Self> {
        let pair = sim.bls(balance + 1);
        let p2 = StandardLayer::new(pair.pk);

        let (mut parent_conditions, vault) = Launcher::new(pair.coin.coin_id(), 1).mint_vault(
            ctx,
            vault_custody_puzzle_hash(pair.pk),
            (),
        )?;

        let puzzle_hash = vault_p2_puzzle_hash(vault.info.launcher_id).into();

        if balance > 0 {
            parent_conditions.push(CreateCoin::new(
                puzzle_hash,
                pair.coin.amount - 1,
                Memos::None,
            ));
        }

        p2.spend(ctx, pair.coin, parent_conditions)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&pair.sk))?;

        Ok(Self {
            info: vault.info,
            puzzle_hash,
            secret_key: pair.sk,
        })
    }

    pub fn spend(
        &self,
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        actions: &[Action],
    ) -> Result<TransactionData> {
        let spends = Spends::new(self.puzzle_hash);
        self.custom_spend(sim, ctx, actions, spends, Conditions::new())
    }

    pub fn custom_spend(
        &self,
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        actions: &[Action],
        mut spends: Spends,
        mut vault_conditions: Conditions,
    ) -> Result<TransactionData> {
        let deltas = Deltas::from_actions(actions);

        for &id in deltas.ids() {
            let delta = deltas.get(&id).copied().unwrap_or_default();

            let mut required_amount = delta.output.saturating_sub(delta.input);

            if deltas.is_needed(&id) && required_amount == 0 {
                required_amount = 1;
            }

            if required_amount > 0 {
                match id {
                    Id::Xch => {
                        for coin in select_coins(self.fetch_xch(sim), required_amount)? {
                            spends.add(coin);
                        }
                    }
                    Id::Existing(asset_id) => {
                        for coin in
                            select_coins(self.fetch_cat_coins(sim, asset_id), required_amount)?
                        {
                            let cat = fetch_cat(sim, coin)?;
                            spends.add(cat);
                        }
                    }
                    Id::New(_) => {}
                }
            }
        }

        let deltas = spends.apply(ctx, actions)?;

        let spends = spends.prepare(ctx, &deltas, Relation::None)?;

        let mut coin_spends = HashMap::new();

        for (asset, kind) in spends.unspent() {
            match kind {
                SpendKind::Conditions(spend) => {
                    let delegated_spend = ctx.delegated_spend(spend.finish())?;

                    let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
                        | MessageFlags::COIN.encode(MessageSide::Receiver);

                    let coin_id = ctx.alloc(&asset.coin().coin_id())?;

                    vault_conditions.push(SendMessage::new(
                        mode,
                        ctx.tree_hash(delegated_spend.puzzle).to_vec().into(),
                        vec![coin_id],
                    ));

                    let mut mips_spend = MipsSpend::new(delegated_spend);

                    let puzzle = ctx.curry(SingletonMember::new(self.info.launcher_id))?;
                    let solution = ctx.alloc(&SingletonMemberSolution::new(
                        vault_custody_puzzle_hash(self.secret_key.public_key()).into(),
                        1,
                    ))?;
                    let custody_hash = vault_p2_puzzle_hash(self.info.launcher_id);

                    mips_spend.members.insert(
                        custody_hash,
                        InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, solution)),
                    );

                    let spend = mips_spend.spend(ctx, custody_hash)?;

                    coin_spends.insert(asset.coin().coin_id(), spend);
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

        let outputs = spends.spend(ctx, coin_spends)?;
        let coin_spends = ctx.take();

        let vault = fetch_vault(sim, self.info.launcher_id, self.info.custody_hash.into())?;

        let delegated_spend = ctx.delegated_spend(vault_conditions.create_coin(
            self.info.custody_hash.into(),
            vault.coin.amount,
            Memos::None,
        ))?;

        let mut mips_spend = MipsSpend::new(delegated_spend);

        let puzzle = ctx.curry(BlsMemberPuzzleAssert::new(self.secret_key.public_key()))?;

        mips_spend.members.insert(
            self.info.custody_hash,
            InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, NodePtr::NIL)),
        );

        vault.spend(ctx, &mips_spend)?;

        let vault_spend = ctx.take().remove(0);

        sim.spend_coins(
            coin_spends
                .clone()
                .into_iter()
                .chain(vec![vault_spend.clone()])
                .collect(),
            slice::from_ref(&self.secret_key),
        )?;

        Ok(TransactionData {
            outputs,
            delegated_spend,
            coin_spends,
            vault_spend,
        })
    }

    pub fn puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    pub fn launcher_id(&self) -> Bytes32 {
        self.info.launcher_id
    }

    pub fn custody_hash(&self) -> TreeHash {
        self.info.custody_hash
    }

    fn fetch_xch(&self, sim: &Simulator) -> Vec<Coin> {
        sim.unspent_coins(self.puzzle_hash, false)
    }

    fn fetch_cat_coins(&self, sim: &Simulator, asset_id: Bytes32) -> Vec<Coin> {
        sim.unspent_coins(
            CatArgs::curry_tree_hash(asset_id, self.puzzle_hash.into()).into(),
            false,
        )
    }
}

fn vault_p2_puzzle_hash(launcher_id: Bytes32) -> TreeHash {
    mips_puzzle_hash(
        0,
        vec![],
        SingletonMember::new(launcher_id).curry_tree_hash(),
        true,
    )
}

fn vault_custody_puzzle_hash(pk: PublicKey) -> TreeHash {
    mips_puzzle_hash(
        0,
        vec![],
        BlsMemberPuzzleAssert::new(pk).curry_tree_hash(),
        true,
    )
}

fn fetch_cat(sim: &Simulator, coin: Coin) -> Result<Cat> {
    let mut allocator = Allocator::new();

    let parent_spend = sim
        .coin_spend(coin.parent_coin_info)
        .ok_or(anyhow!("missing parent spend"))?;
    let parent_puzzle = parent_spend.puzzle_reveal.to_clvm(&mut allocator)?;
    let parent_puzzle = Puzzle::parse(&allocator, parent_puzzle);
    let parent_solution = parent_spend.solution.to_clvm(&mut allocator)?;

    let children = Cat::parse_children(
        &mut allocator,
        parent_spend.coin,
        parent_puzzle,
        parent_solution,
    )?
    .ok_or(anyhow!("missing children"))?;

    let cat = children
        .iter()
        .find(|c| c.coin.coin_id() == coin.coin_id())
        .copied()
        .ok_or(anyhow!("missing cat"))?;

    Ok(cat)
}

fn fetch_vault(sim: &Simulator, launcher_id: Bytes32, custody_hash: Bytes32) -> Result<Vault> {
    let puzzle_hash = SingletonArgs::curry_tree_hash(launcher_id, custody_hash.into()).into();

    let coin = sim
        .unspent_coins(puzzle_hash, false)
        .into_iter()
        .next()
        .ok_or(anyhow!("missing vault coin"))?;

    let mut allocator = Allocator::new();

    let parent_spend = sim
        .coin_spend(coin.parent_coin_info)
        .ok_or(anyhow!("missing parent spend"))?;
    let parent_puzzle = parent_spend.puzzle_reveal.to_clvm(&mut allocator)?;
    let parent_puzzle = Puzzle::parse(&allocator, parent_puzzle);

    let proof = if parent_puzzle.curried_puzzle_hash() == SINGLETON_LAUNCHER_HASH.into() {
        Proof::Eve(EveProof {
            parent_parent_coin_info: parent_spend.coin.parent_coin_info,
            parent_amount: parent_spend.coin.amount,
        })
    } else {
        Proof::Lineage(LineageProof {
            parent_parent_coin_info: parent_spend.coin.parent_coin_info,
            parent_inner_puzzle_hash: custody_hash,
            parent_amount: parent_spend.coin.amount,
        })
    };

    Ok(Vault::new(
        coin,
        proof,
        VaultInfo::new(launcher_id, custody_hash.into()),
    ))
}
