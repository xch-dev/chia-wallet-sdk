use std::num::NonZeroU64;

use chia_protocol::Bytes32;
use chia_sdk_types::{
    AugmentedConditionArgs, AugmentedConditionSolution, Condition, MerkleTree, P2CurriedArgs,
    P2CurriedSolution, P2OneOfManySolution, AUGMENTED_CONDITION_PUZZLE_HASH,
    P2_CURRIED_PUZZLE_HASH,
};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{DriverError, Layer, P2OneOfManyLayer, Spend, SpendContext};

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

        alice_p2.spend(
            ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, None),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

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
