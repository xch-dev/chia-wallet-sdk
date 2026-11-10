use crate::Spend;

use super::Cat;

/// A [`Cat`] paired with the inner spend to use for it, as passed to [`Cat::spend_all`].
#[derive(Debug, Clone, Copy)]
pub struct CatSpend {
    /// The CAT coin being spent.
    pub cat: Cat,
    /// The spend of the p2 puzzle, or of the hidden puzzle if [`hidden`](Self::hidden) is set.
    pub spend: Spend,
    /// Whether to spend the hidden puzzle of the [`RevocationLayer`](crate::RevocationLayer)
    /// instead of the p2 puzzle. Only valid for revocable CATs.
    pub hidden: bool,
}

impl CatSpend {
    /// Creates a normal spend of the CAT's p2 puzzle.
    pub fn new(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: false,
        }
    }

    /// Creates a revocation spend, where `spend` is a spend of the hidden puzzle
    /// whose hash is [`CatInfo::hidden_puzzle_hash`](crate::CatInfo::hidden_puzzle_hash).
    pub fn revoke(cat: Cat, spend: Spend) -> Self {
        Self {
            cat,
            spend,
            hidden: true,
        }
    }
}
