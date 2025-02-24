use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::Bytes32;

use crate::{BlsPairWithCoin, Coin, CoinSpend, SecretKey};

#[derive(Default, Clone)]
pub struct Simulator(Arc<Mutex<chia_sdk_test::Simulator>>);

impl Simulator {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn new_coin(&self, puzzle_hash: Bytes32, amount: u64) -> Result<Coin> {
        Ok(self.0.lock().unwrap().new_coin(puzzle_hash, amount).into())
    }

    pub fn bls(&self, amount: u64) -> Result<BlsPairWithCoin> {
        Ok(self.0.lock().unwrap().bls(amount).into())
    }

    pub fn spend_coins(
        &self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: Vec<SecretKey>,
    ) -> Result<()> {
        self.0.lock().unwrap().spend_coins(
            coin_spends.into_iter().map(Into::into).collect::<Vec<_>>(),
            &secret_keys.into_iter().map(|sk| sk.0).collect::<Vec<_>>(),
        )?;
        Ok(())
    }
}
