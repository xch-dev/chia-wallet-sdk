use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    cat::CatArgs,
    offer::{NotarizedPayment, Payment},
    Memos,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    conditions::{
        AssertBeforeSecondsAbsolute, AssertPuzzleAnnouncement, AssertSecondsAbsolute, CreateCoin,
    },
    payment_assertion,
    puzzles::{
        AugmentedConditionArgs, AugmentedConditionSolution, P2OneOfManySolution, RevocationArgs,
        SingletonMember, SingletonMemberSolution,
    },
    MerkleTree, Mod,
};
use clvm_traits::{clvm_list, clvm_quote, match_list, ClvmEncoder, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash, TreeHasher};
use clvmr::NodePtr;

use crate::{
    member_puzzle_hash, DriverError, InnerPuzzleSpend, Layer, MipsSpend, P2OneOfManyLayer, Spend,
    SpendContext,
};

use super::OptionType;

pub type OptionDelegatedPuzzle = (
    u8,
    match_list!(
        AssertBeforeSecondsAbsolute,
        AssertPuzzleAnnouncement,
        CreateCoin<Bytes32>
    ),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionUnderlying {
    pub launcher_id: Bytes32,
    pub creator_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub amount: u64,
    pub strike_type: OptionType,
}

impl OptionUnderlying {
    pub fn new(
        launcher_id: Bytes32,
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        amount: u64,
        strike_type: OptionType,
    ) -> Self {
        Self {
            launcher_id,
            creator_puzzle_hash,
            seconds,
            amount,
            strike_type,
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

    pub fn requested_payment<E>(
        &self,
        encoder: &mut E,
    ) -> Result<NotarizedPayment<E::Node>, DriverError>
    where
        E: ClvmEncoder,
    {
        Ok(NotarizedPayment {
            nonce: self.launcher_id,
            payments: vec![Payment {
                puzzle_hash: self.creator_puzzle_hash,
                amount: self.strike_type.amount(),
                memos: if self.strike_type.is_hinted() {
                    Memos::Some(vec![self.creator_puzzle_hash].to_clvm(encoder)?)
                } else {
                    Memos::None
                },
            }],
        })
    }

    pub fn delegated_puzzle(&self) -> OptionDelegatedPuzzle {
        let puzzle_hash = match self.strike_type {
            OptionType::Xch { .. } => SETTLEMENT_PAYMENT_HASH.into(),
            OptionType::Cat { asset_id, .. } => {
                CatArgs::curry_tree_hash(asset_id, SETTLEMENT_PAYMENT_HASH.into()).into()
            }
            OptionType::RevocableCat {
                asset_id,
                hidden_puzzle_hash,
                ..
            } => CatArgs::curry_tree_hash(
                asset_id,
                RevocationArgs::new(hidden_puzzle_hash, SETTLEMENT_PAYMENT_HASH.into())
                    .curry_tree_hash(),
            )
            .into(),
            OptionType::Nft {
                settlement_puzzle_hash,
                ..
            } => settlement_puzzle_hash,
        };

        clvm_quote!(clvm_list!(
            AssertBeforeSecondsAbsolute::new(self.seconds),
            payment_assertion(
                puzzle_hash,
                self.requested_payment(&mut TreeHasher)
                    .expect("failed to hash")
                    .tree_hash()
            ),
            CreateCoin::new(SETTLEMENT_PAYMENT_HASH.into(), self.amount, Memos::None)
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
            InnerPuzzleSpend::new(
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
