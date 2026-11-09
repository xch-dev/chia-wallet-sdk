use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    Memos,
    cat::CatArgs,
    offer::{NotarizedPayment, Payment},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    MerkleTree, Mod,
    conditions::{
        AssertBeforeSecondsAbsolute, AssertPuzzleAnnouncement, AssertSecondsAbsolute, CreateCoin,
    },
    payment_assertion,
    puzzles::{
        AugmentedConditionArgs, AugmentedConditionSolution, P2OneOfManySolution, RevocationArgs,
        SingletonMember, SingletonMemberSolution,
    },
};
use clvm_traits::{ClvmEncoder, ToClvm, clvm_list, clvm_quote, match_list};
use clvm_utils::{ToTreeHash, TreeHash, TreeHasher};
use clvmr::NodePtr;

use crate::{
    DriverError, InnerPuzzleSpend, Layer, MipsSpend, P2OneOfManyLayer, Spend, SpendContext,
    mips_puzzle_hash,
};

use super::OptionType;

/// The delegated puzzle that the underlying coin is spent with when an option is exercised.
///
/// It's a quoted list of conditions that asserts the option hasn't expired, asserts that the strike
/// payment was made to the creator, and sends the underlying amount to the settlement payments puzzle.
pub type OptionDelegatedPuzzle = (
    u8,
    match_list!(
        AssertBeforeSecondsAbsolute,
        AssertPuzzleAnnouncement,
        CreateCoin<Bytes32>
    ),
);

/// The p2 puzzle that locks the underlying coin of an option contract.
///
/// The puzzle is a [`P2OneOfManyLayer`] with two paths:
/// - The exercise path can only be spent with a message from the option singleton (identified by
///   [`launcher_id`](Self::launcher_id)), and runs [`OptionUnderlying::delegated_puzzle`] before expiration.
/// - The clawback path lets the creator spend the coin at or after [`seconds`](Self::seconds).
///
/// For CATs and NFTs, this is the inner p2 puzzle inside of the asset's layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionUnderlying {
    /// The launcher id of the option contract singleton.
    pub launcher_id: Bytes32,
    /// The puzzle hash of the option's creator, who receives the strike payment and can claw back
    /// the underlying coin after expiration.
    pub creator_puzzle_hash: Bytes32,
    /// The absolute timestamp (in seconds) at which the option expires.
    pub seconds: u64,
    /// The amount of the underlying coin.
    pub amount: u64,
    /// The asset and amount that must be paid to exercise the option.
    pub strike_type: OptionType,
}

impl OptionUnderlying {
    /// Creates a new [`OptionUnderlying`] from its fields.
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

    /// Builds the [`MerkleTree`] of the exercise and clawback path hashes.
    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[self.exercise_path_hash(), self.clawback_path_hash()])
    }

    /// Calculates the hash of the exercise path, a MIPS puzzle whose only member is a
    /// [`SingletonMember`] for the option's launcher id.
    pub fn exercise_path_hash(&self) -> Bytes32 {
        let singleton_member_hash = SingletonMember::new(self.launcher_id).curry_tree_hash();
        mips_puzzle_hash(0, Vec::new(), singleton_member_hash, true).into()
    }

    /// Calculates the hash of the clawback path, which wraps the creator's puzzle hash with an
    /// `ASSERT_SECONDS_ABSOLUTE` condition.
    pub fn clawback_path_hash(&self) -> Bytes32 {
        AugmentedConditionArgs::<TreeHash, TreeHash>::new(
            AssertSecondsAbsolute::new(self.seconds).into(),
            self.creator_puzzle_hash.into(),
        )
        .curry_tree_hash()
        .into()
    }

    /// Returns the [`P2OneOfManyLayer`] for the merkle root, which is the underlying p2 puzzle.
    pub fn into_1_of_n(&self) -> P2OneOfManyLayer {
        P2OneOfManyLayer::new(self.merkle_tree().root())
    }

    /// Returns the notarized payment of the strike amount to the creator, with the option's launcher id
    /// as the nonce. The exerciser must pay this through the settlement payments puzzle.
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

    /// Returns the [`OptionDelegatedPuzzle`] that the underlying coin is spent with when exercised.
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

    /// Creates a spend of the exercise path.
    ///
    /// `singleton_inner_puzzle_hash` and `singleton_amount` describe the option singleton being
    /// spent, which must send the authorizing message (see [`OptionContract::exercise`](crate::OptionContract::exercise)).
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

    /// Wraps a spend of the creator's puzzle in the clawback path. It's only valid after expiration.
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

    /// Like [`OptionUnderlying::exercise_spend`], but adds the spend of `coin` to the [`SpendContext`].
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

    /// Like [`OptionUnderlying::clawback_spend`], but adds the spend of `coin` to the [`SpendContext`].
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
