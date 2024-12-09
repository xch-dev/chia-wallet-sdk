mod m_of_n;
mod member;
mod restriction;
mod vault_info;

pub use m_of_n::*;
pub use member::*;
pub use restriction::*;
pub use vault_info::*;

use chia_protocol::Coin;

#[derive(Debug, Clone)]
pub struct Vault {
    pub coin: Coin,
}
