use crate::{CatLayer, DriverError, HashedPtr, Layer, Puzzle, Spend, SpendContext};
use chia_consensus::make_aggsig_final_message::u64_to_bytes;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{
    cat::{CatArgs, CatSolution},
    CoinProof, LineageProof, Memos,
};
use chia_sdk_types::{Condition, Conditions};
use chia_sha2::Sha256;
use clvm_traits::FromClvm;
use clvm_utils::TreeHash;
use clvmr::{op_utils::u64_from_bytes, Allocator, NodePtr};

use crate::{StreamLayer, StreamPuzzleSolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingPuzzleInfo {
    pub recipient: Bytes32,
    pub clawback_ph: Option<Bytes32>,
    pub end_time: u64,
    pub last_payment_time: u64,
}

impl StreamingPuzzleInfo {
    pub fn new(
        recipient: Bytes32,
        clawback_ph: Option<Bytes32>,
        end_time: u64,
        last_payment_time: u64,
    ) -> Self {
        Self {
            recipient,
            clawback_ph,
            end_time,
            last_payment_time,
        }
    }

    pub fn amount_to_be_paid(&self, my_coin_amount: u64, payment_time: u64) -> u64 {
        // LAST_PAYMENT_TIME + (to_pay * (END_TIME - LAST_PAYMENT_TIME) / my_amount) = payment_time
        // to_pay = my_amount * (payment_time - LAST_PAYMENT_TIME) / (END_TIME - LAST_PAYMENT_TIME)
        my_coin_amount * (payment_time - self.last_payment_time)
            / (self.end_time - self.last_payment_time)
    }

    pub fn get_hint(recipient: Bytes32) -> Bytes32 {
        let mut s = Sha256::new();
        s.update(b"s");
        s.update(recipient.as_slice());
        s.finalize().into()
    }

    pub fn get_launch_hints(&self) -> Vec<Bytes> {
        let hint: Bytes = self.recipient.into();
        let clawback_ph: Bytes = if let Some(clawback_ph) = self.clawback_ph {
            clawback_ph.into()
        } else {
            Bytes::new(vec![])
        };
        let second_memo = u64_to_bytes(self.last_payment_time);
        let third_memo = u64_to_bytes(self.end_time);

        vec![hint, clawback_ph, second_memo.into(), third_memo.into()]
    }

    #[must_use]
    pub fn with_last_payment_time(self, last_payment_time: u64) -> Self {
        Self {
            last_payment_time,
            ..self
        }
    }

    pub fn parse(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(layer) = StreamLayer::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        Ok(Some(Self::from_layer(layer)))
    }

    pub fn into_layer(self) -> StreamLayer {
        StreamLayer::new(
            self.recipient,
            self.clawback_ph,
            self.end_time,
            self.last_payment_time,
        )
    }

    pub fn from_layer(layer: StreamLayer) -> Self {
        Self {
            recipient: layer.recipient,
            clawback_ph: layer.clawback_ph,
            end_time: layer.end_time,
            last_payment_time: layer.last_payment_time,
        }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        self.into_layer().puzzle_hash()
    }

    pub fn from_memos(memos: &[Bytes]) -> Result<Option<Self>, DriverError> {
        if memos.len() < 4 || memos.len() > 5 {
            return Ok(None);
        }

        let (recipient, clawback_ph, last_payment_time, end_time): (
            Bytes32,
            Option<Bytes32>,
            u64,
            u64,
        ) = if memos.len() == 4 {
            let Ok(recipient_b64): Result<Bytes32, _> = memos[0].clone().try_into() else {
                return Ok(None);
            };
            let clawback_ph_b64: Option<Bytes32> = if memos[1].is_empty() {
                None
            } else {
                let b32: Result<Bytes32, _> = memos[1].clone().try_into();
                if let Ok(b32) = b32 {
                    Some(b32)
                } else {
                    return Ok(None);
                }
            };
            (
                recipient_b64,
                clawback_ph_b64,
                u64_from_bytes(&memos[2]),
                u64_from_bytes(&memos[3]),
            )
        } else {
            let Ok(recipient_b64): Result<Bytes32, _> = memos[1].clone().try_into() else {
                return Ok(None);
            };
            let clawback_ph_b64: Option<Bytes32> = if memos[2].is_empty() {
                None
            } else {
                let b32: Result<Bytes32, _> = memos[2].clone().try_into();
                if let Ok(b32) = b32 {
                    Some(b32)
                } else {
                    return Ok(None);
                }
            };
            (
                recipient_b64,
                clawback_ph_b64,
                u64_from_bytes(&memos[3]),
                u64_from_bytes(&memos[4]),
            )
        };

        Ok(Some(Self::new(
            recipient,
            clawback_ph,
            end_time,
            last_payment_time,
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct StreamedAsset {
    pub coin: Coin,
    pub asset_id: Option<Bytes32>,
    pub proof: Option<LineageProof>,
    pub info: StreamingPuzzleInfo,
}

impl StreamedAsset {
    pub fn cat(
        coin: Coin,
        asset_id: Bytes32,
        proof: LineageProof,
        info: StreamingPuzzleInfo,
    ) -> Self {
        Self {
            coin,
            asset_id: Some(asset_id),
            proof: Some(proof),
            info,
        }
    }

    pub fn xch(coin: Coin, info: StreamingPuzzleInfo) -> Self {
        Self {
            coin,
            asset_id: None,
            proof: None,
            info,
        }
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_layer = self.info.into_layer();
        if let Some(asset_id) = self.asset_id {
            CatLayer::new(asset_id, inner_layer).construct_puzzle(ctx)
        } else {
            inner_layer.construct_puzzle(ctx)
        }
    }

    pub fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        payment_time: u64,
        clawback: bool,
    ) -> Result<NodePtr, DriverError> {
        let inner_layer = self.info.into_layer();
        let inner_solution = StreamPuzzleSolution {
            my_amount: self.coin.amount,
            payment_time,
            to_pay: self.info.amount_to_be_paid(self.coin.amount, payment_time),
            clawback,
        };

        if let Some(asset_id) = self.asset_id {
            CatLayer::new(asset_id, inner_layer).construct_solution(
                ctx,
                CatSolution {
                    inner_puzzle_solution: inner_solution,
                    lineage_proof: Some(self.proof.ok_or(DriverError::Custom(
                        "Missing lineage proof for CAT steam".to_string(),
                    ))?),
                    prev_coin_id: self.coin.coin_id(),
                    this_coin_info: self.coin,
                    next_coin_proof: CoinProof {
                        parent_coin_info: self.coin.parent_coin_info,
                        inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
                        amount: self.coin.amount,
                    },
                    prev_subtotal: 0,
                    extra_delta: 0,
                },
            )
        } else {
            inner_layer.construct_solution(ctx, inner_solution)
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        payment_time: u64,
        clawback: bool,
    ) -> Result<(), DriverError> {
        let puzzle = self.construct_puzzle(ctx)?;
        let solution = self.construct_solution(ctx, payment_time, clawback)?;

        ctx.spend(self.coin, Spend::new(puzzle, solution))
    }

    // if clawback, 3rd arg = last paid amount
    pub fn from_parent_spend(
        ctx: &mut SpendContext,
        coin_spend: &CoinSpend,
    ) -> Result<(Option<Self>, bool, u64), DriverError> {
        let parent_coin = coin_spend.coin;
        let parent_puzzle_ptr = ctx.alloc(&coin_spend.puzzle_reveal)?;
        let parent_puzzle = Puzzle::from_clvm(ctx, parent_puzzle_ptr)?;
        let parent_solution = ctx.alloc(&coin_spend.solution)?;

        if let Some((asset_id, proof, streaming_layer, streaming_solution)) =
            if let Ok(Some(layers)) = CatLayer::<StreamLayer>::parse_puzzle(ctx, parent_puzzle) {
                // parent is CAT streaming coin

                Some((
                    Some(layers.asset_id),
                    Some(LineageProof {
                        parent_parent_coin_info: parent_coin.parent_coin_info,
                        parent_inner_puzzle_hash: layers.inner_puzzle.puzzle_hash().into(),
                        parent_amount: parent_coin.amount,
                    }),
                    layers.inner_puzzle,
                    ctx.extract::<CatSolution<StreamPuzzleSolution>>(parent_solution)?
                        .inner_puzzle_solution,
                ))
            } else if let Ok(Some(layer)) = StreamLayer::parse_puzzle(ctx, parent_puzzle) {
                Some((
                    None,
                    None,
                    layer,
                    ctx.extract::<StreamPuzzleSolution>(parent_solution)?,
                ))
            } else {
                None
            }
        {
            if streaming_solution.clawback {
                return Ok((None, true, streaming_solution.to_pay));
            }

            let new_amount = parent_coin.amount - streaming_solution.to_pay;

            let new_inner_layer = StreamLayer::new(
                streaming_layer.recipient,
                streaming_layer.clawback_ph,
                streaming_layer.end_time,
                streaming_solution.payment_time,
            );
            let new_puzzle_hash = if let Some(asset_id) = asset_id {
                CatArgs::curry_tree_hash(asset_id, new_inner_layer.puzzle_hash())
            } else {
                new_inner_layer.puzzle_hash()
            };

            return Ok((
                Some(Self {
                    coin: Coin::new(parent_coin.coin_id(), new_puzzle_hash.into(), new_amount),
                    asset_id,
                    proof,
                    // last payment time should've been updated by the spend
                    info: StreamingPuzzleInfo::from_layer(streaming_layer)
                        .with_last_payment_time(streaming_solution.payment_time),
                }),
                false,
                0,
            ));
        }

        // if parent is not CAT/XCH streaming coin,
        // check if parent created eve streaming asset
        let parent_puzzle_ptr = parent_puzzle.ptr();
        let output = ctx.run(parent_puzzle_ptr, parent_solution)?;
        let conds = ctx.extract::<Conditions<NodePtr>>(output)?;

        let (asset_id, proof) = if let Ok(Some(parent_layer)) =
            CatLayer::<HashedPtr>::parse_puzzle(ctx, parent_puzzle)
        {
            (
                Some(parent_layer.asset_id),
                Some(LineageProof {
                    parent_parent_coin_info: parent_coin.parent_coin_info,
                    parent_inner_puzzle_hash: parent_layer.inner_puzzle.tree_hash().into(),
                    parent_amount: parent_coin.amount,
                }),
            )
        } else {
            (None, None)
        };

        for cond in conds {
            let Condition::CreateCoin(cc) = cond else {
                continue;
            };

            let Memos::Some(memos) = cc.memos else {
                continue;
            };

            let memos = ctx.extract::<Vec<Bytes>>(memos)?;
            let Some(candidate_info) = StreamingPuzzleInfo::from_memos(&memos)? else {
                continue;
            };
            let candidate_inner_puzzle_hash = candidate_info.inner_puzzle_hash();
            let candidate_puzzle_hash = if let Some(asset_id) = asset_id {
                CatArgs::curry_tree_hash(asset_id, candidate_inner_puzzle_hash)
            } else {
                candidate_inner_puzzle_hash
            };

            if cc.puzzle_hash != candidate_puzzle_hash.into() {
                continue;
            }

            return Ok((
                Some(Self {
                    coin: Coin::new(
                        parent_coin.coin_id(),
                        candidate_puzzle_hash.into(),
                        cc.amount,
                    ),
                    asset_id,
                    proof,
                    info: candidate_info,
                }),
                false,
                0,
            ));
        }

        Ok((None, false, 0))
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_protocol::Bytes;
    use chia_sdk_test::{Benchmark, Simulator};
    use clvm_utils::tree_hash;
    use clvmr::serde::node_from_bytes;
    use rstest::rstest;

    use crate::{
        Cat, CatSpend, FungibleAsset, SpendWithConditions, StandardLayer, STREAM_PUZZLE,
        STREAM_PUZZLE_HASH,
    };

    use super::*;

    #[test]
    fn test_puzzle_hash() {
        let mut allocator = Allocator::new();

        let ptr = node_from_bytes(&mut allocator, &STREAM_PUZZLE).unwrap();
        assert_eq!(tree_hash(&allocator, ptr), STREAM_PUZZLE_HASH);
    }

    #[rstest]
    fn test_streamed_asset(#[values(true, false)] xch_stream: bool) -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let mut sim = Simulator::new();
        let mut benchmark = Benchmark::new(format!(
            "Streamed {}",
            if xch_stream { "XCH" } else { "CAT" }
        ));

        let claim_intervals = [1000, 2000, 500, 1000, 10];
        let clawback_offset = 1234;
        let total_claim_time = claim_intervals.iter().sum::<u64>() + clawback_offset;

        // Create asset (XCH/CAT) & launch streaming coin
        let user_bls = sim.bls(0);
        let minter_bls = sim.bls(1000);

        let clawback_puzzle_ptr = ctx.alloc(&1)?;
        let clawback_ph = ctx.tree_hash(clawback_puzzle_ptr);
        let streaming_inner_puzzle = StreamLayer::new(
            user_bls.puzzle_hash,
            Some(clawback_ph.into()),
            total_claim_time + 1000,
            1000,
        );
        let streaming_inner_puzzle_hash: Bytes32 = streaming_inner_puzzle.puzzle_hash().into();

        let launch_hints =
            ctx.alloc(&StreamingPuzzleInfo::from_layer(streaming_inner_puzzle).get_launch_hints())?;
        let create_inner_spend = StandardLayer::new(minter_bls.pk).spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(
                streaming_inner_puzzle_hash,
                minter_bls.coin.amount,
                Memos::Some(launch_hints),
            ),
        )?;

        let (expected_coin, expected_asset_id, expected_lp) = if xch_stream {
            ctx.spend(minter_bls.coin, create_inner_spend)?;

            (
                minter_bls
                    .coin
                    .make_child(streaming_inner_puzzle_hash, minter_bls.coin.amount),
                None,
                None,
            )
        } else {
            let (issue_cat, cats) = Cat::issue_with_coin(
                &mut ctx,
                minter_bls.coin.coin_id(),
                minter_bls.coin.amount,
                Conditions::new().create_coin(
                    minter_bls.puzzle_hash,
                    minter_bls.coin.amount,
                    Memos::None,
                ),
            )?;
            StandardLayer::new(minter_bls.pk).spend(&mut ctx, minter_bls.coin, issue_cat)?;
            sim.spend_coins(ctx.take(), slice::from_ref(&minter_bls.sk))?;

            let cats = Cat::spend_all(&mut ctx, &[CatSpend::new(cats[0], create_inner_spend)])?;

            (
                cats[0].coin,
                Some(cats[0].info.asset_id),
                cats[0].lineage_proof,
            )
        };

        let spends = ctx.take();
        let launch_spend = spends.last().unwrap().clone();
        benchmark.add_spends(
            &mut ctx,
            &mut sim,
            spends,
            "create",
            slice::from_ref(&minter_bls.sk),
        )?;
        sim.set_next_timestamp(1000 + claim_intervals[0])?;

        // spend streaming CAT
        let mut streamed_asset = StreamedAsset::from_parent_spend(&mut ctx, &launch_spend)?
            .0
            .unwrap();
        assert_eq!(
            streamed_asset,
            StreamedAsset {
                coin: expected_coin,
                asset_id: expected_asset_id,
                proof: expected_lp,
                info: StreamingPuzzleInfo::new(
                    user_bls.puzzle_hash,
                    Some(clawback_ph.into()),
                    total_claim_time + 1000,
                    1000,
                ),
            },
        );

        let mut claim_time = sim.next_timestamp();
        for (i, _interval) in claim_intervals.iter().enumerate() {
            /* Payment is always based on last block's timestamp */
            if i < claim_intervals.len() - 1 {
                sim.pass_time(claim_intervals[i + 1]);
            }

            // to claim the payment, user needs to send a message to the streaming CAT
            let user_coin = sim.new_coin(user_bls.puzzle_hash, 0);
            let message_to_send: Bytes = Bytes::new(u64_to_bytes(claim_time));
            let coin_id_ptr = ctx.alloc(&streamed_asset.coin.coin_id())?;
            StandardLayer::new(user_bls.pk).spend(
                &mut ctx,
                user_coin,
                Conditions::new().send_message(23, message_to_send, vec![coin_id_ptr]),
            )?;

            streamed_asset.spend(&mut ctx, claim_time, false)?;

            let spends = ctx.take();
            let streamed_asset_spend = spends.last().unwrap().clone();
            benchmark.add_spends(
                &mut ctx,
                &mut sim,
                spends,
                "claim",
                slice::from_ref(&user_bls.sk),
            )?;

            // set up for next iteration
            if i < claim_intervals.len() - 1 {
                claim_time += claim_intervals[i + 1];
            }
            let (Some(new_streamed_asset), clawback, _) =
                StreamedAsset::from_parent_spend(&mut ctx, &streamed_asset_spend)?
            else {
                panic!("Failed to parse new streamed asset");
            };

            assert!(!clawback);
            streamed_asset = new_streamed_asset;
        }

        // Test clawback
        assert!(streamed_asset.coin.amount > 0);
        let clawback_msg_coin = sim.new_coin(clawback_ph.into(), 0);
        let claim_time = sim.next_timestamp() + 1;
        let message_to_send: Bytes = Bytes::new(u64_to_bytes(claim_time));
        let coin_id_ptr = ctx.alloc(&streamed_asset.coin.coin_id())?;
        let solution =
            ctx.alloc(&Conditions::new().send_message(23, message_to_send, vec![coin_id_ptr]))?;
        ctx.spend(clawback_msg_coin, Spend::new(clawback_puzzle_ptr, solution))?;

        streamed_asset.spend(&mut ctx, claim_time, true)?;

        let spends = ctx.take();
        let streamed_asset_spend = spends.last().unwrap().clone();
        benchmark.add_spends(
            &mut ctx,
            &mut sim,
            spends,
            "clawback",
            slice::from_ref(&user_bls.sk),
        )?;

        let (new_streamed_asset, clawback, _paid_amount_if_clawback) =
            StreamedAsset::from_parent_spend(&mut ctx, &streamed_asset_spend)?;

        assert!(clawback);
        assert!(new_streamed_asset.is_none());

        benchmark.print_summary(Some(&format!(
            "streamed-{}.costs",
            if xch_stream { "xch" } else { "cat" }
        )));

        Ok(())
    }
}
