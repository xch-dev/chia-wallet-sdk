mod vault_info;
mod vault_memos;

pub use vault_info::*;
pub use vault_memos::*;

use chia_protocol::Coin;

#[derive(Debug, Clone)]
pub struct Vault {
    pub coin: Coin,
}
