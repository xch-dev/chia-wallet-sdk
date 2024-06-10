use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    cat::{CatArgs, CatSolution, CAT_PUZZLE_HASH},
    LineageProof,
};
use chia_sdk_types::{
    conditions::{puzzle_conditions, Condition, CreateCoin},
    puzzles::CatInfo,
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle};

#[derive(Debug, Clone, Copy)]
pub struct CatPuzzle {
    pub asset_id: Bytes32,
    pub inner_puzzle: Puzzle,
}

impl CatPuzzle {
    pub fn parse(allocator: &Allocator, puzzle: &Puzzle) -> Result<Option<Self>, ParseError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != CAT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = CatArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != CAT_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        Ok(Some(CatPuzzle {
            asset_id: args.asset_id,
            inner_puzzle: Puzzle::parse(allocator, args.inner_puzzle),
        }))
    }

    pub fn p2_outputs(
        &self,
        allocator: &mut Allocator,
        solution: NodePtr,
    ) -> Result<Vec<CreateCoin>, ParseError> {
        let solution = CatSolution::<NodePtr>::from_clvm(allocator, solution)?;

        let conditions = puzzle_conditions(
            allocator,
            self.inner_puzzle.ptr(),
            solution.inner_puzzle_solution,
        )?;

        let create_coins = conditions
            .into_iter()
            .filter_map(|condition| match condition {
                Condition::CreateCoin(create_coin) => Some(create_coin),
                _ => None,
            })
            .collect();

        Ok(create_coins)
    }

    pub fn child_coin_info(
        &self,
        allocator: &mut Allocator,
        parent_coin: Coin,
        child_coin: Coin,
        solution: NodePtr,
    ) -> Result<CatInfo, ParseError> {
        let create_coin = self
            .p2_outputs(allocator, solution)?
            .into_iter()
            .find(|create_coin| {
                let cat_puzzle_hash =
                    CatArgs::curry_tree_hash(self.asset_id, create_coin.puzzle_hash.into());

                cat_puzzle_hash == child_coin.puzzle_hash.into()
                    && create_coin.amount == child_coin.amount
            })
            .ok_or(ParseError::MissingChild)?;

        Ok(CatInfo {
            asset_id: self.asset_id,
            p2_puzzle_hash: create_coin.puzzle_hash,
            coin: child_coin,
            lineage_proof: LineageProof {
                parent_parent_coin_id: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: self.inner_puzzle.curried_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::PublicKey;
    use chia_protocol::Coin;
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_driver::{issue_cat_from_key, Conditions, SpendContext};
    use clvm_traits::ToNodePtr;

    use super::*;

    #[test]
    fn test_parse_cat() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let (issue_cat, issuance_info) = issue_cat_from_key(
            ctx,
            parent.coin_id(),
            pk,
            1,
            Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash),
        )?;

        let cat_puzzle_hash = CatArgs::curry_tree_hash(issuance_info.asset_id, puzzle_hash.into());

        let cat_info = CatInfo {
            asset_id: issuance_info.asset_id,
            p2_puzzle_hash: puzzle_hash,
            coin: Coin::new(issuance_info.eve_coin.coin_id(), cat_puzzle_hash.into(), 1),
            lineage_proof: issuance_info.lineage_proof,
        };

        ctx.spend_p2_coin(parent, pk, issue_cat)?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == issuance_info.eve_coin.coin_id())
            .unwrap();

        let puzzle_ptr = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution_ptr = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let puzzle = Puzzle::parse(&allocator, puzzle_ptr);
        let cat = CatPuzzle::parse(&allocator, &puzzle)?.expect("not a cat puzzle");
        let parsed_cat_info =
            cat.child_coin_info(&mut allocator, coin_spend.coin, cat_info.coin, solution_ptr)?;

        assert_eq!(parsed_cat_info, cat_info);

        Ok(())
    }
}
