use crate::Spend;

use super::Cat;

#[derive(Debug, Clone, Copy)]
pub struct CatSpend {
    pub cat: Cat,
    pub inner_spend: Spend,
    pub revoke: bool,
}

impl CatSpend {
    pub fn new(cat: Cat, inner_spend: Spend) -> Self {
        Self {
            cat,
            inner_spend,
            revoke: false,
        }
    }
}
