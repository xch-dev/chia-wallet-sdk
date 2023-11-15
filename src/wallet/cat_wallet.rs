use chia_bls::PublicKey;
use chia_protocol::{
    wallet_protocol::RequestPuzzleSolution, Coin, CoinSpend, Program, RegisterForCoinUpdates,
    RespondPuzzleSolution, RespondToCoinUpdates,
};
use chia_wallet::{
    cat::{CatArgs, CatSolution, CoinProof, CAT_PUZZLE, CAT_PUZZLE_HASH},
    standard::{StandardArgs, StandardSolution, STANDARD_PUZZLE},
    LineageProof,
};
use clvm_traits::{clvm_quote, FromPtr, ToClvmError, ToPtr};
use clvm_utils::{curry_tree_hash, tree_hash, tree_hash_atom, CurriedProgram};
use clvmr::{
    allocator::NodePtr,
    serde::{node_from_bytes, node_to_bytes},
    Allocator,
};

use crate::{
    CatCondition, Condition, CreateCoin, DerivationState, DerivationWallet, KeyStore,
    PuzzleGenerator, StandardPuzzleGenerator,
};

pub type CatWallet<I, K, S> = DerivationWallet<CatPuzzleGenerator<I>, K, S>;
pub type StandardCatWallet<K, S> = CatWallet<StandardPuzzleGenerator, K, S>;

#[derive(Debug, Clone, Copy)]
pub struct CatPuzzleGenerator<I>
where
    I: PuzzleGenerator,
{
    asset_id: [u8; 32],
    inner_puzzle_generator: I,
}

impl<I> CatPuzzleGenerator<I>
where
    I: PuzzleGenerator,
{
    pub fn new(asset_id: [u8; 32], inner_puzzle_generator: I) -> Self {
        Self {
            asset_id,
            inner_puzzle_generator,
        }
    }

    pub fn asset_id(&self) -> [u8; 32] {
        self.asset_id
    }
}

impl<I> PuzzleGenerator for CatPuzzleGenerator<I>
where
    I: PuzzleGenerator,
{
    fn puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32] {
        cat_puzzle_hash(
            self.asset_id,
            self.inner_puzzle_generator.puzzle_hash(public_key),
        )
    }
}

impl<K, S> StandardCatWallet<K, S>
where
    K: KeyStore + 'static,
    S: DerivationState + 'static,
{
    pub async fn spend_coins(
        &self,
        coins: Vec<Coin>,
        conditions: &[CatCondition<NodePtr>],
    ) -> Vec<CoinSpend> {
        let mut a = Allocator::new();
        let standard_puzzle = node_from_bytes(&mut a, &STANDARD_PUZZLE).unwrap();
        let cat_puzzle = node_from_bytes(&mut a, &CAT_PUZZLE).unwrap();

        let mut spends = Vec::new();

        let parent_coin_updates: RespondToCoinUpdates = self
            .peer()
            .request(RegisterForCoinUpdates::new(
                coins.iter().map(|coin| coin.parent_coin_info).collect(),
                0,
            ))
            .await
            .unwrap();

        for (i, coin) in coins.into_iter().enumerate() {
            // Coin info.
            let puzzle_hash = &coin.puzzle_hash;
            let index = self
                .derivation_index(puzzle_hash.into())
                .expect("cannot spend coin with unknown puzzle hash");
            let synthetic_key = self.public_key(index);
            let p2_puzzle_hash = StandardPuzzleGenerator.puzzle_hash(&synthetic_key);

            // Lineage proof.
            let parent_coin_state = parent_coin_updates
                .coin_states
                .iter()
                .find(|coin_state| coin_state.coin == coin)
                .cloned()
                .unwrap();

            let response: RespondPuzzleSolution = self
                .peer()
                .request(RequestPuzzleSolution::new(
                    coin.parent_coin_info,
                    parent_coin_state.spent_height.unwrap(),
                ))
                .await
                .unwrap();

            let response = response.response;

            let parent_ptr = node_from_bytes(&mut a, response.puzzle.as_slice()).unwrap();

            let parent_puzzle: CurriedProgram<NodePtr, CatArgs<NodePtr>> =
                FromPtr::from_ptr(&a, parent_ptr).unwrap();

            assert_eq!(tree_hash(&a, parent_puzzle.program), CAT_PUZZLE_HASH);

            let parent_inner_puzzle_hash = tree_hash(&a, parent_puzzle.args.inner_puzzle);

            // Spend information.
            let spend = CatSpend {
                coin,
                synthetic_key,
                conditions: if i == 0 { conditions } else { &[] },
                extra_delta: 0,
                p2_puzzle_hash,
                lineage_proof: LineageProof {
                    parent_coin_info: parent_coin_state.coin.parent_coin_info,
                    inner_puzzle_hash: parent_inner_puzzle_hash.into(),
                    amount: parent_coin_state.coin.amount,
                },
            };
            spends.push(spend);
        }

        spend_cat_coins(
            &mut a,
            standard_puzzle,
            cat_puzzle,
            &self.puzzle_generator().asset_id,
            &spends,
        )
        .unwrap()
    }
}

pub struct CatSpend<'a> {
    coin: Coin,
    synthetic_key: PublicKey,
    conditions: &'a [CatCondition<NodePtr>],
    extra_delta: i64,
    p2_puzzle_hash: [u8; 32],
    lineage_proof: LineageProof,
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
