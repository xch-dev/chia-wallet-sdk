use chia_protocol::{Bytes32, Coin};
use chia_wallet::{
    cat::{cat_puzzle_hash, CatArgs, CatSolution, CAT_PUZZLE_HASH},
    LineageProof,
};
use clvm_traits::{FromClvm, FromClvmError};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{
    reduction::{EvalErr, Reduction},
    run_program, Allocator, ChiaDialect, NodePtr,
};
use thiserror::Error;

use crate::{CatInfo, CreateCoin, DidInfo, NftInfo};

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("clvm error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("invalid puzzle")]
    InvalidPuzzle,

    #[error("incorrect hint")]
    IncorrectHint,
}

pub enum Puzzle {
    Cat(CatInfo),
    Did(DidInfo<NodePtr>),
    Nft(NftInfo<NodePtr>),
}

impl Puzzle {
    pub fn parse(
        allocator: &mut Allocator,
        parent_puzzle: NodePtr,
        parent_solution: NodePtr,
        parent_coin: Coin,
        coin: Coin,
        max_cost: u64,
    ) -> Result<Self, ParserError> {
        let CurriedProgram { program, args } =
            CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, parent_puzzle)?;

        match tree_hash(allocator, program) {
            CAT_PUZZLE_HASH => {
                let cat_args = CatArgs::<NodePtr>::from_clvm(allocator, args)?;
                let cat_solution = CatSolution::<NodePtr>::from_clvm(allocator, parent_solution)?;

                let Reduction(_cost, output) = run_program(
                    allocator,
                    &ChiaDialect::new(0),
                    cat_args.inner_puzzle,
                    cat_solution.inner_puzzle_solution,
                    max_cost,
                )?;

                let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;
                let mut p2_puzzle_hash = None;

                for condition in conditions {
                    let Ok(create_coin) = CreateCoin::from_clvm(allocator, condition) else {
                        continue;
                    };

                    let cat_puzzle_hash = Bytes32::new(cat_puzzle_hash(
                        cat_args.tail_program_hash.into(),
                        create_coin.puzzle_hash().into(),
                    ));

                    if cat_puzzle_hash == coin.puzzle_hash && create_coin.amount() == coin.amount {
                        p2_puzzle_hash = Some(create_coin.puzzle_hash());
                        break;
                    }
                }

                let Some(p2_puzzle_hash) = p2_puzzle_hash else {
                    return Err(ParserError::IncorrectHint);
                };

                Ok(Puzzle::Cat(CatInfo {
                    asset_id: cat_args.tail_program_hash,
                    p2_puzzle_hash,
                    coin,
                    lineage_proof: LineageProof {
                        parent_coin_info: parent_coin.parent_coin_info,
                        inner_puzzle_hash: tree_hash(allocator, cat_args.inner_puzzle).into(),
                        amount: parent_coin.amount,
                    },
                }))
            }
            _ => Err(ParserError::InvalidPuzzle),
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_wallet::{
        standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
        DeriveSynthetic,
    };
    use clvm_traits::ToNodePtr;

    use crate::{
        testing::SECRET_KEY, Chainable, CreateCoinWithMemos, IssueCat, SpendContext, StandardSpend,
        WalletSimulator,
    };

    use super::*;

    #[tokio::test]
    async fn test_parse_cat() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let sk = SECRET_KEY.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
        let pk = sk.public_key();
        let puzzle_hash = standard_puzzle_hash(&pk).into();

        let parent = sim.generate_coin(puzzle_hash, 1).await.coin;

        let (issue_cat, issuance_info) = IssueCat::new(parent.coin_id())
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash,
                amount: 1,
                memos: vec![puzzle_hash.to_vec().into()],
            })?)
            .multi_issuance(&mut ctx, pk.clone(), 1)?;

        let cat_info = CatInfo {
            asset_id: issuance_info.asset_id,
            p2_puzzle_hash: puzzle_hash,
            coin: Coin::new(
                issuance_info.eve_coin.coin_id(),
                cat_puzzle_hash(issuance_info.asset_id.into(), puzzle_hash.into()).into(),
                1,
            ),
            lineage_proof: LineageProof {
                parent_coin_info: issuance_info.eve_coin.parent_coin_info,
                inner_puzzle_hash: issuance_info.eve_inner_puzzle_hash,
                amount: 1,
            },
        };

        let standard_spend = StandardSpend::new()
            .chain(issue_cat)
            .finish(&mut ctx, parent, pk)?;

        let coin_spend = standard_spend
            .into_iter()
            .find(|cs| cs.coin.coin_id() == issuance_info.eve_coin.coin_id())
            .unwrap();

        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let parse = Puzzle::parse(
            &mut allocator,
            puzzle,
            solution,
            issuance_info.eve_coin,
            cat_info.coin.clone(),
            u64::MAX,
        )?;

        match parse {
            Puzzle::Cat(parsed_cat_info) => assert_eq!(parsed_cat_info, cat_info),
            _ => panic!("unexpected puzzle"),
        }

        Ok(())
    }
}
