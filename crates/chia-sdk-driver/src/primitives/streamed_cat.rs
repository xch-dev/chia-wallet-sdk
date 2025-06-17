use crate::{CatLayer, DriverError, Layer, Puzzle, Spend, SpendContext};
use chia_consensus::make_aggsig_final_message::u64_to_bytes;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::{
    cat::{CatArgs, CatSolution},
    CoinProof, LineageProof, Memos,
};
use chia_sdk_types::{run_puzzle, Condition, Conditions};
use chia_sha2::Sha256;
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, TreeHash};
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

#[derive(Debug, Clone)]
#[must_use]
pub struct StreamedCat {
    pub coin: Coin,
    pub asset_id: Bytes32,
    pub proof: LineageProof,
    pub info: StreamingPuzzleInfo,
}

impl StreamedCat {
    pub fn new(
        coin: Coin,
        asset_id: Bytes32,
        proof: LineageProof,
        info: StreamingPuzzleInfo,
    ) -> Self {
        Self {
            coin,
            asset_id,
            proof,
            info,
        }
    }

    pub fn layers(&self) -> CatLayer<StreamLayer> {
        CatLayer::<StreamLayer>::new(self.asset_id, self.info.into_layer())
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        self.layers().construct_puzzle(ctx)
    }

    pub fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        payment_time: u64,
        clawback: bool,
    ) -> Result<NodePtr, DriverError> {
        self.layers().construct_solution(
            ctx,
            CatSolution {
                inner_puzzle_solution: StreamPuzzleSolution {
                    my_amount: self.coin.amount,
                    payment_time,
                    to_pay: self.info.amount_to_be_paid(self.coin.amount, payment_time),
                    clawback,
                },
                lineage_proof: Some(self.proof),
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

    // if clawback, 3rd arg = las
    pub fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<(Option<Self>, bool, u64), DriverError> {
        let Some(layers) = CatLayer::<StreamLayer>::parse_puzzle(allocator, parent_puzzle)? else {
            // check if parent created streaming CAT
            let parent_puzzle_ptr = parent_puzzle.ptr();
            let output = run_puzzle(allocator, parent_puzzle_ptr, parent_solution)?;
            let conds: Conditions<NodePtr> = Conditions::from_clvm(allocator, output)?;

            let Some(parent_layer) = CatLayer::<NodePtr>::parse_puzzle(allocator, parent_puzzle)?
            else {
                return Ok((None, false, 0));
            };

            let mut found_stream_layer: Option<Self> = None;
            for cond in conds {
                let Condition::CreateCoin(cc) = cond else {
                    continue;
                };

                let Memos::Some(memos) = cc.memos else {
                    continue;
                };

                let memos = Vec::<Bytes>::from_clvm(allocator, memos)?;
                let Some(candidate_info) = StreamingPuzzleInfo::from_memos(&memos)? else {
                    continue;
                };
                let candidate_inner_puzzle_hash = candidate_info.inner_puzzle_hash();
                let candidate_puzzle_hash =
                    CatArgs::curry_tree_hash(parent_layer.asset_id, candidate_inner_puzzle_hash);

                if cc.puzzle_hash != candidate_puzzle_hash.into() {
                    continue;
                }

                found_stream_layer = Some(Self::new(
                    Coin::new(
                        parent_coin.coin_id(),
                        candidate_puzzle_hash.into(),
                        cc.amount,
                    ),
                    parent_layer.asset_id,
                    LineageProof {
                        parent_parent_coin_info: parent_coin.parent_coin_info,
                        parent_inner_puzzle_hash: tree_hash(allocator, parent_layer.inner_puzzle)
                            .into(),
                        parent_amount: parent_coin.amount,
                    },
                    candidate_info,
                ));
            }

            return Ok((found_stream_layer, false, 0));
        };

        let proof = LineageProof {
            parent_parent_coin_info: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: layers.inner_puzzle.puzzle_hash().into(),
            parent_amount: parent_coin.amount,
        };

        let parent_solution =
            CatSolution::<StreamPuzzleSolution>::from_clvm(allocator, parent_solution)?;
        if parent_solution.inner_puzzle_solution.clawback {
            return Ok((None, true, parent_solution.inner_puzzle_solution.to_pay));
        }

        let new_amount = parent_coin.amount - parent_solution.inner_puzzle_solution.to_pay;

        let new_inner_layer = StreamLayer::new(
            layers.inner_puzzle.recipient,
            layers.inner_puzzle.clawback_ph,
            layers.inner_puzzle.end_time,
            parent_solution.inner_puzzle_solution.payment_time,
        );
        let new_puzzle_hash =
            CatArgs::curry_tree_hash(layers.asset_id, new_inner_layer.puzzle_hash());

        Ok((
            Some(Self::new(
                Coin::new(parent_coin.coin_id(), new_puzzle_hash.into(), new_amount),
                layers.asset_id,
                proof,
                // last payment time should've been updated by the spend
                StreamingPuzzleInfo::from_layer(layers.inner_puzzle)
                    .with_last_payment_time(parent_solution.inner_puzzle_solution.payment_time),
            )),
            false,
            0,
        ))
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes;
    use chia_sdk_test::{BlsPair, Simulator};
    use clvm_utils::tree_hash;
    use clvmr::serde::node_from_bytes;

    use crate::{Cat, StandardLayer, STREAM_PUZZLE, STREAM_PUZZLE_HASH};

    use super::*;

    #[test]
    fn test_puzzle_hash() {
        let mut allocator = Allocator::new();

        let ptr = node_from_bytes(&mut allocator, &STREAM_PUZZLE).unwrap();
        assert_eq!(tree_hash(&allocator, ptr), STREAM_PUZZLE_HASH);
    }

    #[test]
    fn test_streamed_cat() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let mut sim = Simulator::new();

        let claim_intervals = [1000, 2000, 500, 1000, 10];
        let clawback_offset = 1234;
        let total_claim_time = claim_intervals.iter().sum::<u64>() + clawback_offset;

        // Create CAT & launch vesting one
        let user_key = BlsPair::new(0);
        let user_p2 = StandardLayer::new(user_key.pk);
        let user_puzzle_hash: Bytes32 = user_key.puzzle_hash;

        let payment_cat_amount = 1000;
        let minter_key = BlsPair::new(1);
        let minter_coin = sim.new_coin(minter_key.puzzle_hash, payment_cat_amount);
        let minter_p2 = StandardLayer::new(minter_key.pk);

        let clawback_puzzle_ptr = ctx.alloc(&1)?;
        let clawback_ph = ctx.tree_hash(clawback_puzzle_ptr);
        let streaming_inner_puzzle = StreamLayer::new(
            user_puzzle_hash,
            Some(clawback_ph.into()),
            total_claim_time + 1000,
            1000,
        );
        let streaming_inner_puzzle_hash: Bytes32 = streaming_inner_puzzle.puzzle_hash().into();
        let (issue_cat, cats) = Cat::issue_with_coin(
            &mut ctx,
            minter_coin.coin_id(),
            payment_cat_amount,
            Conditions::new().create_coin(
                streaming_inner_puzzle_hash,
                payment_cat_amount,
                Memos::None,
            ),
        )?;
        minter_p2.spend(&mut ctx, minter_coin, issue_cat)?;

        let initial_vesting_cat = cats[0];
        sim.spend_coins(ctx.take(), &[minter_key.sk.clone()])?;
        sim.set_next_timestamp(1000 + claim_intervals[0])?;

        // spend streaming CAT
        let mut streamed_cat = StreamedCat::new(
            initial_vesting_cat.coin,
            initial_vesting_cat.info.asset_id,
            initial_vesting_cat.lineage_proof.unwrap(),
            StreamingPuzzleInfo::new(
                user_puzzle_hash,
                Some(clawback_ph.into()),
                total_claim_time + 1000,
                1000,
            ),
        );

        let mut claim_time = sim.next_timestamp();
        for (i, _interval) in claim_intervals.iter().enumerate() {
            /* Payment is always based on last block's timestamp */
            if i < claim_intervals.len() - 1 {
                sim.pass_time(claim_intervals[i + 1]);
            }

            // to claim the payment, user needs to send a message to the streaming CAT
            let user_coin = sim.new_coin(user_puzzle_hash, 0);
            let message_to_send: Bytes = Bytes::new(u64_to_bytes(claim_time));
            let coin_id_ptr = ctx.alloc(&streamed_cat.coin.coin_id())?;
            user_p2.spend(
                &mut ctx,
                user_coin,
                Conditions::new().send_message(23, message_to_send, vec![coin_id_ptr]),
            )?;

            streamed_cat.spend(&mut ctx, claim_time, false)?;

            let spends = ctx.take();
            let streamed_cat_spend = spends.last().unwrap().clone();
            sim.spend_coins(spends, &[user_key.sk.clone()])?;

            // set up for next iteration
            if i < claim_intervals.len() - 1 {
                claim_time += claim_intervals[i + 1];
            }
            let parent_puzzle = ctx.alloc(&streamed_cat_spend.puzzle_reveal)?;
            let parent_puzzle = Puzzle::from_clvm(&ctx, parent_puzzle)?;
            let parent_solution = ctx.alloc(&streamed_cat_spend.solution)?;
            let (Some(new_streamed_cat), clawback, _) = StreamedCat::from_parent_spend(
                &mut ctx,
                streamed_cat.coin,
                parent_puzzle,
                parent_solution,
            )?
            else {
                panic!("Failed to parse new streamed cat");
            };

            assert!(!clawback);
            streamed_cat = new_streamed_cat;
        }

        // Test clawback
        assert!(streamed_cat.coin.amount > 0);
        let clawback_msg_coin = sim.new_coin(clawback_ph.into(), 0);
        let claim_time = sim.next_timestamp() + 1;
        let message_to_send: Bytes = Bytes::new(u64_to_bytes(claim_time));
        let coin_id_ptr = ctx.alloc(&streamed_cat.coin.coin_id())?;
        let solution =
            ctx.alloc(&Conditions::new().send_message(23, message_to_send, vec![coin_id_ptr]))?;
        ctx.spend(clawback_msg_coin, Spend::new(clawback_puzzle_ptr, solution))?;

        streamed_cat.spend(&mut ctx, claim_time, true)?;

        let spends = ctx.take();
        let streamed_cat_spend = spends.last().unwrap().clone();
        sim.spend_coins(spends, &[user_key.sk.clone()])?;

        let parent_puzzle = ctx.alloc(&streamed_cat_spend.puzzle_reveal)?;
        let parent_puzzle = Puzzle::from_clvm(&ctx, parent_puzzle)?;
        let parent_solution = ctx.alloc(&streamed_cat_spend.solution)?;
        let (new_streamed_cat, clawback, _paid_amount_if_clawback) =
            StreamedCat::from_parent_spend(
                &mut ctx,
                streamed_cat.coin,
                parent_puzzle,
                parent_solution,
            )?;

        assert!(clawback);
        assert!(new_streamed_cat.is_none());

        Ok(())
    }
}
