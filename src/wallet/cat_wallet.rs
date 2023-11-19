use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, RegisterForCoinUpdates, RespondToCoinUpdates};
use chia_wallet::{
    cat::{CatArgs, CAT_PUZZLE, CAT_PUZZLE_HASH},
    standard::STANDARD_PUZZLE,
    LineageProof,
};
use clvm_utils::tree_hash;
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};

use crate::{
    cat_puzzle_hash, request_puzzle_args, spend_cat_coins, CatCondition, CatSpend, DerivationState,
    DerivationWallet, KeyStore, PuzzleGenerator, StandardPuzzleGenerator,
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
        conditions: Vec<CatCondition<NodePtr>>,
    ) -> Vec<CoinSpend> {
        let mut a = Allocator::new();
        let standard_puzzle = node_from_bytes(&mut a, &STANDARD_PUZZLE).unwrap();
        let cat_puzzle = node_from_bytes(&mut a, &CAT_PUZZLE).unwrap();

        let mut spends = Vec::new();
        let mut conditions = Some(conditions);

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
                .await
                .expect("cannot spend coin with unknown puzzle hash");
            let synthetic_key = self.public_key(index).await;
            let p2_puzzle_hash = StandardPuzzleGenerator.puzzle_hash(&synthetic_key);

            // Lineage proof.
            let parent_coin_state = parent_coin_updates
                .coin_states
                .iter()
                .find(|coin_state| coin_state.coin == coin)
                .cloned()
                .unwrap();

            let cat_args: CatArgs<NodePtr> = request_puzzle_args(
                &mut a,
                self.peer(),
                &coin,
                CAT_PUZZLE_HASH,
                parent_coin_state.spent_height.unwrap(),
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
                    parent_coin_info: parent_coin_state.coin.parent_coin_info,
                    inner_puzzle_hash: tree_hash(&a, cat_args.inner_puzzle).into(),
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
