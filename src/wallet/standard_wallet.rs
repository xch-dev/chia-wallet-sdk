use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend};
use chia_wallet::standard::{standard_puzzle_hash, STANDARD_PUZZLE};
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};

use crate::{
    spend_standard_coin, Condition, DerivationState, DerivationWallet, KeyStore, PuzzleGenerator,
};

pub type StandardWallet<K, S> = DerivationWallet<StandardPuzzleGenerator, K, S>;

#[derive(Debug, Clone, Copy)]
pub struct StandardPuzzleGenerator;

impl PuzzleGenerator for StandardPuzzleGenerator {
    fn puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32] {
        standard_puzzle_hash(public_key)
    }
}

impl<K, S> StandardWallet<K, S>
where
    K: KeyStore + 'static,
    S: DerivationState + 'static,
{
    pub async fn spend_coins(
        &self,
        coins: Vec<Coin>,
        conditions: &[Condition<NodePtr>],
    ) -> Vec<CoinSpend> {
        let mut a = Allocator::new();
        let standard_puzzle = node_from_bytes(&mut a, &STANDARD_PUZZLE).unwrap();

        let mut coin_spends = Vec::new();
        for (i, coin) in coins.into_iter().enumerate() {
            let puzzle_hash = &coin.puzzle_hash;
            let index = self
                .derivation_index(puzzle_hash.into())
                .await
                .expect("cannot spend coin with unknown puzzle hash");
            let synthetic_key = self.public_key(index).await;

            coin_spends.push(
                spend_standard_coin(
                    &mut a,
                    standard_puzzle,
                    coin,
                    synthetic_key,
                    if i == 0 { conditions } else { &[] },
                )
                .unwrap(),
            );
        }
        coin_spends
    }
}
