mod standard_wallet;

pub use standard_wallet::*;

pub trait Wallet {
    fn spendable_balance(&self) -> u64;
}
