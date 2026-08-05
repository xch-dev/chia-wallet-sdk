use chia_bls::Signature;
use chia_consensus::{
    conditions::ELIGIBLE_FOR_FF, fast_forward::fast_forward_singleton, flags::COMPUTE_FINGERPRINT,
};
use chia_protocol::{Bytes32, Coin, CoinSpend, SpendBundle};
use chia_sdk_types::default_constants;
use clvmr::{
    Allocator, ENABLE_KECCAK_OPS_OUTSIDE_GUARD,
    serde::{node_from_bytes, node_to_bytes},
};

use crate::{
    FullNodeSimulator,
    full_node_simulator::{SIMULATOR_GENESIS_CHALLENGE, ValidatedBundle, ValidatedSpend},
    validate_clvm_and_signature,
};

#[derive(Debug)]
pub(super) enum FastForwardResult {
    Rewritten(Box<SpendBundle>),
    NoProgress,
}

impl FullNodeSimulator {
    pub(super) fn fast_forward_mempool_spends(
        &self,
        validated: &ValidatedBundle,
    ) -> FastForwardResult {
        let mut coin_spends = validated.spend_bundle.coin_spends.clone();
        let mut rewrote_any = false;

        for (coin_id, spend) in &validated.spends {
            if (spend.flags & ELIGIBLE_FOR_FF) == 0 {
                continue;
            }

            let Some(new_coin_spend) = self.fast_forward_mempool_spend(*coin_id, spend) else {
                continue;
            };
            let Some(existing_spend) = coin_spends
                .iter_mut()
                .find(|existing| existing.coin.coin_id() == *coin_id)
            else {
                continue;
            };

            *existing_spend = new_coin_spend;
            rewrote_any = true;
        }

        Self::rewrite_result(
            rewrote_any,
            coin_spends,
            &validated.spend_bundle.aggregated_signature,
        )
    }

    fn fast_forward_mempool_spend(
        &self,
        coin_id: Bytes32,
        spend: &ValidatedSpend,
    ) -> Option<CoinSpend> {
        for mempool_item in self.mempool.values() {
            if !mempool_item.removals.contains(&coin_id) {
                continue;
            }

            let Some(conflicting_spend) = mempool_item.spends.get(&coin_id) else {
                continue;
            };
            let Some((new_coin, _)) = conflicting_spend.additions.iter().find(|(coin, _)| {
                coin.parent_coin_info == coin_id
                    && coin.puzzle_hash == spend.coin_spend.coin.puzzle_hash
                    && coin.amount == spend.coin_spend.coin.amount
                    && (coin.amount & 1) == 1
            }) else {
                continue;
            };

            let Some(rewritten) = Self::fast_forward_coin_spend(
                &spend.coin_spend,
                *new_coin,
                conflicting_spend.coin_spend.coin,
            ) else {
                continue;
            };
            return Some(rewritten);
        }

        None
    }

    pub(super) fn fast_forward_settled_spends(
        &self,
        spend_bundle: &SpendBundle,
    ) -> FastForwardResult {
        let constants = default_constants(SIMULATOR_GENESIS_CHALLENGE, SIMULATOR_GENESIS_CHALLENGE);
        let Ok(conds) = validate_clvm_and_signature(
            spend_bundle,
            11_000_000_000 / 2,
            &constants,
            ENABLE_KECCAK_OPS_OUTSIDE_GUARD | COMPUTE_FINGERPRINT,
        ) else {
            return FastForwardResult::NoProgress;
        };

        let mut coin_spends = spend_bundle.coin_spends.clone();
        let mut rewrote_any = false;

        for spend in &conds.spends {
            let Some(record) = self.state.coins.get(&spend.coin_id) else {
                continue;
            };
            if record.spent_block_index.is_none() {
                continue;
            }

            let Some(index) = coin_spends
                .iter()
                .position(|coin_spend| coin_spend.coin.coin_id() == spend.coin_id)
            else {
                continue;
            };

            let mut rewritten = coin_spends[index].clone();
            loop {
                if (rewritten.coin.amount & 1) == 0 {
                    break;
                }
                let Some(current_record) = self.state.coins.get(&rewritten.coin.coin_id()) else {
                    break;
                };
                if current_record.spent_block_index.is_none() {
                    break;
                }
                let next_coin = Coin::new(
                    rewritten.coin.coin_id(),
                    rewritten.coin.puzzle_hash,
                    rewritten.coin.amount,
                );
                let Some(next_record) = self.state.coins.get(&next_coin.coin_id()) else {
                    break;
                };
                let Some(next_spend) =
                    Self::fast_forward_coin_spend(&rewritten, next_record.coin, rewritten.coin)
                else {
                    break;
                };
                rewritten = next_spend;
                rewrote_any = true;
            }

            coin_spends[index] = rewritten;
        }

        Self::rewrite_result(rewrote_any, coin_spends, &spend_bundle.aggregated_signature)
    }

    fn rewrite_result(
        rewrote_any: bool,
        coin_spends: Vec<CoinSpend>,
        signature: &Signature,
    ) -> FastForwardResult {
        if rewrote_any {
            FastForwardResult::Rewritten(Box::new(SpendBundle::new(coin_spends, signature.clone())))
        } else {
            FastForwardResult::NoProgress
        }
    }

    fn fast_forward_coin_spend(
        coin_spend: &CoinSpend,
        new_coin: Coin,
        new_parent: Coin,
    ) -> Option<CoinSpend> {
        let mut allocator = Allocator::new_limited(500_000_000);
        let puzzle = node_from_bytes(&mut allocator, coin_spend.puzzle_reveal.as_slice()).ok()?;
        let solution = node_from_bytes(&mut allocator, coin_spend.solution.as_slice()).ok()?;
        let new_solution = fast_forward_singleton(
            &mut allocator,
            puzzle,
            solution,
            &coin_spend.coin,
            &new_coin,
            &new_parent,
        )
        .ok()?;
        let new_solution_bytes = node_to_bytes(&allocator, new_solution).ok()?;
        Some(CoinSpend::new(
            new_coin,
            coin_spend.puzzle_reveal.clone(),
            new_solution_bytes.into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::Signature;
    use chia_protocol::{Bytes32, Coin, CoinSpend, Program, SpendBundle};
    use chia_puzzle_types::{
        LineageProof, Proof,
        singleton::{SingletonArgs, SingletonSolution},
    };
    use chia_sdk_types::Mod;
    use chia_sdk_types::conditions::{CreateCoin, Memos};
    use clvm_traits::ToClvm;
    use clvm_utils::CurriedProgram;
    use clvmr::{Allocator, NodePtr, serde::node_from_bytes, serde::node_to_bytes};
    use indexmap::{IndexMap, IndexSet};

    use crate::{full_node_simulator::ValidatedSpend, to_puzzle};

    use super::*;

    fn singleton_spend_to_child(
        coin: Coin,
        launcher_id: Bytes32,
        inner_puzzle_reveal: &Program,
        lineage_proof: LineageProof,
        child_puzzle_hash: Bytes32,
        child_amount: u64,
        hint: Option<Bytes32>,
    ) -> anyhow::Result<CoinSpend> {
        let mut allocator = Allocator::new_limited(500_000_000);
        let memos = if let Some(hint) = hint {
            let hint_atom = allocator.new_atom(hint.as_ref())?;
            let memo_list = allocator.new_pair(hint_atom, NodePtr::NIL)?;
            Memos::Some(memo_list)
        } else {
            Memos::None
        };
        let inner_solution = [CreateCoin::<NodePtr>::new(
            child_puzzle_hash,
            child_amount,
            memos,
        )]
        .to_clvm(&mut allocator)?;
        let singleton_mod = node_from_bytes(
            &mut allocator,
            SingletonArgs::<NodePtr>::mod_reveal().as_ref(),
        )?;
        let inner_puzzle = node_from_bytes(&mut allocator, inner_puzzle_reveal)?;
        let singleton_puzzle = CurriedProgram {
            program: singleton_mod,
            args: SingletonArgs::new(launcher_id, inner_puzzle),
        }
        .to_clvm(&mut allocator)?;
        let singleton_solution = SingletonSolution {
            lineage_proof: Proof::Lineage(lineage_proof),
            amount: coin.amount,
            inner_solution,
        }
        .to_clvm(&mut allocator)?;

        Ok(CoinSpend::new(
            coin,
            node_to_bytes(&allocator, singleton_puzzle)?.into(),
            node_to_bytes(&allocator, singleton_solution)?.into(),
        ))
    }

    fn singleton_conflict(
        sim: &mut FullNodeSimulator,
        seed: u8,
    ) -> anyhow::Result<(CoinSpend, CoinSpend, Coin)> {
        let (inner_puzzle_hash, inner_puzzle_reveal) = to_puzzle(u64::from(seed))?;
        let launcher_id: Bytes32 = [seed; 32].into();
        let singleton_puzzle_hash: Bytes32 =
            SingletonArgs::curry_tree_hash(launcher_id, inner_puzzle_hash.into()).into();
        let parent_coin = Coin::new(
            [seed.saturating_add(1); 32].into(),
            singleton_puzzle_hash,
            101,
        );
        let singleton_coin = Coin::new(parent_coin.coin_id(), singleton_puzzle_hash, 101);
        let lineage_proof = LineageProof {
            parent_parent_coin_info: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: inner_puzzle_hash,
            parent_amount: parent_coin.amount,
        };
        sim.insert_coin(singleton_coin);

        let first = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            &inner_puzzle_reveal,
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            None,
        )?;
        let candidate = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            &inner_puzzle_reveal,
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            Some([seed.saturating_add(2); 32].into()),
        )?;
        let child = Coin::new(
            singleton_coin.coin_id(),
            singleton_coin.puzzle_hash,
            singleton_coin.amount,
        );

        Ok((first, candidate, child))
    }

    #[test]
    fn fast_forward_rewrites_all_eligible_mempool_spends_in_one_pass() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (first_a, candidate_a, child_a) = singleton_conflict(&mut sim, 21)?;
        let (first_b, candidate_b, child_b) = singleton_conflict(&mut sim, 31)?;
        let left_coin_id = first_a.coin.coin_id();
        let right_coin_id = first_b.coin.coin_id();
        let first_bundle =
            SpendBundle::new(vec![first_a.clone(), first_b.clone()], Signature::default());
        sim.mempool.insert(
            first_bundle.name(),
            ValidatedBundle {
                spend_bundle: first_bundle,
                removals: vec![left_coin_id, right_coin_id],
                additions: vec![(child_a, None), (child_b, None)],
                spends: IndexMap::from([
                    (
                        left_coin_id,
                        ValidatedSpend {
                            coin_spend: first_a,
                            flags: ELIGIBLE_FOR_FF,
                            fingerprint: None,
                            additions: vec![(child_a, None)],
                        },
                    ),
                    (
                        right_coin_id,
                        ValidatedSpend {
                            coin_spend: first_b,
                            flags: ELIGIBLE_FOR_FF,
                            fingerprint: None,
                            additions: vec![(child_b, None)],
                        },
                    ),
                ]),
                cost: 0,
                fee: 0,
            },
        );

        let candidate_bundle = SpendBundle::new(
            vec![candidate_a.clone(), candidate_b.clone()],
            Signature::default(),
        );
        let FastForwardResult::Rewritten(rewritten) =
            sim.fast_forward_mempool_spends(&ValidatedBundle {
                spend_bundle: candidate_bundle,
                removals: vec![left_coin_id, right_coin_id],
                additions: Vec::new(),
                spends: IndexMap::from([
                    (
                        left_coin_id,
                        ValidatedSpend {
                            coin_spend: candidate_a,
                            flags: ELIGIBLE_FOR_FF,
                            fingerprint: None,
                            additions: Vec::new(),
                        },
                    ),
                    (
                        right_coin_id,
                        ValidatedSpend {
                            coin_spend: candidate_b,
                            flags: ELIGIBLE_FOR_FF,
                            fingerprint: None,
                            additions: Vec::new(),
                        },
                    ),
                ]),
                cost: 0,
                fee: 0,
            })
        else {
            panic!("both singleton spends should be fast-forwarded");
        };

        let rewritten_ids = rewritten
            .coin_spends
            .iter()
            .map(|spend| spend.coin.coin_id())
            .collect::<IndexSet<_>>();
        assert!(rewritten_ids.contains(&child_a.coin_id()));
        assert!(rewritten_ids.contains(&child_b.coin_id()));

        Ok(())
    }

    #[test]
    fn fast_forward_reports_no_progress_without_matching_lineage() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (_, candidate, _) = singleton_conflict(&mut sim, 41)?;
        let coin_id = candidate.coin.coin_id();
        let bundle = SpendBundle::new(vec![candidate.clone()], Signature::default());

        assert!(matches!(
            sim.fast_forward_mempool_spends(&ValidatedBundle {
                spend_bundle: bundle,
                removals: vec![coin_id],
                additions: Vec::new(),
                spends: IndexMap::from([(
                    coin_id,
                    ValidatedSpend {
                        coin_spend: candidate,
                        flags: ELIGIBLE_FOR_FF,
                        fingerprint: None,
                        additions: Vec::new(),
                    },
                )]),
                cost: 0,
                fee: 0,
            }),
            FastForwardResult::NoProgress
        ));

        Ok(())
    }

    #[test]
    fn fast_forward_rewrites_singleton_spend_against_mempool_item() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (inner_puzzle_hash, inner_puzzle_reveal) = to_puzzle(1)?;
        let launcher_id: Bytes32 = [7; 32].into();
        let singleton_puzzle_hash: Bytes32 =
            SingletonArgs::curry_tree_hash(launcher_id, inner_puzzle_hash.into()).into();
        let parent_coin = Coin::new([9; 32].into(), singleton_puzzle_hash, 101);
        let singleton_coin = Coin::new(parent_coin.coin_id(), singleton_puzzle_hash, 101);
        let lineage_proof = LineageProof {
            parent_parent_coin_info: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: inner_puzzle_hash,
            parent_amount: parent_coin.amount,
        };
        sim.insert_coin(singleton_coin);

        let first_singleton_spend = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            &inner_puzzle_reveal,
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            None,
        )?;
        let fast_forward_hint: Bytes32 = [8; 32].into();
        let second_singleton_spend = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            &inner_puzzle_reveal,
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            Some(fast_forward_hint),
        )?;
        let child_coin = Coin::new(
            singleton_coin.coin_id(),
            singleton_coin.puzzle_hash,
            singleton_coin.amount,
        );
        let first_tx = SpendBundle::new(vec![first_singleton_spend.clone()], Signature::default());
        sim.mempool.insert(
            first_tx.name(),
            ValidatedBundle {
                spend_bundle: first_tx,
                removals: vec![singleton_coin.coin_id()],
                additions: vec![(child_coin, None)],
                spends: IndexMap::from([(
                    singleton_coin.coin_id(),
                    ValidatedSpend {
                        coin_spend: first_singleton_spend,
                        flags: ELIGIBLE_FOR_FF,
                        fingerprint: None,
                        additions: vec![(child_coin, None)],
                    },
                )]),
                cost: 0,
                fee: 0,
            },
        );

        let candidate_bundle =
            SpendBundle::new(vec![second_singleton_spend.clone()], Signature::default());
        let FastForwardResult::Rewritten(rewritten) =
            sim.fast_forward_mempool_spends(&ValidatedBundle {
                spend_bundle: candidate_bundle.clone(),
                removals: vec![singleton_coin.coin_id()],
                additions: Vec::new(),
                spends: IndexMap::from([(
                    singleton_coin.coin_id(),
                    ValidatedSpend {
                        coin_spend: second_singleton_spend,
                        flags: ELIGIBLE_FOR_FF,
                        fingerprint: None,
                        additions: Vec::new(),
                    },
                )]),
                cost: 0,
                fee: 0,
            })
        else {
            panic!("singleton spend should be fast-forwarded");
        };
        assert!(
            rewritten
                .coin_spends
                .iter()
                .any(|spend| spend.coin.coin_id() == child_coin.coin_id())
        );

        Ok(())
    }

    #[test]
    fn push_tx_fast_forwards_deep_settled_singleton_lineage() -> anyhow::Result<()> {
        let mut sim = FullNodeSimulator::new();
        let (inner_puzzle_hash, inner_puzzle_reveal) = to_puzzle(1)?;
        let launcher_id: Bytes32 = [11; 32].into();
        let singleton_puzzle_hash: Bytes32 =
            SingletonArgs::curry_tree_hash(launcher_id, inner_puzzle_hash.into()).into();
        let parent_coin = Coin::new([13; 32].into(), singleton_puzzle_hash, 101);
        let singleton_coin = Coin::new(parent_coin.coin_id(), singleton_puzzle_hash, 101);
        let lineage_proof = LineageProof {
            parent_parent_coin_info: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: inner_puzzle_hash,
            parent_amount: parent_coin.amount,
        };
        sim.insert_coin(singleton_coin);
        let mut lineage_tip = singleton_coin;
        for height in 2..=98 {
            let child_coin = Coin::new(
                lineage_tip.coin_id(),
                lineage_tip.puzzle_hash,
                lineage_tip.amount,
            );
            sim.insert_coin(child_coin);
            sim.state
                .coins
                .get_mut(&lineage_tip.coin_id())
                .unwrap()
                .spent_block_index = Some(height);
            lineage_tip = child_coin;
        }

        let stale_singleton_spend = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            &inner_puzzle_reveal,
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            Some([14; 32].into()),
        )?;
        let stale_bundle = SpendBundle::new(vec![stale_singleton_spend], Signature::default());
        let FastForwardResult::Rewritten(rewritten) =
            sim.fast_forward_settled_spends(&stale_bundle)
        else {
            panic!("settled singleton spend should be fast-forwarded");
        };
        assert_eq!(
            rewritten.coin_spends[0].coin.coin_id(),
            lineage_tip.coin_id()
        );

        let response = sim.push_tx(stale_bundle);
        assert!(response.success, "{response:?}");
        sim.farm_block(1);

        let tip_record = sim
            .get_coin_record_by_name(lineage_tip.coin_id())
            .coin_record
            .unwrap();
        assert!(tip_record.spent);

        let last_spends = sim
            .get_block_spends(sim.header_hash())
            .block_spends
            .unwrap();
        assert_eq!(last_spends.len(), 1);
        assert_eq!(last_spends[0].coin.coin_id(), lineage_tip.coin_id());

        Ok(())
    }
}
