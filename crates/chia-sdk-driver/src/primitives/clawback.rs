use crate::{DriverError, Layer, P2OneOfManyLayer, Puzzle, Spend, SpendContext};
use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::AUGMENTED_CONDITION_HASH;
use chia_sdk_types::{
    conditions::Remark,
    puzzles::{
        AugmentedConditionArgs, AugmentedConditionSolution, P2CurriedArgs, P2CurriedSolution,
        P2OneOfManySolution, P2_CURRIED_PUZZLE_HASH,
    },
    run_puzzle, Condition, MerkleTree,
};
use chia_streamable_macro::streamable;
use chia_traits::Streamable;
use clvm_traits::clvm_list;
use clvm_traits::FromClvm;
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

#[streamable]
pub struct VersionedBlob {
    version: u16,
    blob: Bytes,
}

#[streamable]
#[derive(Copy)]
pub struct Clawback {
    /// The number of seconds until this clawback can be claimed by the recipient.
    pub timelock: u64,
    /// The original sender of the coin, who can claw it back until claimed.
    pub sender_puzzle_hash: Bytes32,
    /// The intended recipient who can claim after the timelock period is up.
    pub receiver_puzzle_hash: Bytes32,
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
        let mut outputs = Vec::<Clawback>::new();
        let mut metadatas = Vec::<Clawback>::new();
        let mut puzhashes = Vec::<[u8; 32]>::with_capacity(conditions.len());
        for condition in conditions {
            match condition {
                Condition::CreateCoin(cc) => puzhashes.push(cc.puzzle_hash.into()),
                Condition::Remark(rm) => match allocator.sexp(rm.rest) {
                    clvmr::SExp::Atom => continue,
                    clvmr::SExp::Pair(first, rest) => {
                        match allocator.sexp(first) {
                            clvmr::SExp::Atom => {
                                let Some(atom) = allocator.small_number(first) else {
                                    continue;
                                };
                                if atom != 2 {
                                    continue;
                                } // magic number for Clawback in REMARK
                            }
                            clvmr::SExp::Pair(_, _) => continue,
                        }
                        // we have seen the magic number
                        // try to deserialise blob
                        match allocator.sexp(rest) {
                            clvmr::SExp::Atom => continue,
                            clvmr::SExp::Pair(r_first, _r_rest) => match allocator.sexp(r_first) {
                                clvmr::SExp::Atom => {
                                    let rest_atom = &allocator.atom(r_first);
                                    metadatas.push(
                                        Clawback::from_bytes_unchecked(
                                            VersionedBlob::from_bytes_unchecked(rest_atom)
                                                .map_err(|_| DriverError::InvalidMemo)?
                                                .blob
                                                .as_ref(),
                                        )
                                        .map_err(|_| DriverError::InvalidMemo)?,
                                    );
                                }
                                clvmr::SExp::Pair(_, _) => continue,
                            },
                        }
                    }
                },
                _ => {}
            }
        }
        for &clawback in &metadatas {
            if puzhashes.contains(&clawback.to_layer().tree_hash().to_bytes()) {
                outputs.push(clawback);
            }
        }
        Ok(Some(outputs))
    }

    pub fn receiver_path_puzzle_hash(&self) -> TreeHash {
        CurriedProgram {
            program: TreeHash::new(AUGMENTED_CONDITION_HASH),
            args: AugmentedConditionArgs::new(
                Condition::<TreeHash>::assert_seconds_relative(self.timelock),
                TreeHash::from(self.receiver_puzzle_hash),
            ),
        }
        .tree_hash()
    }

    pub fn receiver_path_puzzle(
        &self,
        ctx: &mut SpendContext,
        inner_puzzle: NodePtr,
    ) -> Result<NodePtr, DriverError> {
        ctx.curry(AugmentedConditionArgs::new(
            Condition::<NodePtr>::assert_seconds_relative(self.timelock),
            inner_puzzle,
        ))
    }

    pub fn sender_path_puzzle_hash(&self) -> TreeHash {
        CurriedProgram {
            program: P2_CURRIED_PUZZLE_HASH,
            args: P2CurriedArgs::new(self.sender_puzzle_hash),
        }
        .tree_hash()
    }

    pub fn sender_path_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2CurriedArgs::new(self.sender_puzzle_hash))
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[
            self.receiver_path_puzzle_hash().into(),
            self.sender_path_puzzle_hash().into(),
        ])
    }

    pub fn to_layer(&self) -> P2OneOfManyLayer {
        P2OneOfManyLayer::new(self.merkle_tree().root())
    }

    // this function returns the Remark condition required to hint at this clawback
    // it should be included alongside the createcoin that creates this
    pub fn get_remark_condition(
        &self,
        allocator: &mut Allocator,
    ) -> Result<Remark<NodePtr>, DriverError> {
        let vb = VersionedBlob {
            version: 1,
            blob: self
                .to_bytes()
                .map_err(|_| DriverError::InvalidMemo)?
                .into(),
        };
        // 2 is the magic number for clawback
        let node_ptr = clvm_list!(
            2,
            Bytes::new(vb.to_bytes().map_err(|_| DriverError::InvalidMemo)?)
        )
        .to_clvm(allocator)?;

        Ok(Remark::new(node_ptr))
    }

    pub fn receiver_spend(
        &self,
        ctx: &mut SpendContext,
        spend: Spend,
    ) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle = self.receiver_path_puzzle(ctx, spend.puzzle)?;
        let solution = ctx.alloc(&AugmentedConditionSolution::new(spend.solution))?;

        let proof = merkle_tree
            .proof(ctx.tree_hash(puzzle).into())
            .ok_or(DriverError::InvalidMerkleProof)?;

        P2OneOfManyLayer::new(merkle_tree.root())
            .construct_spend(ctx, P2OneOfManySolution::new(proof, puzzle, solution))
    }

    pub fn sender_spend(&self, ctx: &mut SpendContext, spend: Spend) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle = self.sender_path_puzzle(ctx)?;
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
    use chia_protocol::{Coin, SpendBundle};
    use chia_puzzle_types::Memos;
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
            timelock: 1,
            sender_puzzle_hash: alice.puzzle_hash,
            receiver_puzzle_hash: bob.puzzle_hash,
        };
        let clawback_puzzle_hash = clawback.to_layer().tree_hash().into();
        let coin = alice.coin;
        let conditions = Conditions::new()
            .create_coin(clawback_puzzle_hash, 1, Memos::None)
            .with(clawback.get_remark_condition(ctx)?);
        alice_p2.spend(ctx, coin, conditions)?;

        let cs = ctx.take();

        let clawback_coin = Coin::new(coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(cs, &[alice.sk])?;

        let puzzle_reveal = sim
            .puzzle_reveal(coin.coin_id())
            .expect("missing puzzle")
            .to_clvm(ctx)?;

        let solution = sim
            .solution(coin.coin_id())
            .expect("missing solution")
            .to_clvm(ctx)?;

        let puzzle = Puzzle::parse(ctx, puzzle_reveal);

        // check we can recreate Clawback from the spend
        let children = Clawback::parse_children(ctx, puzzle, solution)
            .expect("we should have found the child")
            .expect("we should have found children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], clawback);

        let bob_inner = bob_p2.spend_with_conditions(ctx, Conditions::new().reserve_fee(1))?;
        let receiver_spend = clawback.receiver_spend(ctx, bob_inner)?;
        ctx.spend(clawback_coin, receiver_spend)?;

        sim.spend_coins(ctx.take(), &[bob.sk])?;

        Ok(())
    }

    #[test]
    fn test_clawback_compatible_with_python() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let bytes = hex_literal::hex!("00000001e3b0c44298fc1c149afbf4c8996fb924000000000000000000000000000000014eb7420f8651b09124e1d40cdc49eeddacbaa0c25e6ae5a0a482fac8e3b5259f000001977420dc00ff02ffff01ff02ffff01ff02ffff03ff0bffff01ff02ffff03ffff09ff05ffff1dff0bffff1effff0bff0bffff02ff06ffff04ff02ffff04ff17ff8080808080808080ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff01ff04ffff04ff04ffff04ff05ffff04ffff02ff06ffff04ff02ffff04ff17ff80808080ff80808080ffff02ff17ff2f808080ff0180ffff04ffff01ff32ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080ffff04ffff01b0b50b02adba343fff8bf3a94e92ed7df43743aedf0006b81a6c00ae573c0cce7d08216f60886fe84e4078a5209b0e5171ff018080ff80ffff01ffff33ffa0aeb663f32c4cfe1122710bc03cdc086f87e3243c055e8bebba42189cafbaf465ff840098968080ffff01ff02ffc04e00010000004800000000000000644eb7420f8651b09124e1d40cdc49eeddacbaa0c25e6ae5a0a482fac8e3b5259f5abb5d5568b4a7411dd97b3356cfedfac09b5fb35621a7fa29ab9b59dc905fb68080ff8080a8a06f869d849d69f194df0c5e003a302aa360309a8a75eb50867f8f4c90484d8fe6cc63d4d3bc1f4d5ac456e75678ad09209f744a4aea5857e2771f0c351623f90f72418d086862c66d4270d8b04c13814d8279050ff9e9944c8d491377da87");
        let sb = SpendBundle::from_bytes(&bytes)?;
        let puzzle_clvm = sb.coin_spends[0].puzzle_reveal.to_clvm(ctx)?;
        let puz = Puzzle::parse(ctx, puzzle_clvm);
        let sol = sb.coin_spends[0].solution.to_clvm(ctx)?;
        let children = Clawback::parse_children(ctx, puz, sol)
            .expect("we should have found the child")
            .expect("we should have found children");
        assert_eq!(children.len(), 1);
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
            timelock: u64::MAX,
            sender_puzzle_hash: alice.puzzle_hash,
            receiver_puzzle_hash: Bytes32::default(),
        };
        let clawback_puzzle_hash = clawback.to_layer().tree_hash().into();

        alice_p2.spend(
            ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, Memos::None),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let inner = alice_p2.spend_with_conditions(ctx, Conditions::new().reserve_fee(1))?;
        let sender_spend = clawback.sender_spend(ctx, inner)?;
        ctx.spend(clawback_coin, sender_spend)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
