use bindy::Result;
use chia_protocol::Bytes32;

#[derive(Clone)]
pub struct Coin {
    pub parent_coin_info: Bytes32,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

impl Coin {
    pub fn coin_id(&self) -> Result<Bytes32> {
        Ok(
            chia_protocol::Coin::new(self.parent_coin_info, self.puzzle_hash, self.amount)
                .coin_id(),
        )
    }
}
