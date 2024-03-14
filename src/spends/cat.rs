use chia_client::Peer;
use chia_protocol::{Coin, CoinSpend, RejectPuzzleSolution};
use chia_wallet::{
    cat::{CatArgs, CAT_PUZZLE_HASH},
    standard::standard_puzzle_hash,
    LineageProof,
};
use clvm_traits::{FromClvm, FromClvmError, ToClvmError};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{allocator::NodePtr, serde::node_from_bytes, Allocator};
use thiserror::Error;

use crate::{CatCondition, DerivationStore};

mod issuance;
mod raw_spend;

pub use issuance::*;
pub use raw_spend::*;

/// An error that occurs while trying to spend a CAT.
#[derive(Debug, Error)]
pub enum CatSpendError {
    /// When the mod hash of a parent coin is not a CAT.
    #[error("wrong mod hash")]
    WrongModHash([u8; 32]),

    /// When conversion to a CLVM NodePtr fails.
    #[error("to clvm error: {0}")]
    ToClvm(#[from] ToClvmError),

    /// When conversion from a CLVM NodePtr fails.
    #[error("from clvm error: {0}")]
    FromClvm(#[from] FromClvmError),

    /// When conversion to or from bytes fails.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// When a request to the peer fails.
    #[error("peer error: {0}")]
    Peer(#[from] chia_client::Error<RejectPuzzleSolution>),
}

/// Creates spend for a list of CAT coins.
#[allow(clippy::too_many_arguments)]
pub async fn construct_cat_spends(
    a: &mut Allocator,
    standard_puzzle_ptr: NodePtr,
    cat_puzzle_ptr: NodePtr,
    peer: &Peer,
    derivation_store: &impl DerivationStore,
    coins: Vec<Coin>,
    conditions: Vec<CatCondition<NodePtr>>,
    asset_id: [u8; 32],
) -> Result<Vec<CoinSpend>, CatSpendError> {
    let mut spends = Vec::new();
    let mut conditions = Some(conditions);

    let parents = peer
        .register_for_coin_updates(coins.iter().map(|coin| coin.parent_coin_info).collect(), 0)
        .await
        .unwrap();

    for (i, coin) in coins.into_iter().enumerate() {
        // Coin info.
        let puzzle_hash = &coin.puzzle_hash;
        let index = derivation_store
            .puzzle_hash_index(puzzle_hash.into())
            .await
            .expect("cannot spend coin with unknown puzzle hash");

        let synthetic_key = derivation_store
            .public_key(index)
            .await
            .expect("cannot spend coin with unknown public key");
        let p2_puzzle_hash = standard_puzzle_hash(&synthetic_key);

        // Lineage proof.
        let parent = parents
            .iter()
            .find(|coin_state| coin_state.coin.coin_id() == coin.parent_coin_info.to_bytes())
            .cloned()
            .unwrap();

        let puzzle = peer
            .request_puzzle_and_solution(coin.parent_coin_info, parent.spent_height.unwrap())
            .await?
            .puzzle;

        let ptr = node_from_bytes(a, puzzle.as_slice())?;
        let puzzle: CurriedProgram<NodePtr, CatArgs<NodePtr>> = FromClvm::from_clvm(a, ptr)?;

        let mod_hash = tree_hash(a, puzzle.program);
        if mod_hash != CAT_PUZZLE_HASH {
            return Err(CatSpendError::WrongModHash(mod_hash));
        }

        // Spend information.
        let spend = CatSpend {
            coin,
            synthetic_key,
            conditions: if i == 0 {
                conditions.take().unwrap()
            } else {
                Vec::new()
            },
            extra_delta: 0,
            p2_puzzle_hash,
            lineage_proof: LineageProof {
                parent_coin_info: parent.coin.parent_coin_info,
                inner_puzzle_hash: tree_hash(a, puzzle.args.inner_puzzle).into(),
                amount: parent.coin.amount,
            },
        };
        spends.push(spend);
    }

    Ok(spend_cat_coins(
        a,
        standard_puzzle_ptr,
        cat_puzzle_ptr,
        asset_id,
        &spends,
    )?)
}
