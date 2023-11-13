use anyhow::Result;
use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::standard::{
    standard_puzzle_hash, StandardArgs, StandardSolution, STANDARD_PUZZLE,
};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};

use crate::{
    CoinSelectionError, CoinSelectionMode, Condition, DerivationState, DerivationWallet, KeyStore,
    PuzzleGenerator, Wallet,
};

pub type StandardWallet<K, S> = DerivationWallet<StandardPuzzleGenerator, K, S>;

pub struct StandardPuzzleGenerator;

impl PuzzleGenerator for StandardPuzzleGenerator {
    fn puzzle_hash(public_key: &PublicKey) -> [u8; 32] {
        standard_puzzle_hash(public_key)
    }
}

impl<K, S> DerivationWallet<StandardPuzzleGenerator, K, S>
where
    K: KeyStore + 'static,
    S: DerivationState + 'static,
{
    pub fn spend_amount(
        &self,
        amount: u64,
        mode: CoinSelectionMode,
        conditions: &[Condition],
    ) -> Result<Vec<CoinSpend>, CoinSelectionError> {
        let coins = self.select_coins(amount, mode)?;
        Ok(self.spend_coins(coins, conditions))
    }

    pub fn spend_coins(&self, coins: Vec<Coin>, conditions: &[Condition]) -> Vec<CoinSpend> {
        let a = &mut Allocator::new();
        let standard_puzzle = node_from_bytes(a, &STANDARD_PUZZLE).unwrap();

        coins
            .into_iter()
            .enumerate()
            .map(|(i, coin)| {
                self.spend_coin(
                    a,
                    standard_puzzle,
                    coin,
                    if i == 0 { conditions } else { &[] },
                )
            })
            .collect::<Result<_>>()
            .unwrap()
    }

    fn spend_coin(
        &self,
        a: &mut Allocator,
        standard_puzzle: NodePtr,
        coin: Coin,
        conditions: &[Condition],
    ) -> Result<CoinSpend> {
        let puzzle_hash = &coin.puzzle_hash;
        let index = self
            .derivation_index(puzzle_hash.into())
            .expect("cannot spend coin with unknown puzzle hash");
        let synthetic_key = self.public_key(index);

        let puzzle = CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs { synthetic_key },
        }
        .to_clvm(a)?;

        let solution = StandardSolution {
            original_public_key: None,
            delegated_puzzle: clvm_quote!(conditions).to_clvm(a).unwrap(),
            solution: a.null(),
        }
        .to_clvm(a)?;

        let puzzle = Program::from_clvm(a, puzzle)?;
        let solution = Program::from_clvm(a, solution)?;
        Ok(CoinSpend::new(coin, puzzle, solution))
    }
}
