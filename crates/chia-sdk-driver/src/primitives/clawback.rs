use crate::{DriverError, Layer, P2OneOfManyLayer, Puzzle, Spend, SpendContext};
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{
    puzzles::{
        AugmentedConditionArgs, AugmentedConditionSolution, P2CurriedArgs, P2CurriedSolution,
        P2OneOfManySolution, AUGMENTED_CONDITION_PUZZLE_HASH, P2_CURRIED_PUZZLE_HASH,
    },
    Condition, MerkleTree,
};
use chia_streamable_macro::streamable;
use chia_traits::Streamable;
use clvm_traits::FromClvm;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use std::num::NonZeroU64;

#[streamable]
pub struct VersionedBlob {
    version: u16,
    blob: Bytes,
}

#[streamable]
#[derive(Copy)]
// this struct is an unfortunate hack in order to get the streamable bytes which are used on chain for recreating ourself
pub struct ClawbackMetadata {
    timelock: u64,
    sender_puzzle_hash: Bytes32,
    recipient_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clawback {
    /// The number of seconds until this clawback can be claimed by the recipient.
    pub timelock: NonZeroU64,
    /// The original sender of the coin, who can claw it back until claimed.
    pub sender_puzzle_hash: Bytes32,
    /// The intended recipient who can claim after the timelock period is up.
    pub recipient_puzzle_hash: Bytes32,
}

impl Clawback {
    pub fn parse_children(
        allocator: &mut Allocator,
        parent_puzzle: Puzzle, // this could be any puzzle type
        parent_solution: NodePtr,
    ) -> Result<Option<Vec<Self>>, DriverError>
    where
        Self: Sized,
    {
        let output = run_puzzle(allocator, parent_puzzle.ptr(), parent_solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;
        println!("DEBUG 1 - conditions: {:?}", conditions);
        let mut outputs = Vec::<Clawback>::new();
        let mut metadatas = Vec::<ClawbackMetadata>::new();
        let mut puzhashes = Vec::<[u8; 32]>::with_capacity(conditions.len());
        for condition in conditions {
            match condition {
                Condition::CreateCoin(cc) => puzhashes.push(cc.puzzle_hash.into()),
                Condition::Remark(rm) => match allocator.sexp(rm.rest) {
                    clvmr::SExp::Atom => {}
                    clvmr::SExp::Pair(first, _rest) => {
                        metadatas.push(
                            ClawbackMetadata::from_bytes_unchecked(
                                VersionedBlob::from_bytes_unchecked(&allocator.atom(first))
                                    .map_err(|_| DriverError::InvalidMemo)?
                                    .blob
                                    .as_ref(),
                            )
                            .map_err(|_| DriverError::InvalidMemo)?,
                        );
                        println!("DEBUG 2 - metadatas: {:?}", metadatas);
                    }
                },
                _ => {}
            }
            for metadata in &metadatas {
                let clawback = Clawback {
                    timelock: metadata.timelock.try_into()?,
                    sender_puzzle_hash: metadata.sender_puzzle_hash,
                    recipient_puzzle_hash: metadata.recipient_puzzle_hash,
                };
                if puzhashes.contains(&clawback.to_layer().tree_hash().to_bytes()) {
                    outputs.push(clawback)
                }
            }
        }
        println!("DEBUG 3 - outputs: {:?}", outputs);
        Ok(Some(outputs))
    }

    pub fn claim_path_puzzle_hash(&self) -> TreeHash {
        CurriedProgram {
            program: AUGMENTED_CONDITION_PUZZLE_HASH,
            args: AugmentedConditionArgs::new(
                Condition::<TreeHash>::assert_seconds_relative(self.timelock.into()),
                TreeHash::from(self.recipient_puzzle_hash),
            ),
        }
        .tree_hash()
    }

    pub fn claim_path_puzzle(
        &self,
        ctx: &mut SpendContext,
        inner_puzzle: NodePtr,
    ) -> Result<NodePtr, DriverError> {
        ctx.curry(AugmentedConditionArgs::new(
            Condition::<NodePtr>::assert_seconds_relative(self.timelock.into()),
            inner_puzzle,
        ))
    }

    pub fn clawback_path_puzzle_hash(&self) -> TreeHash {
        CurriedProgram {
            program: P2_CURRIED_PUZZLE_HASH,
            args: P2CurriedArgs::new(self.sender_puzzle_hash),
        }
        .tree_hash()
    }

    pub fn clawback_path_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2CurriedArgs::new(self.sender_puzzle_hash))
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[
            self.claim_path_puzzle_hash().into(),
            self.clawback_path_puzzle_hash().into(),
        ])
    }

    pub fn to_layer(&self) -> P2OneOfManyLayer {
        P2OneOfManyLayer::new(self.merkle_tree().root())
    }

    // this function returns the Remark condition required to hint at this clawback
    // it should be included alongside the createcoin that creates this
    pub fn get_remark_condition(&self, allocator: &mut Allocator) -> Result<Condition, DriverError> {
        let first = allocator.new_small_number(2)?; // magic number for clawback bytes dump
        let cbm = ClawbackMetadata {
            timelock: self.timelock.into(),
            sender_puzzle_hash: self.sender_puzzle_hash,
            recipient_puzzle_hash: self.recipient_puzzle_hash,
        };
        let vb = VersionedBlob {
            version: 1,
            blob: cbm.to_bytes().map_err(|_| DriverError::InvalidMemo)?.into(),
        };
        let bytes = allocator.new_atom(
            vb.to_bytes()
                .map_err(|_| DriverError::InvalidMemo)?
                .as_ref(),
        )?;
        let rest = allocator.new_pair(bytes, allocator.nil())?;
        let node_ptr = allocator.new_pair(first, rest)?;

        return Ok(Condition::remark(node_ptr));
    }

    pub fn claim_spend(&self, ctx: &mut SpendContext, spend: Spend) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle = self.claim_path_puzzle(ctx, spend.puzzle)?;
        let solution = ctx.alloc(&AugmentedConditionSolution::new(spend.solution))?;

        let proof = merkle_tree
            .proof(ctx.tree_hash(puzzle).into())
            .ok_or(DriverError::InvalidMerkleProof)?;

        P2OneOfManyLayer::new(merkle_tree.root())
            .construct_spend(ctx, P2OneOfManySolution::new(proof, puzzle, solution))
    }

    pub fn clawback_spend(
        &self,
        ctx: &mut SpendContext,
        spend: Spend,
    ) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle = self.clawback_path_puzzle(ctx)?;
        let solution = ctx.alloc(&P2CurriedSolution::new(spend.puzzle, spend.solution))?;

        let proof = merkle_tree
            .proof(ctx.tree_hash(puzzle).into())
            .ok_or(DriverError::InvalidMerkleProof)?;

        P2OneOfManyLayer::new(merkle_tree.root())
            .construct_spend(ctx, P2OneOfManySolution::new(proof, puzzle, solution))
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_traits::ToClvm;

    use crate::{SpendWithConditions, StandardLayer};

    use super::*;

    #[test]
    #[allow(clippy::similar_names)]
    fn test_clawback_coin_claim() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let bob = sim.bls(1);
        let bob_p2 = StandardLayer::new(bob.pk);

        let clawback = Clawback {
            timelock: NonZeroU64::MIN,
            sender_puzzle_hash: alice.puzzle_hash,
            recipient_puzzle_hash: bob.puzzle_hash,
        };
        let clawback_puzzle_hash = clawback.to_layer().tree_hash().into();
        let coin = alice.coin;
        let mut allocator = Allocator::new();
        let conditions = Conditions::new().create_coin(clawback_puzzle_hash, 1, None);
        let conditions = conditions.with(clawback.get_remark_condition(&mut allocator)?);
        println!("DEBUG 0 -  CONDITION INTENTIONS: {:?}", conditions);
        alice_p2.spend(ctx, coin, conditions)?;
        
        let cs = ctx.take();

        let clawback_coin = Coin::new(coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(cs, &[alice.sk])?;

        let puzzle_reveal = sim
            .puzzle_reveal(coin.coin_id())
            .expect("missing puzzle")
            .to_clvm(&mut allocator)?;

        let solution = sim
            .solution(coin.coin_id())
            .expect("missing solution")
            .to_clvm(&mut allocator)?;

        let puzzle = Puzzle::parse(&allocator, puzzle_reveal);

        // check we can recreate Clawback from the spend
        let children = Clawback::parse_children(&mut allocator, puzzle, solution)
            .expect("we should have found the child")
            .expect("we should have found children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], clawback);

        let bob_inner = bob_p2.spend_with_conditions(ctx, Conditions::new().reserve_fee(1))?;
        let claim_spend = clawback.claim_spend(ctx, bob_inner)?;
        ctx.spend(clawback_coin, claim_spend)?;

        sim.spend_coins(ctx.take(), &[bob.sk])?;

        Ok(())
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_clawback_coin_clawback() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let clawback = Clawback {
            timelock: NonZeroU64::MAX,
            sender_puzzle_hash: alice.puzzle_hash,
            recipient_puzzle_hash: Bytes32::default(),
        };
        let clawback_puzzle_hash = clawback.to_layer().tree_hash().into();

        alice_p2.spend(
            ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, None),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let inner = alice_p2.spend_with_conditions(ctx, Conditions::new().reserve_fee(1))?;
        let clawback_spend = clawback.clawback_spend(ctx, inner)?;
        ctx.spend(clawback_coin, clawback_spend)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
