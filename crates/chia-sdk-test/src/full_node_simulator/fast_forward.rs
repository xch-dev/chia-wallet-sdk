use super::*;

impl FullNodeSimulator {
    pub(super) fn try_fast_forward_bundle(
        &self,
        validated: &ValidatedBundle,
    ) -> Option<SpendBundle> {
        for (coin_id, spend) in &validated.spends {
            if (spend.flags & ELIGIBLE_FOR_FF) == 0 {
                continue;
            }

            for mempool_item in self.mempool.values() {
                if !mempool_item.removals.contains(coin_id) {
                    continue;
                }

                let Some(conflicting_spend) = mempool_item.spends.get(coin_id) else {
                    continue;
                };

                let Some((new_coin, _)) = conflicting_spend.additions.iter().find(|(coin, _)| {
                    coin.parent_coin_info == *coin_id
                        && coin.puzzle_hash == spend.coin_spend.coin.puzzle_hash
                        && coin.amount == spend.coin_spend.coin.amount
                        && (coin.amount & 1) == 1
                }) else {
                    continue;
                };

                let Some(new_coin_spend) = Self::fast_forward_coin_spend(
                    &spend.coin_spend,
                    *new_coin,
                    conflicting_spend.coin_spend.coin,
                ) else {
                    continue;
                };

                let mut coin_spends = validated.spend_bundle.coin_spends.clone();
                let Some(existing_spend) = coin_spends
                    .iter_mut()
                    .find(|existing| existing.coin.coin_id() == *coin_id)
                else {
                    continue;
                };

                *existing_spend = new_coin_spend;
                return Some(SpendBundle::new(
                    coin_spends,
                    validated.spend_bundle.aggregated_signature.clone(),
                ));
            }
        }

        None
    }

    pub(super) fn try_fast_forward_settled_bundle(
        &self,
        spend_bundle: &SpendBundle,
    ) -> Option<SpendBundle> {
        let constants = default_constants(SIMULATOR_GENESIS_CHALLENGE, SIMULATOR_GENESIS_CHALLENGE);
        let conds = validate_clvm_and_signature(
            spend_bundle,
            11_000_000_000 / 2,
            &constants,
            ENABLE_KECCAK_OPS_OUTSIDE_GUARD | COMPUTE_FINGERPRINT,
        )
        .ok()?;

        let mut coin_spends = spend_bundle.coin_spends.clone();
        let mut rewrote_any = false;

        for spend in &conds.spends {
            let Some(record) = self.coins.get(&spend.coin_id) else {
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
                let Some(current_record) = self.coins.get(&rewritten.coin.coin_id()) else {
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
                let Some(next_record) = self.coins.get(&next_coin.coin_id()) else {
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

        if !rewrote_any {
            return None;
        }

        Some(SpendBundle::new(
            coin_spends,
            spend_bundle.aggregated_signature.clone(),
        ))
    }

    pub(super) fn fast_forward_coin_spend(
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
    use chia_protocol::{Coin, CoinSpend, SpendBundle};
    use chia_puzzle_types::{
        LineageProof, Proof,
        singleton::{SingletonArgs, SingletonSolution},
    };
    use chia_sdk_types::Mod;
    use chia_sdk_types::conditions::{CreateCoin, Memos};
    use clvm_traits::ToClvm;
    use clvm_utils::CurriedProgram;
    use clvmr::{Allocator, NodePtr, serde::node_from_bytes, serde::node_to_bytes};

    use crate::to_puzzle;

    use super::*;

    fn singleton_spend_to_child(
        coin: Coin,
        launcher_id: Bytes32,
        inner_puzzle_reveal: chia_protocol::Program,
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
        let inner_puzzle = node_from_bytes(&mut allocator, inner_puzzle_reveal.as_slice())?;
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

    #[test]
    fn try_fast_forward_rewrites_singleton_spend_against_mempool_item() -> anyhow::Result<()> {
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
            inner_puzzle_reveal.clone(),
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            None,
        )?;
        let fast_forward_hint: Bytes32 = [8; 32].into();
        let second_singleton_spend = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            inner_puzzle_reveal.clone(),
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
        let rewritten = sim
            .try_fast_forward_bundle(&ValidatedBundle {
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
            .expect("singleton spend should be fast-forwarded");
        assert!(
            rewritten
                .coin_spends
                .iter()
                .any(|spend| spend.coin.coin_id() == child_coin.coin_id())
        );

        Ok(())
    }

    #[test]
    fn push_tx_fast_forwards_already_settled_singleton_spend() -> anyhow::Result<()> {
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
        let child_coin = Coin::new(
            singleton_coin.coin_id(),
            singleton_coin.puzzle_hash,
            singleton_coin.amount,
        );
        sim.insert_coin(child_coin);
        sim.coins
            .get_mut(&singleton_coin.coin_id())
            .unwrap()
            .spent_block_index = Some(2);

        let stale_singleton_spend = singleton_spend_to_child(
            singleton_coin,
            launcher_id,
            inner_puzzle_reveal.clone(),
            lineage_proof,
            singleton_puzzle_hash,
            singleton_coin.amount,
            Some([14; 32].into()),
        )?;
        let stale_bundle = SpendBundle::new(vec![stale_singleton_spend], Signature::default());
        assert!(
            FullNodeSimulator::fast_forward_coin_spend(
                &stale_bundle.coin_spends[0],
                child_coin,
                singleton_coin,
            )
            .is_some()
        );
        let maybe_rewritten = sim.try_fast_forward_settled_bundle(&stale_bundle);
        assert!(maybe_rewritten.is_some());
        let rewritten = maybe_rewritten.unwrap();
        assert_eq!(
            rewritten.coin_spends[0].coin.coin_id(),
            child_coin.coin_id()
        );

        let response = sim.push_tx(stale_bundle);
        assert!(response.success, "{response:?}");

        let child_record = sim
            .get_coin_record_by_name(child_coin.coin_id())
            .coin_record
            .unwrap();
        assert!(child_record.spent);

        let last_spends = sim
            .get_block_spends(sim.header_hash())
            .block_spends
            .unwrap();
        assert_eq!(last_spends.len(), 1);
        assert_eq!(last_spends[0].coin.coin_id(), child_coin.coin_id());

        Ok(())
    }
}
