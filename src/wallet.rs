use chia_protocol::Coin;

mod derivation_info;
mod derivation_wallet;
mod standard_state;
mod standard_wallet;

pub use derivation_info::*;
pub use derivation_wallet::*;
pub use standard_state::*;
pub use standard_wallet::*;

pub trait Wallet {
    fn spendable_coins(&self) -> Vec<Coin>;

    fn spendable_balance(&self) -> u64 {
        self.spendable_coins()
            .iter()
            .fold(0, |balance, coin| balance + coin.amount)
    }
}
