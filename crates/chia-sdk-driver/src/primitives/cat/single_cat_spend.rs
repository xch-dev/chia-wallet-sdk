use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::CoinProof;

use crate::Spend;

/// The solution parameters for spending a single CAT coin with [`Cat::spend`](crate::Cat::spend).
///
/// The ring fields link this coin to its neighbors in the CAT announcement ring.
/// [`Cat::spend_all`](crate::Cat::spend_all) calculates them for you.
#[derive(Debug, Clone, Copy)]
pub struct SingleCatSpend {
    /// The coin id of the previous CAT coin in the ring.
    pub prev_coin_id: Bytes32,
    /// The parent coin id, inner puzzle hash, and amount of the next CAT coin in the ring.
    pub next_coin_proof: CoinProof,
    /// The sum of the deltas (input amount minus output amounts) of the coins before this one in the ring.
    pub prev_subtotal: i64,
    /// The amount by which the TAIL allows this spend to change the supply. It must be zero unless the TAIL is run.
    pub extra_delta: i64,
    /// The spend of the inner puzzle (the p2 puzzle, or the hidden puzzle if [`revoke`](Self::revoke) is set).
    pub p2_spend: Spend,
    /// Whether to spend the hidden puzzle of the revocation layer instead of the p2 puzzle.
    pub revoke: bool,
}

impl SingleCatSpend {
    /// Creates the parameters for spending `coin` on its own, in a ring of one.
    ///
    /// The coin is its own previous and next coin, so `inner_puzzle_hash` must be the
    /// inner puzzle hash of `coin` (see [`CatInfo::inner_puzzle_hash`](crate::CatInfo::inner_puzzle_hash)).
    pub fn eve(coin: Coin, inner_puzzle_hash: Bytes32, p2_spend: Spend) -> Self {
        Self {
            prev_coin_id: coin.coin_id(),
            next_coin_proof: CoinProof {
                parent_coin_info: coin.parent_coin_info,
                inner_puzzle_hash,
                amount: coin.amount,
            },
            prev_subtotal: 0,
            extra_delta: 0,
            p2_spend,
            revoke: false,
        }
    }
}
