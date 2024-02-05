use chia_bls::PublicKey;
use chia_client::Peer;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_wallet::{
    cat::{CatArgs, CatSolution, CoinProof, EverythingWithSignatureTailArgs, CAT_PUZZLE_HASH},
    standard::{standard_puzzle_hash, StandardArgs, StandardSolution},
    LineageProof,
};
use clvm_traits::{clvm_quote, FromNodePtr, ToClvmError, ToNodePtr};
use clvm_utils::{curry_tree_hash, tree_hash, tree_hash_atom, CurriedProgram};
use clvmr::{allocator::NodePtr, Allocator};

use crate::{
    utils::request_puzzle_args, CatCondition, Condition, CreateCoin, DerivationStore, RunTail,
};

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

/// The information required to create and spend an eve CAT coin.
pub struct EveSpendInfo {
    /// The full puzzle hash of the eve CAT coin.
    pub puzzle_hash: [u8; 32],
    /// The coin spend for the eve CAT.
    pub coin_spend: CoinSpend,
}

/// Constructs a coin spend to issue more of an `EverythingWithSignature` CAT.
pub fn issue_cat_with_public_key(
    a: &mut Allocator,
    cat_puzzle_ptr: NodePtr,
    tail_puzzle_ptr: NodePtr,
    public_key: PublicKey,
    parent_coin_id: Bytes32,
    amount: u64,
    conditions: &[Condition<NodePtr>],
) -> Result<EveSpendInfo, ToClvmError> {
    let mut cat_conditions: Vec<CatCondition<NodePtr>> = Vec::with_capacity(conditions.len() + 1);
    cat_conditions.extend(
        conditions
            .iter()
            .map(|condition| CatCondition::Normal(condition.clone())),
    );

    let tail = CurriedProgram {
        program: tail_puzzle_ptr,
        args: EverythingWithSignatureTailArgs { public_key },
    }
    .to_node_ptr(a)?;

    cat_conditions.push(CatCondition::RunTail(RunTail {
        program: tail,
        solution: NodePtr::NIL,
    }));

    spend_new_eve_cat(
        a,
        cat_puzzle_ptr,
        parent_coin_id,
        tree_hash(a, tail),
        amount,
        &cat_conditions,
    )
}

/// Creates an eve CAT coin and spends it.
pub fn spend_new_eve_cat(
    a: &mut Allocator,
    cat_puzzle_ptr: NodePtr,
    parent_coin_id: Bytes32,
    tail_program_hash: [u8; 32],
    amount: u64,
    conditions: &[CatCondition<NodePtr>],
) -> Result<EveSpendInfo, ToClvmError> {
    let inner_puzzle = clvm_quote!(conditions).to_node_ptr(a)?;
    let inner_puzzle_hash = tree_hash(a, inner_puzzle);

    let puzzle = CurriedProgram {
        program: cat_puzzle_ptr,
        args: CatArgs {
            mod_hash: CAT_PUZZLE_HASH.into(),
            tail_program_hash: tail_program_hash.into(),
            inner_puzzle,
        },
    }
    .to_node_ptr(a)?;

    let puzzle_hash = tree_hash(a, puzzle);
    let coin = Coin::new(parent_coin_id, puzzle_hash.into(), amount);

    let solution = CatSolution {
        inner_puzzle_solution: (),
        lineage_proof: None,
        prev_coin_id: coin.coin_id().into(),
        this_coin_info: coin.clone(),
        next_coin_proof: CoinProof {
            parent_coin_info: parent_coin_id,
            inner_puzzle_hash: inner_puzzle_hash.into(),
            amount,
        },
        prev_subtotal: 0,
        extra_delta: 0,
    }
    .to_node_ptr(a)?;

    let coin_spend = CoinSpend::new(
        coin,
        Program::from_node_ptr(a, puzzle).unwrap(),
        Program::from_node_ptr(a, solution).unwrap(),
    );

    Ok(EveSpendInfo {
        puzzle_hash,
        coin_spend,
    })
}

/// Creates spend for a list of CAT coins.
#[allow(clippy::too_many_arguments)] // TODO: fix
pub async fn spend_cat_coins(
    a: &mut Allocator,
    standard_puzzle_ptr: NodePtr,
    cat_puzzle_ptr: NodePtr,
    derivation_store: &impl DerivationStore,
    peer: &Peer,
    coins: Vec<Coin>,
    conditions: Vec<CatCondition<NodePtr>>,
    asset_id: [u8; 32],
) -> Vec<CoinSpend> {
    let mut spends = Vec::new();
    let mut conditions = Some(conditions);

    let parents = peer
        .register_for_coin_updates(coins.iter().map(|coin| coin.parent_coin_info).collect(), 0)
        .await
        .unwrap();

    for (i, coin) in coins.into_iter().enumerate() {
        // Coin info.
        let puzzle_hash = &coin.puzzle_hash;
        let index = derivation_store
            .index_of_ph(puzzle_hash.into())
            .await
            .expect("cannot spend coin with unknown puzzle hash");

        let synthetic_key = derivation_store
            .public_key(index)
            .await
            .expect("cannot spend coin with unknown public key");
        let p2_puzzle_hash = standard_puzzle_hash(&synthetic_key);

        // Lineage proof.
        let parent = parents
            .iter()
            .find(|coin_state| coin_state.coin.coin_id() == coin.parent_coin_info.to_bytes())
            .cloned()
            .unwrap();

        let cat_args: CatArgs<NodePtr> = request_puzzle_args(
            a,
            peer,
            &coin,
            CAT_PUZZLE_HASH,
            parent.spent_height.unwrap(),
        )
        .await
        .unwrap();

        // Spend information.
        let spend = CatSpend {
            coin,
            synthetic_key,
            conditions: if i == 0 {
                conditions.take().unwrap()
            } else {
                Vec::new()
            },
            extra_delta: 0,
            p2_puzzle_hash,
            lineage_proof: LineageProof {
                parent_coin_info: parent.coin.parent_coin_info,
                inner_puzzle_hash: tree_hash(a, cat_args.inner_puzzle).into(),
                amount: parent.coin.amount,
            },
        };
        spends.push(spend);
    }

    create_raw_cat_spends(a, standard_puzzle_ptr, cat_puzzle_ptr, asset_id, &spends).unwrap()
}

/// Creates a set of CAT coin spends for a given asset id.
pub fn create_raw_cat_spends(
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

/// Calculates the puzzle hash of a CAT without generating the full puzzle.
pub fn cat_puzzle_hash(asset_id: [u8; 32], inner_puzzle_hash: [u8; 32]) -> [u8; 32] {
    let mod_hash = tree_hash_atom(&CAT_PUZZLE_HASH);
    let asset_id_hash = tree_hash_atom(&asset_id);
    curry_tree_hash(
        CAT_PUZZLE_HASH,
        &[mod_hash, asset_id_hash, inner_puzzle_hash],
    )
}
