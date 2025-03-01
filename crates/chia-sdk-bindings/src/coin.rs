use bindy::Result;
use chia_protocol::{Bytes32, Coin};

pub trait CoinExt {
    fn coin_id(&self) -> Result<Bytes32>;
}

impl CoinExt for Coin {
    fn coin_id(&self) -> Result<Bytes32> {
        Ok(self.coin_id())
    }
}
