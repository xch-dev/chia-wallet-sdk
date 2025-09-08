use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use chia_sdk_types::{
    puzzles::{
        CatalogVerificationInnerPuzzleMakerArgs, CatalogVerificationInnerPuzzleMakerSolution,
        VerificationAsserterArgs, VerificationAsserterSolution, VerificationLayer1stCurryArgs,
    },
    Mod,
};
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{DriverError, Spend, SpendContext};

#[derive(Debug, Copy, Clone)]
#[must_use]
pub struct VerificationAsserter {
    pub verifier_singleton_struct_hash: Bytes32,
    pub verification_inner_puzzle_self_hash: Bytes32,
    pub version: u32,
    pub tail_hash_hash: Bytes32,
    pub data_hash_hash: Bytes32,
}

impl VerificationAsserter {
    pub fn new(
        verifier_singleton_struct_hash: Bytes32,
        verification_inner_puzzle_self_hash: Bytes32,
        version: u32,
        tail_hash_hash: TreeHash,
        data_hash_hash: TreeHash,
    ) -> Self {
        Self {
            verifier_singleton_struct_hash,
            verification_inner_puzzle_self_hash,
            version,
            tail_hash_hash: tail_hash_hash.into(),
            data_hash_hash: data_hash_hash.into(),
        }
    }

    pub fn from(
        verifier_launcher_id: Bytes32,
        version: u32,
        tail_hash_hash: TreeHash,
        data_hash_hash: TreeHash,
    ) -> Self {
        Self::new(
            SingletonStruct::new(verifier_launcher_id)
                .tree_hash()
                .into(),
            VerificationLayer1stCurryArgs::curry_tree_hash(verifier_launcher_id).into(),
            version,
            tail_hash_hash,
            data_hash_hash,
        )
    }

    pub fn tree_hash(&self) -> TreeHash {
        VerificationAsserterArgs::new(
            self.verifier_singleton_struct_hash,
            CatalogVerificationInnerPuzzleMakerArgs::new(
                self.verification_inner_puzzle_self_hash,
                self.version,
                self.tail_hash_hash.into(),
                self.data_hash_hash.into(),
            )
            .curry_tree_hash(),
        )
        .curry_tree_hash()
    }

    pub fn inner_spend(
        &self,
        ctx: &mut SpendContext,
        verifier_proof: LineageProof,
        launcher_amount: u64,
        comment: String,
    ) -> Result<Spend, DriverError> {
        let verification_inner_puzzle_maker =
            ctx.curry(CatalogVerificationInnerPuzzleMakerArgs::new(
                self.verification_inner_puzzle_self_hash,
                self.version,
                self.tail_hash_hash.into(),
                self.data_hash_hash.into(),
            ))?;

        let puzzle = ctx.curry(VerificationAsserterArgs::new(
            self.verifier_singleton_struct_hash,
            verification_inner_puzzle_maker,
        ))?;

        let solution = ctx.alloc(&VerificationAsserterSolution {
            verifier_proof,
            verification_inner_puzzle_maker_solution: CatalogVerificationInnerPuzzleMakerSolution {
                comment,
            },
            launcher_amount,
        })?;

        Ok(Spend::new(puzzle, solution))
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        verifier_proof: LineageProof,
        launcher_amount: u64,
        comment: String,
    ) -> Result<(), DriverError> {
        let spend = self.inner_spend(ctx, verifier_proof, launcher_amount, comment)?;

        ctx.spend(coin, spend)?;
        Ok(())
    }
}
