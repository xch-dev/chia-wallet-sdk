use std::{collections::HashMap, slice};

use anyhow::{Result, anyhow};
use chia_bls::{PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    EveProof, LineageProof, Memos, Proof,
    cat::{CatArgs, CatSolution},
    offer::SettlementPaymentsSolution,
    singleton::{SingletonArgs, SingletonStruct},
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_test::{Simulator, sign_transaction};
use chia_sdk_types::{
    Condition, Conditions, MessageFlags, MessageSide, Mod,
    conditions::{CreateCoin, SendMessage},
    puzzles::{BlsMember, EverythingWithSingletonTailArgs, RevocationArgs},
};
use chia_sdk_utils::select_coins;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    Action, Cat, ClawbackV2, CurriedPuzzle, Deltas, Id, InnerPuzzleSpend, Launcher, Layer,
    MipsSpend, Nft, Outputs, P2ConditionsOrSingleton, P2Singleton, Puzzle, Relation,
    SettlementLayer, Spend, SpendContext, SpendKind, Spends, StandardLayer, Vault, VaultInfo,
    mips_puzzle_hash,
};

#[derive(Debug, Clone)]
pub struct TransactionData {
    pub outputs: Outputs,
    pub delegated_spend: Spend,
    pub coin_spends: Vec<CoinSpend>,
    pub vault_spend: CoinSpend,
    pub signature: Signature,
}

#[derive(Debug, Clone, Copy)]
pub enum TestP2Puzzle {
    P2ConditionsOrSingleton(P2ConditionsOrSingleton),
    P2Singleton(P2Singleton),
    Clawback(ClawbackV2),
}

#[derive(Debug, Clone)]
pub struct TestVault {
    pub info: VaultInfo,
    pub p2_puzzle_hash: Bytes32,
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
    pub p2_puzzles: HashMap<TreeHash, TestP2Puzzle>,
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

        let mut p2_puzzles = HashMap::new();

        let p2_singleton = P2Singleton::new(vault.info.launcher_id, 0);
        let p2_puzzle_hash = p2_singleton.tree_hash();

        p2_puzzles.insert(p2_puzzle_hash, TestP2Puzzle::P2Singleton(p2_singleton));

        parent_conditions.push(CreateCoin::new(
            p2_puzzle_hash.into(),
            pair.coin.amount - 1,
            Memos::None,
        ));

        p2.spend(ctx, pair.coin, parent_conditions)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&pair.sk))?;

        Ok(Self {
            info: vault.info,
            p2_puzzle_hash: p2_puzzle_hash.into(),
            secret_key: pair.sk,
            public_key: pair.pk,
            p2_puzzles,
        })
    }

    pub fn spend(
        &self,
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        actions: &[Action],
    ) -> Result<TransactionData> {
        let mut spends = Spends::new(self.p2_puzzle_hash);
        self.select_coins(sim, &mut spends, &Deltas::from_actions(actions))?;
        self.custom_spend(sim, ctx, actions, spends, Conditions::new())
    }

    pub fn select_coins(
        &self,
        sim: &Simulator,
        spends: &mut Spends,
        deltas: &Deltas,
    ) -> Result<()> {
        for &id in deltas.ids() {
            let delta = deltas.get(&id).copied().unwrap_or_default();

            let required_amount = delta.output.saturating_sub(delta.input);

            if required_amount == 0 && !deltas.is_needed(&id) {
                continue;
            }

            match id {
                Id::Xch => {
                    for coin in select_coins(self.fetch_xch(sim), required_amount)? {
                        spends.add(coin);
                    }
                }
                Id::Existing(asset_id) => {
                    let mut is_nft = false;

                    for coin in self.fetch_hinted_coins(sim) {
                        let nft = try_fetch_nft(sim, coin)?;

                        if let Some(nft) = nft
                            && nft.info.launcher_id == asset_id
                        {
                            is_nft = true;
                            spends.add(nft);
                            break;
                        }
                    }

                    if is_nft {
                        continue;
                    }

                    for coin in select_coins(self.fetch_cat_coins(sim, asset_id), required_amount)?
                    {
                        let cat = fetch_cat(sim, coin)?;
                        spends.add(cat);
                    }
                }
                Id::New(_) => {}
            }
        }

        Ok(())
    }

    pub fn custom_spend(
        &self,
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        actions: &[Action],
        mut spends: Spends,
        mut vault_conditions: Conditions,
    ) -> Result<TransactionData> {
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

                    let spend = self.spend_p2_puzzle(
                        ctx,
                        asset.p2_puzzle_hash(),
                        delegated_spend,
                        sim.next_timestamp(),
                    )?;

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

        let tail_messages = vault_tail_messages(ctx, self.info.launcher_id, &coin_spends)?;
        for message in tail_messages {
            vault_conditions.push(message);
        }

        let vault = self.fetch_vault(sim)?;

        let delegated_spend = ctx.delegated_spend(vault_conditions.create_coin(
            self.info.custody_hash.into(),
            vault.coin.amount,
            Memos::None,
        ))?;

        let mut mips_spend = MipsSpend::new(delegated_spend);

        let puzzle = ctx.curry(BlsMember::new(self.secret_key.public_key()))?;

        mips_spend.members.insert(
            self.info.custody_hash,
            InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, NodePtr::NIL)),
        );

        vault.spend(ctx, &mips_spend)?;

        let vault_spend = ctx.take().remove(0);

        let bundle_coin_spends: Vec<CoinSpend> = coin_spends
            .clone()
            .into_iter()
            .chain(vec![vault_spend.clone()])
            .collect();

        let signature = sign_transaction(&bundle_coin_spends, slice::from_ref(&self.secret_key))?;
        let spend_bundle = SpendBundle::new(bundle_coin_spends, signature.clone());

        sim.new_transaction(spend_bundle)?;

        Ok(TransactionData {
            outputs,
            delegated_spend,
            coin_spends,
            vault_spend,
            signature,
        })
    }

    pub fn fetch_vault(&self, sim: &Simulator) -> Result<Vault> {
        fetch_vault(sim, self.info.launcher_id, self.info.custody_hash.into())
    }

    fn fetch_xch(&self, sim: &Simulator) -> Vec<Coin> {
        self.p2_puzzles
            .keys()
            .flat_map(|&p2_puzzle_hash| sim.unspent_coins(p2_puzzle_hash.into(), false))
            .collect()
    }

    fn fetch_cat_coins(&self, sim: &Simulator, asset_id: Bytes32) -> Vec<Coin> {
        self.p2_puzzles
            .keys()
            .flat_map(|&p2_puzzle_hash| {
                Self::fetch_cat_coins_for_p2_puzzle_hash(sim, p2_puzzle_hash.into(), asset_id)
            })
            .collect()
    }

    fn fetch_cat_coins_for_p2_puzzle_hash(
        sim: &Simulator,
        p2_puzzle_hash: Bytes32,
        asset_id: Bytes32,
    ) -> Vec<Coin> {
        let non_revocable = sim.unspent_coins(
            CatArgs::curry_tree_hash(asset_id, p2_puzzle_hash.into()).into(),
            false,
        );

        let revocable = sim.unspent_coins(
            CatArgs::curry_tree_hash(
                asset_id,
                RevocationArgs::new(Bytes32::default(), p2_puzzle_hash).curry_tree_hash(),
            )
            .into(),
            false,
        );

        [non_revocable, revocable].concat()
    }

    fn fetch_hinted_coins(&self, sim: &Simulator) -> Vec<Coin> {
        self.p2_puzzles
            .keys()
            .flat_map(|&p2_puzzle_hash| sim.unspent_coins(p2_puzzle_hash.into(), true))
            .collect()
    }

    fn spend_p2_puzzle(
        &self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        delegated_spend: Spend,
        timestamp: u64,
    ) -> Result<Spend> {
        let p2_puzzle = self
            .p2_puzzles
            .get(&p2_puzzle_hash.into())
            .expect("unknown p2 puzzle");

        Ok(match p2_puzzle {
            TestP2Puzzle::P2Singleton(p2_singleton) => {
                p2_singleton.spend(ctx, self.info.custody_hash.into(), 1, delegated_spend)?
            }
            TestP2Puzzle::P2ConditionsOrSingleton(p2_conditions_or_singleton) => {
                p2_conditions_or_singleton.p2_singleton_spend(
                    ctx,
                    self.info.custody_hash.into(),
                    1,
                    delegated_spend,
                )?
            }
            TestP2Puzzle::Clawback(clawback) => {
                if timestamp < clawback.seconds {
                    let inner_spend = self.spend_p2_puzzle(
                        ctx,
                        clawback.sender_puzzle_hash,
                        delegated_spend,
                        timestamp,
                    )?;

                    clawback.sender_spend(ctx, inner_spend)?
                } else {
                    let inner_spend = self.spend_p2_puzzle(
                        ctx,
                        clawback.receiver_puzzle_hash,
                        delegated_spend,
                        timestamp,
                    )?;

                    clawback.receiver_spend(ctx, inner_spend)?
                }
            }
        })
    }
}

fn vault_custody_puzzle_hash(pk: PublicKey) -> TreeHash {
    mips_puzzle_hash(0, vec![], BlsMember::new(pk).curry_tree_hash(), true)
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

fn vault_tail_messages(
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    coin_spends: &[CoinSpend],
) -> Result<Vec<SendMessage<NodePtr>>, anyhow::Error> {
    let expected_struct_hash = SingletonStruct::new(launcher_id).tree_hash();

    let mut messages = Vec::new();

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(ctx)?;
        let puzzle = Puzzle::parse(ctx, puzzle);
        let solution = coin_spend.solution.to_clvm(ctx)?;

        let Some((_cat, inner_puzzle, inner_solution)) =
            Cat::parse(ctx, coin_spend.coin, puzzle, solution)?
        else {
            continue;
        };

        let output = ctx.run(inner_puzzle.ptr(), inner_solution)?;
        let conditions: Vec<Condition> = ctx.extract(output)?;

        let Some(run_cat_tail) = conditions.iter().find_map(Condition::as_run_cat_tail) else {
            continue;
        };

        let Some(curried) = CurriedPuzzle::parse(ctx, run_cat_tail.program) else {
            continue;
        };
        if curried.mod_hash != EverythingWithSingletonTailArgs::mod_hash() {
            continue;
        }
        let args = ctx.extract::<EverythingWithSingletonTailArgs>(curried.args)?;
        if args.singleton_struct_hash != expected_struct_hash.into() {
            continue;
        }

        let cat_solution = CatSolution::<NodePtr>::from_clvm(ctx, solution)?;
        let extra_delta_ptr = ctx.alloc(&cat_solution.extra_delta)?;
        let extra_delta_bytes: Bytes = ctx.atom(extra_delta_ptr).as_ref().to_vec().into();

        let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
            | MessageFlags::COIN.encode(MessageSide::Receiver);
        let coin_id_ptr = ctx.alloc(&coin_spend.coin.coin_id())?;
        messages.push(SendMessage::new(mode, extra_delta_bytes, vec![coin_id_ptr]));
    }

    Ok(messages)
}

fn try_fetch_nft(sim: &Simulator, coin: Coin) -> Result<Option<Nft>> {
    let mut allocator = Allocator::new();

    let parent_spend = sim
        .coin_spend(coin.parent_coin_info)
        .ok_or(anyhow!("missing parent spend"))?;
    let parent_puzzle = parent_spend.puzzle_reveal.to_clvm(&mut allocator)?;
    let parent_puzzle = Puzzle::parse(&allocator, parent_puzzle);
    let parent_solution = parent_spend.solution.to_clvm(&mut allocator)?;

    Ok(Nft::parse_child(
        &mut allocator,
        parent_spend.coin,
        parent_puzzle,
        parent_solution,
    )?)
}
