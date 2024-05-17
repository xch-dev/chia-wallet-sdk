use chia_protocol::Bytes32;
use chia_puzzles::{
    cat::{CatArgs, CatSolution, CAT_PUZZLE_HASH},
    LineageProof,
};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{reduction::Reduction, run_program, Allocator, ChiaDialect, NodePtr};

use crate::{CatInfo, CreateCoin, ParseContext, ParseError};

pub fn parse_cat(
    allocator: &mut Allocator,
    ctx: &ParseContext,
    max_cost: u64,
) -> Result<Option<CatInfo>, ParseError> {
    if ctx.mod_hash().to_bytes() != CAT_PUZZLE_HASH.to_bytes() {
        return Ok(None);
    }

    let args = CatArgs::<NodePtr>::from_clvm(allocator, ctx.args())?;
    let solution = CatSolution::<NodePtr>::from_clvm(allocator, ctx.solution())?;

    let Reduction(_cost, output) = run_program(
        allocator,
        &ChiaDialect::new(0),
        args.inner_puzzle,
        solution.inner_puzzle_solution,
        max_cost,
    )?;

    let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;
    let mut p2_puzzle_hash = None;

    for condition in conditions {
        let Ok(create_coin) = CreateCoin::from_clvm(allocator, condition) else {
            continue;
        };

        let cat_puzzle_hash = CurriedProgram {
            program: CAT_PUZZLE_HASH,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                tail_program_hash: args.tail_program_hash,
                inner_puzzle: TreeHash::from(create_coin.puzzle_hash()),
            },
        }
        .tree_hash();

        if Bytes32::from(cat_puzzle_hash) == ctx.coin().puzzle_hash
            && create_coin.amount() == ctx.coin().amount
        {
            p2_puzzle_hash = Some(create_coin.puzzle_hash());
            break;
        }
    }

    let Some(p2_puzzle_hash) = p2_puzzle_hash else {
        return Err(ParseError::MissingCreateCoin);
    };

    Ok(Some(CatInfo {
        asset_id: args.tail_program_hash,
        p2_puzzle_hash,
        coin: ctx.coin(),
        lineage_proof: LineageProof {
            parent_parent_coin_id: ctx.parent_coin().parent_coin_info,
            parent_inner_puzzle_hash: tree_hash(allocator, args.inner_puzzle).into(),
            parent_amount: ctx.parent_coin().amount,
        },
    }))
}

#[cfg(test)]
mod tests {
    use chia_bls::PublicKey;
    use chia_protocol::Coin;
    use chia_puzzles::standard::{StandardArgs, STANDARD_PUZZLE_HASH};
    use clvm_traits::ToNodePtr;
    use clvm_utils::CurriedProgram;

    use crate::{
        parse_puzzle, Chainable, CreateCoinWithMemos, IssueCat, SpendContext, StandardSpend,
    };

    use super::*;

    #[test]
    fn test_parse_cat() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs { synthetic_key: pk },
        }
        .tree_hash()
        .into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let (issue_cat, issuance_info) = IssueCat::new(parent.coin_id())
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash,
                amount: 1,
                memos: vec![puzzle_hash.to_vec().into()],
            })?)
            .multi_issuance(&mut ctx, pk, 1)?;

        let cat_puzzle_hash = CurriedProgram {
            program: CAT_PUZZLE_HASH,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                tail_program_hash: issuance_info.asset_id,
                inner_puzzle: TreeHash::from(puzzle_hash),
            },
        }
        .tree_hash();

        let cat_info = CatInfo {
            asset_id: issuance_info.asset_id,
            p2_puzzle_hash: puzzle_hash,
            coin: Coin::new(issuance_info.eve_coin.coin_id(), cat_puzzle_hash.into(), 1),
            lineage_proof: issuance_info.lineage_proof,
        };

        StandardSpend::new()
            .chain(issue_cat)
            .finish(&mut ctx, parent, pk)?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == issuance_info.eve_coin.coin_id())
            .unwrap();

        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let parse_ctx = parse_puzzle(
            &mut allocator,
            puzzle,
            solution,
            coin_spend.coin,
            cat_info.coin,
        )?;

        let parse = parse_cat(&mut allocator, &parse_ctx, u64::MAX)?;
        assert_eq!(parse, Some(cat_info));

        Ok(())
    }
}
