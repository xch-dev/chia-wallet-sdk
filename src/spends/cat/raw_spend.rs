use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::{
    cat::{CatArgs, CatSolution, CoinProof, CAT_PUZZLE_HASH},
    standard::{StandardArgs, StandardSolution},
    LineageProof,
};
use clvm_traits::{clvm_quote, FromNodePtr, ToClvmError, ToNodePtr};
use clvm_utils::CurriedProgram;
use clvmr::{Allocator, NodePtr};

use crate::{CatCondition, Condition, CreateCoin};

/// The information required to spend a CAT coin.
/// This assumes that the inner puzzle is a standard transaction.
pub struct CatSpend {
    /// The CAT coin that is being spent.
    pub coin: Coin,
    /// The public key used for the inner puzzle.
    pub synthetic_key: PublicKey,
    /// The desired output conditions for the coin spend.
    pub conditions: Vec<CatCondition<NodePtr>>,
    /// The extra delta produced as part of this spend.
    pub extra_delta: i64,
    /// The inner puzzle hash.
    pub p2_puzzle_hash: [u8; 32],
    /// The lineage proof of the CAT.
    pub lineage_proof: LineageProof,
}

/// Creates a set of CAT coin spends for a given asset id.
pub fn spend_cat_coins(
    a: &mut Allocator,
    standard_puzzle_ptr: NodePtr,
    cat_puzzle_ptr: NodePtr,
    asset_id: [u8; 32],
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
                program: cat_puzzle_ptr,
                args: CatArgs {
                    mod_hash: CAT_PUZZLE_HASH.into(),
                    tail_program_hash: asset_id.into(),
                    inner_puzzle: CurriedProgram {
                        program: standard_puzzle_ptr,
                        args: StandardArgs {
                            synthetic_key: cat_spend.synthetic_key.clone(),
                        },
                    },
                },
            }
            .to_node_ptr(a)?;

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
            .to_node_ptr(a)?;

            // Create the coin spend.
            Ok(CoinSpend::new(
                cat_spend.coin.clone(),
                Program::from_node_ptr(a, puzzle).unwrap(),
                Program::from_node_ptr(a, solution).unwrap(),
            ))
        })
        .collect()
}
