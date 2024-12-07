mod m_of_n;
mod restriction;
mod vault_info;
mod vault_memos;

pub use m_of_n::*;
pub use restriction::*;
pub use vault_info::*;
pub use vault_memos::*;

use chia_protocol::Coin;

#[derive(Debug, Clone)]
pub struct Vault {
    pub coin: Coin,
}
