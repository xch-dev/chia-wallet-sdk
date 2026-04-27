use std::{collections::HashMap, slice};

use anyhow::{Result, anyhow};
use chia_bls::{PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    EveProof, LineageProof, Memos, Proof,
    cat::CatArgs,
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
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    Action, Cat, CurriedPuzzle, Deltas, Id, InnerPuzzleSpend, Launcher, Layer, MipsSpend, Nft,
    Outputs, P2ConditionsOrSingleton, P2Singleton, Puzzle, Relation, SettlementLayer,
    SpendableAsset, Spend, SpendContext, SpendKind, Spends, StandardLayer, Vault, VaultInfo,
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

        if balance > 0 {
            parent_conditions.push(CreateCoin::new(
                p2_puzzle_hash.into(),
                pair.coin.amount - 1,
                Memos::None,
            ));
        }

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

        // Pre-compute the per-CAT-ring TAIL messages this vault must send. The CAT layer hands
        // `extra_delta = -total_ring_delta` to the TAIL puzzle of the coin that runs it; for an
        // `EverythingWithSingleton` TAIL, that delta is also the body of the `RECEIVE_MESSAGE` the
        // TAIL emits. We mirror `Cat::spend_all`'s ring math here so we can emit the matching
        // `SendMessage` from the vault before the cat ring is finalized.
        let unspent = spends.unspent();
        let tail_messages = compute_singleton_tail_messages(ctx, self.info.launcher_id, &unspent)?;
        for (coin_id, message_bytes) in tail_messages {
            let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
                | MessageFlags::COIN.encode(MessageSide::Receiver);
            let coin_id_node = ctx.alloc(&coin_id)?;
            vault_conditions.push(SendMessage::new(mode, message_bytes, vec![coin_id_node]));
        }

        let mut coin_spends = HashMap::new();

        for (asset, kind) in unspent {
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

                    let p2_puzzle = self
                        .p2_puzzles
                        .get(&asset.p2_puzzle_hash().into())
                        .expect("unknown p2 puzzle");

                    let spend = match p2_puzzle {
                        TestP2Puzzle::P2Singleton(p2_singleton) => p2_singleton.spend(
                            ctx,
                            self.info.custody_hash.into(),
                            1,
                            delegated_spend,
                        )?,
                        TestP2Puzzle::P2ConditionsOrSingleton(p2_conditions_or_singleton) => {
                            p2_conditions_or_singleton.p2_singleton_spend(
                                ctx,
                                self.info.custody_hash.into(),
                                1,
                                delegated_spend,
                            )?
                        }
                    };

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

/// For each CAT ring with a `RunCatTail(EverythingWithSingleton)` matching the given vault, return
/// the coin id of the TAIL-running coin and the bytes the vault must include as the body of its
/// `SendMessage` so that the TAIL's `RECEIVE_MESSAGE` is satisfied.
fn compute_singleton_tail_messages(
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    unspent: &[(SpendableAsset, SpendKind)],
) -> Result<Vec<(Bytes32, chia_protocol::Bytes)>> {
    let expected_struct_hash: Bytes32 = SingletonStruct::new(launcher_id).tree_hash().into();

    // Group CAT spends by asset id so we can compute a single ring delta per asset id.
    let mut rings: HashMap<Bytes32, Vec<(Bytes32, &Conditions)>> = HashMap::new();

    for (asset, kind) in unspent {
        let SpendableAsset::Cat(cat) = asset else {
            continue;
        };
        let SpendKind::Conditions(conditions) = kind else {
            continue;
        };
        rings
            .entry(cat.info.asset_id)
            .or_default()
            .push((cat.coin.coin_id(), conditions.conditions()));
    }

    let mut messages = Vec::new();

    for (_asset_id, items) in rings {
        let mut total_delta: i128 = 0;
        let mut tail_coin_and_program: Option<(Bytes32, NodePtr)> = None;
        let mut amounts_by_coin: HashMap<Bytes32, u64> = HashMap::new();
        let mut output_total_by_coin: HashMap<Bytes32, i128> = HashMap::new();

        for (coin_id, conditions) in &items {
            let mut output_total: i128 = 0;
            for condition in conditions.iter() {
                match condition {
                    Condition::CreateCoin(cc) => output_total += i128::from(cc.amount),
                    Condition::RunCatTail(rct) if tail_coin_and_program.is_none() => {
                        tail_coin_and_program = Some((*coin_id, rct.program));
                    }
                    _ => {}
                }
            }
            output_total_by_coin.insert(*coin_id, output_total);
        }

        for (coin_id, _) in &items {
            // Find the input amount for this coin id via the unspent items.
            for (asset, _) in unspent {
                if let SpendableAsset::Cat(cat) = asset
                    && cat.coin.coin_id() == *coin_id
                {
                    amounts_by_coin.insert(*coin_id, cat.coin.amount);
                    let output = output_total_by_coin.get(coin_id).copied().unwrap_or(0);
                    total_delta += i128::from(cat.coin.amount) - output;
                    break;
                }
            }
        }

        let Some((tail_coin_id, tail_program)) = tail_coin_and_program else {
            continue;
        };

        // Only emit a message if the TAIL is `EverythingWithSingleton` curried for this vault.
        let Some(curried) = CurriedPuzzle::parse(&*ctx, tail_program) else {
            continue;
        };
        if curried.mod_hash != EverythingWithSingletonTailArgs::mod_hash() {
            continue;
        }
        let args = ctx.extract::<EverythingWithSingletonTailArgs>(curried.args)?;
        if args.singleton_struct_hash != expected_struct_hash {
            continue;
        }

        let extra_delta: i128 = -total_delta;
        let extra_delta_node = ctx.alloc(&extra_delta)?;
        let extra_delta_bytes: Vec<u8> = ctx.atom(extra_delta_node).as_ref().to_vec();

        messages.push((tail_coin_id, extra_delta_bytes.into()));
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
