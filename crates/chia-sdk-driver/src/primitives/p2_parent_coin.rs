use crate::{
    CatLayer, CatMaker, DriverError, HashedPtr, Layer, P2ParentLayer, Puzzle, Spend, SpendContext,
};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    cat::{CatArgs, CatSolution},
    CoinProof, LineageProof, Memos,
};
use chia_puzzles::CAT_PUZZLE_HASH;
use chia_sdk_types::{
    puzzles::{P2ParentArgs, P2ParentSolution},
    run_puzzle, Conditions,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[must_use]
pub struct P2ParentCoin {
    pub coin: Coin,
    pub asset_id: Option<Bytes32>,
    pub proof: LineageProof,
}

impl P2ParentCoin {
    pub fn new(coin: Coin, asset_id: Option<Bytes32>, proof: LineageProof) -> Self {
        Self {
            coin,
            asset_id,
            proof,
        }
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        if let Some(asset_id) = self.asset_id {
            CatLayer::new(asset_id, P2ParentLayer::cat(asset_id.tree_hash())).construct_puzzle(ctx)
        } else {
            P2ParentLayer::xch().construct_puzzle(ctx)
        }
    }

    pub fn inner_puzzle_hash(asset_id: Option<Bytes32>) -> TreeHash {
        P2ParentArgs {
            cat_maker: if let Some(asset_id) = asset_id {
                CatMaker::Default {
                    tail_hash_hash: asset_id.tree_hash(),
                }
                .curry_tree_hash()
            } else {
                CatMaker::Xch.curry_tree_hash()
            },
        }
        .tree_hash()
    }

    pub fn puzzle_hash(asset_id: Option<Bytes32>) -> TreeHash {
        let inner_puzzle_hash = Self::inner_puzzle_hash(asset_id);

        if let Some(asset_id) = asset_id {
            CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash)
        } else {
            inner_puzzle_hash
        }
    }

    pub fn construct_solution<CMS>(
        &self,
        ctx: &mut SpendContext,
        delegated_spend: Spend,
        cat_maker_solution: CMS,
    ) -> Result<NodePtr, DriverError>
    where
        CMS: ToClvm<Allocator>,
    {
        let inner_solution = P2ParentSolution {
            parent_parent_id: self.proof.parent_parent_coin_info,
            parent_amount: self.proof.parent_amount,
            parent_inner_puzzle: delegated_spend.puzzle,
            parent_solution: delegated_spend.solution,
            cat_maker_solution: cat_maker_solution.to_clvm(ctx)?,
        };

        if let Some(asset_id) = self.asset_id {
            let inner_layer = P2ParentLayer::cat(asset_id.tree_hash());

            CatLayer::new(asset_id, inner_layer).construct_solution(
                ctx,
                CatSolution {
                    inner_puzzle_solution: inner_solution,
                    lineage_proof: Some(self.proof),
                    prev_coin_id: self.coin.coin_id(),
                    this_coin_info: self.coin,
                    next_coin_proof: CoinProof {
                        parent_coin_info: self.coin.parent_coin_info,
                        inner_puzzle_hash: Self::inner_puzzle_hash(self.asset_id).into(),
                        amount: self.coin.amount,
                    },
                    prev_subtotal: 0,
                    extra_delta: 0,
                },
            )
        } else {
            P2ParentLayer::xch().construct_solution(ctx, inner_solution)
        }
    }

    pub fn spend<CMS>(
        &self,
        ctx: &mut SpendContext,
        delegated_spend: Spend,
        cat_maker_solution: CMS,
    ) -> Result<(), DriverError>
    where
        CMS: ToClvm<Allocator>,
    {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, delegated_spend, cat_maker_solution)?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))
    }

    // also returns memo
    pub fn parse_child(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<(Self, Memos)>, DriverError> {
        let (parent_inner_puzzle_hash, asset_id) =
            if parent_puzzle.mod_hash() == CAT_PUZZLE_HASH.into() {
                let Some(parent_puzzle) = parent_puzzle.as_curried() else {
                    return Err(DriverError::Custom(
                        "Expected parent puzzle to be curried but it's not.".to_string(),
                    ));
                };

                let args = CatArgs::<HashedPtr>::from_clvm(allocator, parent_puzzle.args)?;
                (args.inner_puzzle.tree_hash().into(), Some(args.asset_id))
            } else {
                (parent_coin.puzzle_hash, None)
            };

        let proof = LineageProof {
            parent_parent_coin_info: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash,
            parent_amount: parent_coin.amount,
        };

        let expected_puzzle_hash: Bytes32 = Self::puzzle_hash(asset_id).into();

        let parent_output = run_puzzle(allocator, parent_puzzle.ptr(), parent_solution)?;
        let parent_conditions = Conditions::<NodePtr>::from_clvm(allocator, parent_output)?;
        let Some(create_coin) = parent_conditions.iter().find_map(|c| {
            c.as_create_coin()
                .filter(|&create_coin| create_coin.puzzle_hash == expected_puzzle_hash)
        }) else {
            return Ok(None);
        };

        Ok(Some((
            Self {
                coin: Coin::new(
                    parent_coin.coin_id(),
                    expected_puzzle_hash,
                    create_coin.amount,
                ),
                asset_id,
                proof,
            },
            create_coin.memos,
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_protocol::Bytes;
    use chia_sdk_test::{Benchmark, Simulator};
    use chia_sdk_types::puzzles::{P2_PARENT_PUZZLE, P2_PARENT_PUZZLE_HASH};
    use clvm_utils::tree_hash;
    use clvmr::serde::node_from_bytes;
    use rstest::rstest;

    use crate::{Cat, CatSpend, FungibleAsset, SpendWithConditions, StandardLayer};

    use super::*;

    #[test]
    fn test_puzzle_hash() {
        let mut allocator = Allocator::new();

        let ptr = node_from_bytes(&mut allocator, &P2_PARENT_PUZZLE).unwrap();
        assert_eq!(tree_hash(&allocator, ptr), P2_PARENT_PUZZLE_HASH);
    }

    #[rstest]
    #[case::xch(false)]
    #[case::cat(true)]
    fn test_p2_parent(#[case] cat_mode: bool) -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let mut sim = Simulator::new();
        let mut benchmark = Benchmark::new(format!(
            "P2 Parent Coin ({})",
            if cat_mode { "CAT" } else { "XCH" }
        ));

        let parent_bls = sim.bls(1337);

        // Server coins will be created with a list of strings as memos
        let server_list = vec![
            Bytes::new(b"yak1".to_vec()),
            Bytes::new(b"yak2".to_vec()),
            Bytes::new(b"yak3".to_vec()),
        ];

        let (expected_coin, expected_asset_id, expected_lp) = if cat_mode {
            let (issue_cat, cats) = Cat::issue_with_coin(
                &mut ctx,
                parent_bls.coin.coin_id(),
                parent_bls.coin.amount,
                Conditions::new().create_coin(
                    parent_bls.puzzle_hash,
                    parent_bls.coin.amount,
                    Memos::None,
                ),
            )?;
            StandardLayer::new(parent_bls.pk).spend(&mut ctx, parent_bls.coin, issue_cat)?;
            sim.spend_coins(ctx.take(), slice::from_ref(&parent_bls.sk))?;

            let parent_conds = Conditions::new().create_coin(
                P2ParentCoin::puzzle_hash(Some(cats[0].info.asset_id)).into(),
                1337,
                ctx.memos(&server_list)?,
            );
            let parent_cat_inner_spend =
                StandardLayer::new(parent_bls.pk).spend_with_conditions(&mut ctx, parent_conds)?;

            let cats = Cat::spend_all(&mut ctx, &[CatSpend::new(cats[0], parent_cat_inner_spend)])?;

            (
                cats[0].coin,
                Some(cats[0].info.asset_id),
                cats[0].lineage_proof.unwrap(),
            )
        } else {
            let parent_conds = Conditions::new().create_coin(
                P2ParentCoin::puzzle_hash(None).into(),
                1337,
                ctx.memos(&server_list)?,
            );
            let parent_inner_spend =
                StandardLayer::new(parent_bls.pk).spend_with_conditions(&mut ctx, parent_conds)?;

            ctx.spend(parent_bls.coin, parent_inner_spend)?;

            (
                parent_bls.coin.make_child(
                    P2ParentCoin::puzzle_hash(None).into(),
                    parent_bls.coin.amount,
                ),
                None,
                LineageProof {
                    parent_parent_coin_info: parent_bls.coin.parent_coin_info,
                    parent_inner_puzzle_hash: parent_bls.coin.puzzle_hash,
                    parent_amount: parent_bls.coin.amount,
                },
            )
        };

        let spends = ctx.take();
        let launch_spend = spends.last().unwrap().clone();
        benchmark.add_spends(
            &mut ctx,
            &mut sim,
            spends,
            "create",
            slice::from_ref(&parent_bls.sk),
        )?;

        // Test parsing
        let parent_puzzle = ctx.alloc(&launch_spend.puzzle_reveal)?;
        let parent_puzzle = Puzzle::parse(&ctx, parent_puzzle);
        let parent_solution = ctx.alloc(&launch_spend.solution)?;
        let (p2_parent_coin, memos) =
            P2ParentCoin::parse_child(&mut ctx, launch_spend.coin, parent_puzzle, parent_solution)?
                .unwrap();

        assert_eq!(
            p2_parent_coin,
            P2ParentCoin {
                coin: expected_coin,
                asset_id: expected_asset_id,
                proof: expected_lp,
            },
        );
        let Memos::Some(memos) = memos else {
            panic!("Expected memos");
        };
        let memos = ctx.extract::<Vec<Bytes>>(memos)?;
        assert_eq!(memos, server_list);

        // Spend the p2_parent coin
        let new_coin_inner_puzzle_hash = Bytes32::new([0; 32]);
        let new_coin = Coin::new(
            p2_parent_coin.coin.coin_id(),
            if cat_mode {
                CatArgs::curry_tree_hash(
                    p2_parent_coin.asset_id.unwrap(),
                    new_coin_inner_puzzle_hash.into(),
                )
                .into()
            } else {
                new_coin_inner_puzzle_hash
            },
            p2_parent_coin.coin.amount,
        );

        let delegated_spend = StandardLayer::new(parent_bls.pk).spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(new_coin_inner_puzzle_hash, new_coin.amount, Memos::None),
        )?;
        p2_parent_coin.spend(&mut ctx, delegated_spend, ())?;

        let spends = ctx.take();
        benchmark.add_spends(
            &mut ctx,
            &mut sim,
            spends,
            "spend",
            slice::from_ref(&parent_bls.sk),
        )?;

        assert!(sim.coin_state(new_coin.coin_id()).is_some());

        benchmark.print_summary(Some(&format!(
            "p2-parent-coin-{}.costs",
            if cat_mode { "cat" } else { "xch" }
        )));

        Ok(())
    }
}
