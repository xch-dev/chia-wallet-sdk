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

#[cfg(test)]
mod tests {
    use chia_bls::{derive_keys::master_to_wallet_unhardened, SecretKey};
    use chia_protocol::Bytes32;
    use chia_wallet::{
        cat::{cat_puzzle_hash, CAT_PUZZLE},
        standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH, STANDARD_PUZZLE},
        DeriveSynthetic,
    };
    use clvmr::serde::{node_from_bytes, node_to_bytes};
    use hex_literal::hex;

    use crate::testing::SEED;

    use super::*;

    #[test]
    fn test_cat_spend() {
        let synthetic_key =
            master_to_wallet_unhardened(&SecretKey::from_seed(SEED.as_ref()).public_key(), 0)
                .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);

        let a = &mut Allocator::new();
        let standard_puzzle_ptr = node_from_bytes(a, &STANDARD_PUZZLE).unwrap();
        let cat_puzzle_ptr = node_from_bytes(a, &CAT_PUZZLE).unwrap();

        let asset_id = [42; 32];

        let p2_puzzle_hash = standard_puzzle_hash(&synthetic_key);
        let cat_puzzle_hash = cat_puzzle_hash(asset_id, p2_puzzle_hash);

        let parent_coin = Coin::new(Bytes32::new([0; 32]), Bytes32::new(cat_puzzle_hash), 69);
        let coin = Coin::new(
            Bytes32::from(parent_coin.coin_id()),
            Bytes32::new(cat_puzzle_hash),
            42,
        );

        let conditions = vec![CatCondition::Normal(Condition::CreateCoin(
            CreateCoin::Normal {
                puzzle_hash: coin.puzzle_hash,
                amount: coin.amount,
            },
        ))];

        let coin_spend = spend_cat_coins(
            a,
            standard_puzzle_ptr,
            cat_puzzle_ptr,
            asset_id,
            &[CatSpend {
                coin,
                synthetic_key,
                conditions,
                extra_delta: 0,
                lineage_proof: LineageProof {
                    parent_coin_info: parent_coin.parent_coin_info,
                    inner_puzzle_hash: p2_puzzle_hash.into(),
                    amount: parent_coin.amount,
                },
                p2_puzzle_hash,
            }],
        )
        .unwrap()
        .remove(0);

        let output_ptr = coin_spend
            .puzzle_reveal
            .run(a, 0, u64::MAX, &coin_spend.solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(a, output_ptr).unwrap();

        let expected = hex!(
            "
            ffff46ffa06438c882c2db9f5c2a8b4cbda9258c40a6583b2d7c6becc1678607
            4d558c834980ffff3cffa1cb9c4d253a0e1a091d620a55616e104f3329f58ee8
            6e708d0527b1cc58a73b649e80ffff3dffa0c3bb7f0a7e1bd2cae332bbd0d1a7
            e275c1e6c643b2659e22c24f513886d3874e80ffff32ffb08584adae5630842a
            1766bc444d2b872dd3080f4e5daaecf6f762a4be7dc148f37868149d4217f3dc
            c9183fe61e48d8bfffa0e5924c23faf33c9a1bf18c70d40cb09e4b194f521b9f
            6fceb2685c0612ac34a980ffff33ffa0f9f2d59294f2aae8f9833db876d1bf43
            95d46af18c17312041c6f4a4d73fa041ff2a8080
            "
        );
        assert_eq!(hex::encode(actual), hex::encode(expected));
    }
}
