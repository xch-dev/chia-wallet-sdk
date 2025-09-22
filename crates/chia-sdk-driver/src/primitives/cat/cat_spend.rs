use crate::Spend;

use super::Cat;

#[derive(Debug, Clone, Copy)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
    pub hidden: bool,
}

impl CatSpend {
    pub fn new(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: false,
        }
    }

    pub fn revoke(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: true,
        }
    }
}
