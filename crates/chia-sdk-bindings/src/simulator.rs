use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::SecretKey;
use chia_protocol::{Bytes32, Coin, CoinSpend, CoinState, SpendBundle};
use chia_sdk_test::SimulatorConfig;

use crate::{BlsPairWithCoin, TweakData};

#[derive(Default, Clone)]
pub struct Simulator(Arc<Mutex<chia_sdk_test::Simulator>>);

impl Simulator {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn with_seed(seed: u64) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(
            chia_sdk_test::Simulator::with_config(SimulatorConfig {
                seed,
                ..Default::default()
            }),
        ))))
    }

    pub fn height(&self) -> Result<u32> {
        Ok(self.0.lock().unwrap().height())
    }

    pub fn next_timestamp(&self) -> Result<u64> {
        Ok(self.0.lock().unwrap().next_timestamp())
    }

    pub fn header_hash(&self) -> Result<Bytes32> {
        Ok(self.0.lock().unwrap().header_hash())
    }

    pub fn header_hash_of(&self, height: u32) -> Result<Option<Bytes32>> {
        Ok(self.0.lock().unwrap().header_hash_of(height))
    }

    pub fn insert_coin(&self, coin: Coin) -> Result<()> {
        self.0.lock().unwrap().insert_coin(coin);
        Ok(())
    }

    pub fn new_coin(&self, puzzle_hash: Bytes32, amount: u64) -> Result<Coin> {
        Ok(self.0.lock().unwrap().new_coin(puzzle_hash, amount))
    }

    pub fn bls(&self, amount: u64) -> Result<BlsPairWithCoin> {
        Ok(self.0.lock().unwrap().bls(amount).into())
    }

    pub fn set_next_timestamp(&self, time: u64) -> Result<()> {
        self.0.lock().unwrap().set_next_timestamp(time)?;
        Ok(())
    }

    pub fn pass_time(&self, time: u64) -> Result<()> {
        self.0.lock().unwrap().pass_time(time);
        Ok(())
    }

    pub fn hint_coin(&self, coin_id: Bytes32, hint: Bytes32) -> Result<()> {
        self.0.lock().unwrap().hint_coin(coin_id, hint);
        Ok(())
    }

    pub fn coin_state(&self, coin_id: Bytes32) -> Result<Option<CoinState>> {
        Ok(self.0.lock().unwrap().coin_state(coin_id))
    }

    pub fn children(&self, coin_id: Bytes32) -> Result<Vec<CoinState>> {
        Ok(self.0.lock().unwrap().children(coin_id))
    }

    pub fn hinted_coins(&self, hint: Bytes32) -> Result<Vec<Bytes32>> {
        Ok(self.0.lock().unwrap().hinted_coins(hint))
    }

    pub fn coin_spend(&self, coin_id: Bytes32) -> Result<Option<CoinSpend>> {
        Ok(self.0.lock().unwrap().coin_spend(coin_id))
    }

    /// Construct a [`TweakData`] from one block of the simulator's history
    /// (CHIP-0057 silent-payments test helper).
    ///
    /// Wraps `chia_sdk_test::silent_payments::tweak_data_from_simulator_block`,
    /// converting the driver-side `TweakData` into the binding-facade
    /// [`TweakData`] via the existing `From` impl in `crate::silent_payments`.
    pub fn tweak_data_from_block(&self, height: u32) -> Result<TweakData> {
        Ok(
            chia_sdk_test::silent_payments::tweak_data_from_simulator_block(
                &self.0.lock().unwrap(),
                height,
            )
            .into(),
        )
    }

    /// Return every coin spend that resolved at `height` (binding facade).
    ///
    /// Thin wrapper over `chia_sdk_test::Simulator::block_spends(height)` —
    /// returns the block's removals as a `Vec<CoinSpend>` suitable for
    /// passing to `SilentPayments::tweak_data_from_block_spends`. Returns
    /// an empty vector when no spends resolved at `height`.
    pub fn block_spends(&self, height: u32) -> Result<Vec<CoinSpend>> {
        Ok(self.0.lock().unwrap().block_spends(height))
    }

    /// Return every coin created at `height` (binding facade).
    ///
    /// Thin wrapper over `chia_sdk_test::Simulator::block_outputs(height)` —
    /// returns the block's additions as a `Vec<Coin>` suitable for pairing
    /// with `block_spends` when calling
    /// `SilentPayments::tweak_data_from_block_spends`. Returns an empty
    /// vector when no coins were created at `height`.
    pub fn block_outputs(&self, height: u32) -> Result<Vec<Coin>> {
        Ok(self.0.lock().unwrap().block_outputs(height))
    }

    pub fn spend_coins(
        &self,
        coin_spends: Vec<CoinSpend>,
        secret_keys: Vec<SecretKey>,
    ) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .spend_coins(coin_spends, &secret_keys)?;
        Ok(())
    }

    pub fn new_transaction(&self, spend_bundle: SpendBundle) -> Result<()> {
        self.0.lock().unwrap().new_transaction(spend_bundle)?;
        Ok(())
    }

    pub fn lookup_coin_ids(&self, coin_ids: Vec<Bytes32>) -> Result<Vec<CoinState>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .lookup_coin_ids(&coin_ids.into_iter().collect()))
    }

    pub fn lookup_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        include_hints: bool,
    ) -> Result<Vec<CoinState>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .lookup_puzzle_hashes(puzzle_hashes.into_iter().collect(), include_hints))
    }

    pub fn unspent_coins(&self, puzzle_hash: Bytes32, include_hints: bool) -> Result<Vec<Coin>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .unspent_coins(puzzle_hash, include_hints))
    }

    pub fn create_block(&self) -> Result<()> {
        self.0.lock().unwrap().create_block();
        Ok(())
    }
}
