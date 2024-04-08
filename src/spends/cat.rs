use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_wallet::{
    cat::{CatArgs, CatSolution, CoinProof, EverythingWithSignatureTailArgs, CAT_PUZZLE_HASH},
    standard::{StandardArgs, StandardSolution},
    LineageProof,
};
use clvm_traits::{clvm_quote, destructure_tuple, match_tuple, MatchByte, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{RunTail, SpendContext, SpendError};

/// The information required to spend a CAT coin.
/// This assumes that the inner puzzle is a standard transaction.
pub struct CatSpend {
    /// The CAT coin that is being spent.
    pub coin: Coin,
    /// The public key used for the inner puzzle.
    pub synthetic_key: PublicKey,
    /// The desired output conditions for the coin spend.
    pub conditions: NodePtr,
    /// The extra delta produced as part of this spend.
    pub extra_delta: i64,
    /// The inner puzzle hash.
    pub p2_puzzle_hash: Bytes32,
    /// The lineage proof of the CAT.
    pub lineage_proof: LineageProof,
}

/// Creates a set of CAT coin spends for a given asset id.
pub fn spend_cat_coins(
    ctx: &mut SpendContext,
    asset_id: Bytes32,
    cat_spends: &[CatSpend],
) -> Result<Vec<CoinSpend>, SpendError> {
    let cat_puzzle_ptr = ctx.cat_puzzle();
    let standard_puzzle_ptr = ctx.standard_puzzle();

    let mut coin_spends = Vec::new();
    let mut total_delta = 0;
    let len = cat_spends.len();

    for (index, cat_spend) in cat_spends.iter().enumerate() {
        // Calculate the delta and add it to the subtotal.
        let conditions: Vec<NodePtr> = ctx.extract(cat_spend.conditions)?;
        let create_coins = conditions.into_iter().filter_map(|ptr| {
            ctx.extract::<match_tuple!(MatchByte<51>, NodePtr, u64, NodePtr)>(ptr)
                .ok()
        });
        let delta = create_coins.fold(
            cat_spend.coin.amount as i64 - cat_spend.extra_delta,
            |delta, destructure_tuple!(_, _, amount, _)| delta - amount as i64,
        );

        let prev_subtotal = total_delta;
        total_delta += delta;

        // Find information of neighboring coins on the ring.
        let prev_cat = &cat_spends[if index == 0 { len - 1 } else { index - 1 }];
        let next_cat = &cat_spends[if index == len - 1 { 0 } else { index + 1 }];

        let puzzle_reveal = ctx.serialize(CurriedProgram {
            program: cat_puzzle_ptr,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                tail_program_hash: asset_id,
                inner_puzzle: CurriedProgram {
                    program: standard_puzzle_ptr,
                    args: StandardArgs {
                        synthetic_key: cat_spend.synthetic_key.clone(),
                    },
                },
            },
        })?;

        let solution = ctx.serialize(CatSolution {
            inner_puzzle_solution: StandardSolution {
                original_public_key: None,
                delegated_puzzle: clvm_quote!(&cat_spend.conditions),
                solution: (),
            },
            lineage_proof: Some(cat_spend.lineage_proof.clone()),
            prev_coin_id: prev_cat.coin.coin_id(),
            this_coin_info: cat_spend.coin.clone(),
            next_coin_proof: CoinProof {
                parent_coin_info: next_cat.coin.parent_coin_info,
                inner_puzzle_hash: next_cat.p2_puzzle_hash,
                amount: next_cat.coin.amount,
            },
            prev_subtotal,
            extra_delta: cat_spend.extra_delta,
        })?;

        coin_spends.push(CoinSpend::new(
            cat_spend.coin.clone(),
            puzzle_reveal,
            solution,
        ));
    }

    Ok(coin_spends)
}

/// The information required to create and spend an eve CAT coin.
pub struct EveSpend {
    /// The full puzzle hash of the eve CAT coin.
    pub puzzle_hash: Bytes32,
    /// The coin spend for the eve CAT.
    pub coin_spend: CoinSpend,
}

/// Constructs a coin spend to issue more of an `EverythingWithSignature` CAT.
pub fn issue_cat_everything_with_signature<T>(
    ctx: &mut SpendContext,
    public_key: PublicKey,
    parent_coin_id: Bytes32,
    amount: u64,
    conditions: T,
) -> Result<EveSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    let tail_puzzle_ptr = ctx.everything_with_signature_tail_puzzle();

    let tail = ctx.alloc(CurriedProgram {
        program: tail_puzzle_ptr,
        args: EverythingWithSignatureTailArgs { public_key },
    })?;
    let asset_id = ctx.tree_hash(tail);

    let run_tail = RunTail {
        program: tail,
        solution: NodePtr::NIL,
    };

    let conditions = (run_tail, conditions);

    create_and_spend_eve_cat(ctx, parent_coin_id, asset_id, amount, conditions)
}

/// Creates an eve CAT coin and spends it.
pub fn create_and_spend_eve_cat<T>(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
    asset_id: Bytes32,
    amount: u64,
    conditions: T,
) -> Result<EveSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    let cat_puzzle_ptr = ctx.cat_puzzle();

    let inner_puzzle = ctx.alloc(clvm_quote!(conditions))?;
    let inner_puzzle_hash = ctx.tree_hash(inner_puzzle);

    let puzzle = ctx.alloc(CurriedProgram {
        program: cat_puzzle_ptr,
        args: CatArgs {
            mod_hash: CAT_PUZZLE_HASH.into(),
            tail_program_hash: asset_id,
            inner_puzzle,
        },
    })?;

    let puzzle_hash = ctx.tree_hash(puzzle);
    let coin = Coin::new(parent_coin_id, puzzle_hash, amount);

    let solution = ctx.serialize(CatSolution {
        inner_puzzle_solution: (),
        lineage_proof: None,
        prev_coin_id: coin.coin_id(),
        this_coin_info: coin.clone(),
        next_coin_proof: CoinProof {
            parent_coin_info: parent_coin_id,
            inner_puzzle_hash,
            amount,
        },
        prev_subtotal: 0,
        extra_delta: 0,
    })?;

    let puzzle_reveal = ctx.serialize(puzzle)?;
    let coin_spend = CoinSpend::new(coin, puzzle_reveal, solution);

    Ok(EveSpend {
        puzzle_hash,
        coin_spend,
    })
}

#[cfg(test)]
mod tests {
    use chia_bls::derive_keys::master_to_wallet_unhardened;
    use chia_consensus::gen::{
        conditions::EmptyVisitor, run_block_generator::run_block_generator,
        solution_generator::solution_generator,
    };
    use chia_protocol::{Bytes32, Program};
    use chia_wallet::{
        cat::cat_puzzle_hash,
        standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
        DeriveSynthetic,
    };
    use clvmr::{serde::node_to_bytes, Allocator};
    use hex_literal::hex;

    use crate::{testing::SECRET_KEY, CreateCoinWithoutMemos};

    use super::*;

    #[test]
    fn test_cat_spend() {
        let synthetic_key = master_to_wallet_unhardened(&SECRET_KEY.public_key(), 0)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let asset_id = Bytes32::new([42; 32]);

        let p2_puzzle_hash = Bytes32::new(standard_puzzle_hash(&synthetic_key));
        let cat_puzzle_hash = cat_puzzle_hash(asset_id.to_bytes(), p2_puzzle_hash.to_bytes());

        let parent_coin = Coin::new(Bytes32::new([0; 32]), Bytes32::new(cat_puzzle_hash), 69);
        let coin = Coin::new(
            Bytes32::from(parent_coin.coin_id()),
            Bytes32::new(cat_puzzle_hash),
            42,
        );

        let conditions = ctx
            .alloc([CreateCoinWithoutMemos {
                puzzle_hash: coin.puzzle_hash,
                amount: coin.amount,
            }])
            .unwrap();

        let coin_spend = spend_cat_coins(
            &mut ctx,
            asset_id,
            &[CatSpend {
                coin,
                synthetic_key,
                conditions,
                extra_delta: 0,
                lineage_proof: LineageProof {
                    parent_coin_info: parent_coin.parent_coin_info,
                    inner_puzzle_hash: p2_puzzle_hash,
                    amount: parent_coin.amount,
                },
                p2_puzzle_hash,
            }],
        )
        .unwrap()
        .remove(0);

        let output_ptr = coin_spend
            .puzzle_reveal
            .run(&mut allocator, 0, u64::MAX, &coin_spend.solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(&allocator, output_ptr).unwrap();

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

    #[test]
    fn test_cat_spend_multi() {
        let synthetic_key = master_to_wallet_unhardened(&SECRET_KEY.public_key(), 0)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let asset_id = Bytes32::new([42; 32]);

        let p2_puzzle_hash = Bytes32::new(standard_puzzle_hash(&synthetic_key));
        let cat_puzzle_hash = cat_puzzle_hash(asset_id.to_bytes(), p2_puzzle_hash.to_bytes());

        let parent_coin_1 = Coin::new(Bytes32::new([0; 32]), Bytes32::new(cat_puzzle_hash), 69);
        let coin_1 = Coin::new(
            Bytes32::from(parent_coin_1.coin_id()),
            Bytes32::new(cat_puzzle_hash),
            42,
        );

        let parent_coin_2 = Coin::new(Bytes32::new([0; 32]), Bytes32::new(cat_puzzle_hash), 69);
        let coin_2 = Coin::new(
            Bytes32::from(parent_coin_2.coin_id()),
            Bytes32::new(cat_puzzle_hash),
            34,
        );

        let parent_coin_3 = Coin::new(Bytes32::new([0; 32]), Bytes32::new(cat_puzzle_hash), 69);
        let coin_3 = Coin::new(
            Bytes32::from(parent_coin_3.coin_id()),
            Bytes32::new(cat_puzzle_hash),
            69,
        );

        let conditions = ctx
            .alloc([CreateCoinWithoutMemos {
                puzzle_hash: coin_1.puzzle_hash,
                amount: coin_1.amount + coin_2.amount + coin_3.amount,
            }])
            .unwrap();

        let coin_spends = spend_cat_coins(
            &mut ctx,
            asset_id,
            &[
                CatSpend {
                    coin: coin_1,
                    synthetic_key: synthetic_key.clone(),
                    conditions,
                    extra_delta: 0,
                    lineage_proof: LineageProof {
                        parent_coin_info: parent_coin_1.parent_coin_info,
                        inner_puzzle_hash: p2_puzzle_hash,
                        amount: parent_coin_1.amount,
                    },
                    p2_puzzle_hash,
                },
                CatSpend {
                    coin: coin_2,
                    synthetic_key: synthetic_key.clone(),
                    conditions: NodePtr::NIL,
                    extra_delta: 0,
                    lineage_proof: LineageProof {
                        parent_coin_info: parent_coin_2.parent_coin_info,
                        inner_puzzle_hash: p2_puzzle_hash,
                        amount: parent_coin_2.amount,
                    },
                    p2_puzzle_hash,
                },
                CatSpend {
                    coin: coin_3,
                    synthetic_key,
                    conditions: NodePtr::NIL,
                    extra_delta: 0,
                    lineage_proof: LineageProof {
                        parent_coin_info: parent_coin_3.parent_coin_info,
                        inner_puzzle_hash: p2_puzzle_hash,
                        amount: parent_coin_3.amount,
                    },
                    p2_puzzle_hash,
                },
            ],
        )
        .unwrap();

        let spend_vec = coin_spends
            .clone()
            .into_iter()
            .map(|coin_spend| {
                (
                    coin_spend.coin,
                    coin_spend.puzzle_reveal,
                    coin_spend.solution,
                )
            })
            .collect::<Vec<_>>();
        let gen = solution_generator(spend_vec).unwrap();
        let block =
            run_block_generator::<Program, EmptyVisitor>(&mut allocator, &gen, &[], u64::MAX, 0)
                .unwrap();

        assert_eq!(block.cost, 101289468);

        assert_eq!(coin_spends.len(), 3);

        let output_ptr_1 = coin_spends[0]
            .puzzle_reveal
            .run(&mut allocator, 0, u64::MAX, &coin_spends[0].solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(&allocator, output_ptr_1).unwrap();

        let expected = hex!(
            "
            ffff46ffa06438c882c2db9f5c2a8b4cbda9258c40a6583b2d7c6becc1678607
            4d558c834980ffff3cffa1cb1cb6597fe61e67a6cbbcd4e8f0bda5e9fc56cd84
            c9e9502772b410dc8a03207680ffff3dffa0742ddb368882193072ea013bde24
            4a5c9d40ab4454c09666e84777a79307e17a80ffff32ffb08584adae5630842a
            1766bc444d2b872dd3080f4e5daaecf6f762a4be7dc148f37868149d4217f3dc
            c9183fe61e48d8bfffa004c476adfcffeacfef7c979bdd03b4641f1870d3f81b
            20636eefbcf879bb64ec80ffff33ffa0f9f2d59294f2aae8f9833db876d1bf43
            95d46af18c17312041c6f4a4d73fa041ff8200918080
            "
        );
        assert_eq!(hex::encode(actual), hex::encode(expected));

        let output_ptr_2 = coin_spends[1]
            .puzzle_reveal
            .run(&mut allocator, 0, u64::MAX, &coin_spends[1].solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(&allocator, output_ptr_2).unwrap();

        let expected = hex!(
            "
            ffff46ffa0ae60b8db0664959078a1c6e51ca6a8fc55207c63a8ac74d026f1d9
            15c406bac480ffff3cffa1cb9a41843ab318a8336f61a6bf9e8b0b1d555b9f07
            cd19582e0bc52a961c65dc9e80ffff3dffa0294cda8d35164e01c4e3b7c07c36
            a5bb2f38a23e93ef49c882ee74349a0df8bd80ffff32ffb08584adae5630842a
            1766bc444d2b872dd3080f4e5daaecf6f762a4be7dc148f37868149d4217f3dc
            c9183fe61e48d8bfffa0ba4484b961b7a2369d948d06c55b64bdbfaffb326bc1
            3b490ab1215dd33d8d468080
            "
        );
        assert_eq!(hex::encode(actual), hex::encode(expected));

        let output_ptr_3 = coin_spends[2]
            .puzzle_reveal
            .run(&mut allocator, 0, u64::MAX, &coin_spends[2].solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(&allocator, output_ptr_3).unwrap();

        let expected = hex!(
            "
            ffff46ffa0f8eacbef2bad0c7b27b638a90a37244e75013e977f250230856d05
            a2784e1d0980ffff3cffa1cb17c47c5fa8d795efa0d9227d2066cde36dd4e845
            7e8f4e507d2015a1c7f3d94b80ffff3dffa0629abc502829339c7880ee003c4e
            68a8181d71206e50e7b36c29301ef60128f580ffff32ffb08584adae5630842a
            1766bc444d2b872dd3080f4e5daaecf6f762a4be7dc148f37868149d4217f3dc
            c9183fe61e48d8bfffa0ba4484b961b7a2369d948d06c55b64bdbfaffb326bc1
            3b490ab1215dd33d8d468080
            "
        );
        assert_eq!(hex::encode(actual), hex::encode(expected));
    }
}
