use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    conditions::{AssertSecondsAbsolute, CreateCoin},
    puzzles::{
        AugmentedConditionArgs, AugmentedConditionSolution, P2OneOfManySolution, SingletonMember,
        SingletonMemberSolution,
    },
    MerkleTree, Mod,
};
use clvm_traits::{clvm_list, clvm_quote, match_list};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    member_puzzle_hash, DriverError, Layer, MemberSpend, MipsSpend, P2OneOfManyLayer, Spend,
    SpendContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionUnderlying {
    pub launcher_id: Bytes32,
    pub creator_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub amount: u64,
}

impl OptionUnderlying {
    pub fn new(
        launcher_id: Bytes32,
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        amount: u64,
    ) -> Self {
        Self {
            launcher_id,
            creator_puzzle_hash,
            seconds,
            amount,
        }
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[self.exercise_path_hash(), self.clawback_path_hash()])
    }

    pub fn exercise_path_hash(&self) -> Bytes32 {
        let singleton_member_hash = SingletonMember::new(self.launcher_id).curry_tree_hash();
        member_puzzle_hash(0, Vec::new(), singleton_member_hash, true).into()
    }

    pub fn clawback_path_hash(&self) -> Bytes32 {
        AugmentedConditionArgs::<TreeHash, TreeHash>::new(
            AssertSecondsAbsolute::new(self.seconds).into(),
            self.creator_puzzle_hash.into(),
        )
        .curry_tree_hash()
        .into()
    }

    pub fn into_1_of_n(&self) -> P2OneOfManyLayer {
        P2OneOfManyLayer::new(self.merkle_tree().root())
    }

    pub fn delegated_puzzle(
        &self,
    ) -> (u8, match_list!(AssertSecondsAbsolute, CreateCoin<Bytes32>)) {
        clvm_quote!(clvm_list!(
            AssertSecondsAbsolute::new(self.seconds),
            CreateCoin::new(SETTLEMENT_PAYMENT_HASH.into(), self.amount, None)
        ))
    }

    pub fn exercise_spend(
        &self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
    ) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let custody_hash: TreeHash = self.exercise_path_hash().into();
        let merkle_proof = merkle_tree
            .proof(custody_hash.into())
            .ok_or(DriverError::InvalidMerkleProof)?;

        let delegated_puzzle = ctx.alloc(&self.delegated_puzzle())?;
        let delegated_spend = Spend::new(delegated_puzzle, NodePtr::NIL);

        let mut mips = MipsSpend::new(delegated_spend);

        let singleton_member_puzzle = ctx.curry(SingletonMember::new(self.launcher_id))?;
        let singleton_member_solution = ctx.alloc(&SingletonMemberSolution::new(
            singleton_inner_puzzle_hash,
            singleton_amount,
        ))?;
        mips.members.insert(
            custody_hash,
            MemberSpend::new(
                0,
                Vec::new(),
                Spend::new(singleton_member_puzzle, singleton_member_solution),
            ),
        );

        let spend = mips.spend(ctx, custody_hash)?;

        P2OneOfManyLayer::new(merkle_tree.root()).construct_spend(
            ctx,
            P2OneOfManySolution::new(merkle_proof, spend.puzzle, spend.solution),
        )
    }

    pub fn clawback_spend(
        &self,
        ctx: &mut SpendContext,
        spend: Spend,
    ) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle_hash = self.clawback_path_hash();
        let merkle_proof = merkle_tree
            .proof(puzzle_hash)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let puzzle = ctx.curry(AugmentedConditionArgs::<NodePtr, NodePtr>::new(
            AssertSecondsAbsolute::new(self.seconds).into(),
            spend.puzzle,
        ))?;

        let solution = ctx.alloc(&AugmentedConditionSolution::new(spend.solution))?;

        P2OneOfManyLayer::new(merkle_tree.root()).construct_spend(
            ctx,
            P2OneOfManySolution::new(merkle_proof, puzzle, solution),
        )
    }

    pub fn exercise_coin_spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
    ) -> Result<(), DriverError> {
        let spend = self.exercise_spend(ctx, singleton_inner_puzzle_hash, singleton_amount)?;
        ctx.spend(coin, spend)
    }

    pub fn clawback_coin_spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        spend: Spend,
    ) -> Result<(), DriverError> {
        let spend = self.clawback_spend(ctx, spend)?;
        ctx.spend(coin, spend)
    }
}

impl ToTreeHash for OptionUnderlying {
    fn tree_hash(&self) -> TreeHash {
        self.into_1_of_n().tree_hash()
    }
}
