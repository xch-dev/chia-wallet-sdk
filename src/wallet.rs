use chia_protocol::Coin;

mod derivation_state;
mod derivation_wallet;
mod puzzle_generator;
mod standard_wallet;

pub use derivation_state::*;
pub use derivation_wallet::*;
pub use puzzle_generator::*;
pub use standard_wallet::*;

pub trait Wallet {
    fn spendable_coins(&self) -> Vec<Coin>;

    fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }
}
