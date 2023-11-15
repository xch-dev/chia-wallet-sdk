use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::{
    cat::{CatArgs, CatSolution, CoinProof, CAT_PUZZLE_HASH},
    standard::{StandardArgs, StandardSolution},
    LineageProof,
};
use clvm_traits::{clvm_quote, ToClvmError, ToPtr};
use clvm_utils::{curry_tree_hash, tree_hash_atom, CurriedProgram};
use clvmr::{allocator::NodePtr, serde::node_to_bytes, Allocator};

use crate::{CatCondition, Condition, CreateCoin};

pub struct CatSpend {
    pub coin: Coin,
    pub synthetic_key: PublicKey,
    pub conditions: Vec<CatCondition<NodePtr>>,
    pub extra_delta: i64,
    pub p2_puzzle_hash: [u8; 32],
    pub lineage_proof: LineageProof,
}

pub fn spend_cat_coins(
    a: &mut Allocator,
    standard_puzzle: NodePtr,
    cat_puzzle: NodePtr,
    asset_id: &[u8; 32],
    cat_spends: &[CatSpend],
) -> Result<Vec<CoinSpend>, ToClvmError> {
    let mut total_delta = 0;

    cat_spends
        .iter()
        .enumerate()
        .map(|(index, cat_spend)| {
            // Calculate the delta and add it to the subtotal.
            let delta = cat_spend.conditions.iter().fold(
                cat_spend.coin.amount as i64 - cat_spend.extra_delta,
                |delta, condition| {
                    if let CatCondition::Normal(Condition::CreateCoin(
                        CreateCoin::Normal { amount, .. } | CreateCoin::Memos { amount, .. },
                    )) = condition
                    {
                        return delta - *amount as i64;
                    }
                    delta
                },
            );

            let prev_subtotal = total_delta;

            total_delta += delta;

            // Find information of neighboring coins on the ring.
            let prev_cat = &cat_spends[index.wrapping_sub(1) % cat_spends.len()];
            let next_cat = &cat_spends[index.wrapping_add(1) % cat_spends.len()];

            // Construct the puzzle.
            let puzzle = CurriedProgram {
                program: cat_puzzle,
                args: CatArgs {
                    mod_hash: CAT_PUZZLE_HASH.into(),
                    tail_program_hash: (*asset_id).into(),
                    inner_puzzle: CurriedProgram {
                        program: standard_puzzle,
                        args: StandardArgs {
                            synthetic_key: cat_spend.synthetic_key.clone(),
                        },
                    },
                },
            }
            .to_ptr(a)?;

            // Construct the solution.
            let solution = CatSolution {
                inner_puzzle_solution: StandardSolution {
                    original_public_key: None,
                    delegated_puzzle: clvm_quote!(&cat_spend.conditions),
                    solution: (),
                },
                lineage_proof: Some(cat_spend.lineage_proof.clone()),
                prev_coin_id: prev_cat.coin.coin_id().into(),
                this_coin_info: cat_spend.coin.clone(),
                next_coin_proof: CoinProof {
                    parent_coin_info: next_cat.coin.parent_coin_info,
                    inner_puzzle_hash: next_cat.p2_puzzle_hash.into(),
                    amount: next_cat.coin.amount,
                },
                prev_subtotal,
                extra_delta: cat_spend.extra_delta,
            }
            .to_ptr(a)?;

            let puzzle_bytes = node_to_bytes(a, puzzle).unwrap();
            let solution_bytes = node_to_bytes(a, solution).unwrap();

            // Create the coin spend.
            Ok(CoinSpend::new(
                cat_spend.coin.clone(),
                Program::new(puzzle_bytes.into()),
                Program::new(solution_bytes.into()),
            ))
        })
        .collect()
}

pub fn cat_puzzle_hash(asset_id: [u8; 32], inner_puzzle_hash: [u8; 32]) -> [u8; 32] {
    let mod_hash = tree_hash_atom(&CAT_PUZZLE_HASH);
    let asset_id_hash = tree_hash_atom(&asset_id);
    curry_tree_hash(
        CAT_PUZZLE_HASH,
        &[mod_hash, asset_id_hash, inner_puzzle_hash],
    )
}
