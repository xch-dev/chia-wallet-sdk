use crate::Spend;

use super::Cat;

#[derive(Debug, Clone, Copy)]
pub struct CatSpend {
    pub cat: Cat,
    pub inner_spend: Spend,
    pub extra_delta: i64,
}

impl CatSpend {
    pub fn new(cat: Cat, inner_spend: Spend) -> Self {
        Self {
            cat,
            inner_spend,
            extra_delta: 0,
        }
    }

    pub fn with_extra_delta(cat: Cat, inner_spend: Spend, extra_delta: i64) -> Self {
        Self {
            cat,
            inner_spend,
            extra_delta,
        }
    }
}
