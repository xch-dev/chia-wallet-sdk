use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    cat::{CatArgs, CatSolution, CAT_PUZZLE_HASH},
    LineageProof,
};
use chia_sdk_types::conditions::{puzzle_conditions, Condition, CreateCoin};
use clvm_traits::{FromClvm, ToClvm, ToNodePtr};
use clvm_utils::{tree_hash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{FromPuzzle, FromSpend, ParseError, Puzzle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CAT<P = NodePtr> {
    pub coin: Coin,
    pub asset_id: Bytes32,

    pub inner_puzzle_hash: TreeHash,
    pub inner_puzzle: Option<P>,

    pub lineage_proof: Option<LineageProof>,
}

impl<P> CAT<P> {
    pub fn with_inner_puzzle(mut self, inner_puzzle: P) -> Self {
        self.inner_puzzle = Some(inner_puzzle);
        self
    }

    pub fn with_lineage_proof(mut self, lineage_proof: LineageProof) -> Self {
        self.lineage_proof = Some(lineage_proof);
        self
    }

    pub fn p2_outputs(
        &self,
        allocator: &mut Allocator,
        solution: NodePtr,
    ) -> Result<Vec<CreateCoin>, ParseError>
    where
        P: ToClvm<NodePtr>,
    {
        let solution = CatSolution::<NodePtr>::from_clvm(allocator, solution)?;

        let inner_puzzle = self
            .inner_puzzle
            .to_clvm(allocator)
            .map_err(|err| ParseError::ToClvm(err))?;

        let conditions =
            puzzle_conditions(allocator, inner_puzzle, solution.inner_puzzle_solution)?;

        let create_coins = conditions
            .into_iter()
            .filter_map(|condition| match condition {
                Condition::CreateCoin(create_coin) => Some(create_coin),
                _ => None,
            })
            .collect();

        Ok(create_coins)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CATFromPuzzleInfo {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
}

impl<P> FromPuzzle<CATFromPuzzleInfo> for CAT<P>
where
    P: FromClvm<NodePtr>,
{
    fn from_puzzle(
        allocator: &mut Allocator,
        puzzle: NodePtr,
        info: CATFromPuzzleInfo,
    ) -> Result<CAT<P>, ParseError> {
        let puzzle = Puzzle::parse(allocator, puzzle);

        let Some(puzzle) = puzzle.as_curried() else {
            return Err(ParseError::LayerMismatch);
        };

        if puzzle.mod_hash != CAT_PUZZLE_HASH {
            return Err(ParseError::LayerMismatch);
        }

        let args = CatArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.mod_hash != CAT_PUZZLE_HASH.into() {
            return Err(ParseError::InvalidModHash);
        }

        let inner_puzzle_hash = tree_hash(&allocator, args.inner_puzzle);
        let inner_puzzle = P::from_clvm(allocator, args.inner_puzzle)?;

        Ok(CAT {
            coin: info.coin,
            asset_id: args.asset_id,

            inner_puzzle_hash,
            inner_puzzle: Some(inner_puzzle),

            lineage_proof: info.lineage_proof,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CATFromSpendInfo {
    pub child_coin: Coin,
}

impl<P> FromSpend<CATFromSpendInfo> for CAT<P> {
    fn from_spend(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
        solution: NodePtr,
        info: CATFromSpendInfo,
    ) -> Result<CAT<P>, ParseError> {
        let parent_cat = CAT::<NodePtr>::from_puzzle(
            allocator,
            puzzle,
            CATFromPuzzleInfo {
                coin,
                lineage_proof: None,
            },
        )?;

        let create_coin = parent_cat
            .p2_outputs(allocator, solution)?
            .into_iter()
            .find(|create_coin| {
                let cat_puzzle_hash =
                    CatArgs::curry_tree_hash(parent_cat.asset_id, create_coin.puzzle_hash.into());

                cat_puzzle_hash == info.child_coin.puzzle_hash.into()
                    && create_coin.amount == info.child_coin.amount
            })
            .ok_or(ParseError::MissingChild)?;

        Ok(CAT {
            coin: info.child_coin,
            asset_id: parent_cat.asset_id,

            inner_puzzle_hash: create_coin.puzzle_hash.into(),
            inner_puzzle: None,

            lineage_proof: Some(LineageProof {
                parent_parent_coin_id: coin.parent_coin_info,
                parent_inner_puzzle_hash: parent_cat.inner_puzzle_hash.into(),
                parent_amount: coin.amount,
            }),
        })
    }

    fn from_coin_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
        additional_info: CATFromSpendInfo,
    ) -> Result<CAT<P>, ParseError> {
        let puzzle_ptr = cs.puzzle_reveal.to_node_ptr(allocator)?;
        let solution_ptr = cs.solution.to_node_ptr(allocator)?;

        CAT::from_spend(
            allocator,
            cs.coin,
            puzzle_ptr,
            solution_ptr,
            additional_info,
        )
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::PublicKey;
    use chia_protocol::Coin;
    use chia_puzzles::standard::StandardArgs;

    use crate::{issue_cat_from_key, Conditions, SpendContext};

    use super::*;

    #[test]
    fn test_parse_cat() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let pk = PublicKey::default();
        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let conditions = Conditions::new().create_hinted_coin(puzzle_hash, 1, puzzle_hash);
        let (issue_cat, issuance_info) =
            issue_cat_from_key(&mut ctx, parent.coin_id(), pk, 1, conditions)?;

        let cat_puzzle_hash = CatArgs::curry_tree_hash(issuance_info.asset_id, puzzle_hash.into());

        let cat = CAT::<()> {
            coin: Coin::new(issuance_info.eve_coin.coin_id(), cat_puzzle_hash.into(), 1),
            asset_id: issuance_info.asset_id,

            inner_puzzle_hash: puzzle_hash.into(),
            inner_puzzle: None,

            lineage_proof: Some(issuance_info.lineage_proof),
        };

        ctx.spend_p2_coin(parent, pk, issue_cat)?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == issuance_info.eve_coin.coin_id())
            .unwrap();

        let mut allocator = ctx.into();

        let parsed_cat_info = CAT::from_coin_spend(
            &mut allocator,
            &coin_spend,
            CATFromSpendInfo {
                child_coin: cat.coin,
            },
        )
        .expect("CAT::from_coin_spend err");

        assert_eq!(parsed_cat_info, cat);

        Ok(())
    }
}
