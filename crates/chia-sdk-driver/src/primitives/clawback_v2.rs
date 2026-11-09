use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{
    Conditions, MerkleTree, Mod,
    conditions::{AssertBeforeSecondsAbsolute, AssertSecondsAbsolute, CreateCoin, Memos},
    puzzles::{AugmentedConditionArgs, AugmentedConditionSolution, P2OneOfManySolution},
};
use clvm_traits::{FromClvm, clvm_list, clvm_quote, match_list};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, P2OneOfManyLayer, Spend, SpendContext, SpendWithConditions};

/// The push-through path puzzle of a [`ClawbackV2`]: a quoted list of conditions that asserts the
/// expiration time has passed and creates the coin for the receiver.
pub type PushThroughPath = (
    u8,
    match_list!(AssertSecondsAbsolute, CreateCoin<[Bytes32; 1]>),
);

/// A clawback with an absolute expiration time, which can wrap the p2 puzzle of XCH or CAT coins.
///
/// The puzzle is a [`P2OneOfManyLayer`] with three paths:
/// - The sender path lets the sender spend the coin before [`seconds`](Self::seconds). This is used
///   to claw back the coin ([`ClawbackV2::recover_spend`]) or to send it to the receiver early
///   ([`ClawbackV2::force_spend`]).
/// - The receiver path lets the receiver spend the coin at or after `seconds`
///   ([`ClawbackV2::finish_spend`]).
/// - The push-through path lets anyone send the coin to the receiver at or after `seconds`
///   ([`ClawbackV2::push_through_spend`]).
///
/// The clawback is discovered from the memos of the `CREATE_COIN` that creates it. The memos are the
/// receiver's puzzle hash (as the hint) followed by [`ClawbackV2::memo`].
///
/// # Example
///
/// Send a coin with a clawback, then claw it back before it expires:
///
/// ```
/// use chia_protocol::{Bytes32, Coin};
/// use chia_sdk_driver::{ClawbackV2, SpendContext, StandardLayer};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::Conditions;
/// use clvm_traits::clvm_list;
/// use clvm_utils::ToTreeHash;
///
/// let mut sim = Simulator::new();
/// let ctx = &mut SpendContext::new();
///
/// let alice = sim.bls(1);
/// let alice_p2 = StandardLayer::new(alice.pk);
/// let bob_puzzle_hash = Bytes32::new([1; 32]);
///
/// // The clawback expires at a timestamp of 100.
/// let clawback = ClawbackV2::new(alice.puzzle_hash, bob_puzzle_hash, 100, 1, true);
/// let clawback_puzzle_hash = clawback.tree_hash().into();
///
/// // Hint the coin to Bob and include the clawback memo so that it can be parsed.
/// let memos = ctx.memos(&clvm_list!(bob_puzzle_hash, clawback.memo()))?;
/// alice_p2.spend(
///     ctx,
///     alice.coin,
///     Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
/// )?;
/// sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;
///
/// // Alice claws the coin back before it expires.
/// let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);
/// clawback.recover_coin_spend(ctx, clawback_coin, &alice_p2, Conditions::new())?;
/// sim.spend_coins(ctx.take(), &[alice.sk])?;
///
/// let recovered = Coin::new(clawback_coin.coin_id(), alice.puzzle_hash, 1);
/// assert!(sim.coin_state(recovered.coin_id()).is_some());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClawbackV2 {
    /// The puzzle hash of the sender, who can claw back the coin before it expires.
    pub sender_puzzle_hash: Bytes32,
    /// The puzzle hash of the receiver, who can claim the coin once it expires.
    pub receiver_puzzle_hash: Bytes32,
    /// The absolute timestamp (in seconds) at which the clawback expires.
    pub seconds: u64,
    /// The amount of the clawback coin, which is the amount of the coin each path creates.
    pub amount: u64,
    /// Whether the coins created by the spend paths are hinted with their destination puzzle hash.
    /// This should be `true` for CATs, so that the outputs can be discovered.
    pub hinted: bool,
}

impl ClawbackV2 {
    /// Creates a new [`ClawbackV2`] from its fields.
    pub fn new(
        sender_puzzle_hash: Bytes32,
        receiver_puzzle_hash: Bytes32,
        seconds: u64,
        amount: u64,
        hinted: bool,
    ) -> Self {
        Self {
            sender_puzzle_hash,
            receiver_puzzle_hash,
            seconds,
            amount,
            hinted,
        }
    }

    /// Parses a [`ClawbackV2`] from the [`ClawbackV2::memo`] that follows the hint in a `CREATE_COIN`.
    ///
    /// The remaining fields come from the created coin and the context. Returns [`None`] if the memo
    /// is malformed or the resulting clawback doesn't hash to `expected_puzzle_hash`.
    pub fn from_memo(
        allocator: &Allocator,
        memo: NodePtr,
        receiver_puzzle_hash: Bytes32,
        amount: u64,
        hinted: bool,
        expected_puzzle_hash: Bytes32,
    ) -> Option<Self> {
        let (sender_puzzle_hash, (seconds, ())) =
            <(Bytes32, (u64, ()))>::from_clvm(allocator, memo).ok()?;

        let clawback = Self {
            sender_puzzle_hash,
            receiver_puzzle_hash,
            seconds,
            amount,
            hinted,
        };

        if clawback.tree_hash() != expected_puzzle_hash.into() {
            return None;
        }

        Some(clawback)
    }

    /// Returns the memo that describes this clawback, which is the sender puzzle hash and expiration.
    ///
    /// This should be included after the receiver's hint in the memos of the `CREATE_COIN`.
    pub fn memo(&self) -> (Bytes32, (u64, ())) {
        (self.sender_puzzle_hash, (self.seconds, ()))
    }

    /// Builds the [`MerkleTree`] of the sender, receiver, and push-through path hashes.
    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[
            self.sender_path_hash(),
            self.receiver_path_hash(),
            self.push_through_path_hash(),
        ])
    }

    /// Calculates the hash of the sender path, which wraps the sender's puzzle hash with an
    /// `ASSERT_BEFORE_SECONDS_ABSOLUTE` condition.
    pub fn sender_path_hash(&self) -> Bytes32 {
        AugmentedConditionArgs::<TreeHash, TreeHash>::new(
            AssertBeforeSecondsAbsolute::new(self.seconds).into(),
            self.sender_puzzle_hash.into(),
        )
        .curry_tree_hash()
        .into()
    }

    /// Calculates the hash of the receiver path, which wraps the receiver's puzzle hash with an
    /// `ASSERT_SECONDS_ABSOLUTE` condition.
    pub fn receiver_path_hash(&self) -> Bytes32 {
        AugmentedConditionArgs::<TreeHash, TreeHash>::new(
            AssertSecondsAbsolute::new(self.seconds).into(),
            self.receiver_puzzle_hash.into(),
        )
        .curry_tree_hash()
        .into()
    }

    /// Returns the push-through path puzzle, which asserts the expiration and creates a coin
    /// of [`amount`](Self::amount) for the receiver (hinted if [`hinted`](Self::hinted) is set).
    pub fn push_through_path(&self) -> PushThroughPath {
        clvm_quote!(clvm_list!(
            AssertSecondsAbsolute::new(self.seconds),
            CreateCoin::new(
                self.receiver_puzzle_hash,
                self.amount,
                if self.hinted {
                    Memos::Some([self.receiver_puzzle_hash])
                } else {
                    Memos::None
                }
            )
        ))
    }

    /// Calculates the hash of the push-through path.
    pub fn push_through_path_hash(&self) -> Bytes32 {
        self.push_through_path().tree_hash().into()
    }

    /// Returns the [`P2OneOfManyLayer`] for the clawback's merkle root, which is the clawback puzzle.
    pub fn into_1_of_n(&self) -> P2OneOfManyLayer {
        P2OneOfManyLayer::new(self.merkle_tree().root())
    }

    /// Wraps a spend of the sender's puzzle in the sender path. It's only valid before expiration.
    pub fn sender_spend(&self, ctx: &mut SpendContext, spend: Spend) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle_hash = self.sender_path_hash();
        let merkle_proof = merkle_tree
            .proof(puzzle_hash)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let puzzle = ctx.curry(AugmentedConditionArgs::<NodePtr, NodePtr>::new(
            AssertBeforeSecondsAbsolute::new(self.seconds).into(),
            spend.puzzle,
        ))?;

        let solution = ctx.alloc(&AugmentedConditionSolution::new(spend.solution))?;

        P2OneOfManyLayer::new(merkle_tree.root()).construct_spend(
            ctx,
            P2OneOfManySolution::new(merkle_proof, puzzle, solution),
        )
    }

    /// Wraps a spend of the receiver's puzzle in the receiver path. It's only valid after expiration.
    pub fn receiver_spend(
        &self,
        ctx: &mut SpendContext,
        spend: Spend,
    ) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle_hash = self.receiver_path_hash();
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

    /// Creates a spend of the push-through path, which requires no signature. It's only valid after expiration.
    pub fn push_through_spend(&self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let merkle_tree = self.merkle_tree();

        let puzzle_hash = self.push_through_path_hash();
        let merkle_proof = merkle_tree
            .proof(puzzle_hash)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let puzzle = ctx.alloc(&self.push_through_path())?;

        P2OneOfManyLayer::new(merkle_tree.root()).construct_spend(
            ctx,
            P2OneOfManySolution::new(merkle_proof, puzzle, NodePtr::NIL),
        )
    }

    /// Creates a sender spend that sends the coin back to the sender, along with the extra `conditions`.
    ///
    /// `inner` must be the sender's puzzle. It's only valid before expiration.
    pub fn recover_spend<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<Spend, DriverError>
    where
        I: SpendWithConditions,
    {
        let hint = ctx.hint(self.sender_puzzle_hash)?;

        let inner_spend = inner.spend_with_conditions(
            ctx,
            conditions.create_coin(
                self.sender_puzzle_hash,
                self.amount,
                if self.hinted { hint } else { Memos::None },
            ),
        )?;

        self.sender_spend(ctx, inner_spend)
    }

    /// Like [`ClawbackV2::recover_spend`], but adds the spend of `coin` to the [`SpendContext`].
    pub fn recover_coin_spend<I>(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        inner: &I,
        conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let spend = self.recover_spend(ctx, inner, conditions)?;
        ctx.spend(coin, spend)
    }

    /// Creates a sender spend that sends the coin to the receiver early, along with the extra `conditions`.
    ///
    /// `inner` must be the sender's puzzle. It's only valid before expiration.
    pub fn force_spend<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<Spend, DriverError>
    where
        I: SpendWithConditions,
    {
        let hint = ctx.hint(self.receiver_puzzle_hash)?;

        let inner_spend = inner.spend_with_conditions(
            ctx,
            conditions.create_coin(
                self.receiver_puzzle_hash,
                self.amount,
                if self.hinted { hint } else { Memos::None },
            ),
        )?;

        self.sender_spend(ctx, inner_spend)
    }

    /// Like [`ClawbackV2::force_spend`], but adds the spend of `coin` to the [`SpendContext`].
    pub fn force_coin_spend<I>(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        inner: &I,
        conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let spend = self.force_spend(ctx, inner, conditions)?;
        ctx.spend(coin, spend)
    }

    /// Creates a receiver spend that sends the coin to the receiver, along with the extra `conditions`.
    ///
    /// `inner` must be the receiver's puzzle. It's only valid after expiration.
    pub fn finish_spend<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<Spend, DriverError>
    where
        I: SpendWithConditions,
    {
        let hint = ctx.hint(self.receiver_puzzle_hash)?;

        let inner_spend = inner.spend_with_conditions(
            ctx,
            conditions.create_coin(
                self.receiver_puzzle_hash,
                self.amount,
                if self.hinted { hint } else { Memos::None },
            ),
        )?;

        self.receiver_spend(ctx, inner_spend)
    }

    /// Like [`ClawbackV2::finish_spend`], but adds the spend of `coin` to the [`SpendContext`].
    pub fn finish_coin_spend<I>(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        inner: &I,
        conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let spend = self.finish_spend(ctx, inner, conditions)?;
        ctx.spend(coin, spend)
    }

    /// Like [`ClawbackV2::push_through_spend`], but adds the spend of `coin` to the [`SpendContext`].
    pub fn push_through_coin_spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
    ) -> Result<(), DriverError> {
        let spend = self.push_through_spend(ctx)?;
        ctx.spend(coin, spend)
    }
}

impl ToTreeHash for ClawbackV2 {
    fn tree_hash(&self) -> TreeHash {
        self.into_1_of_n().tree_hash()
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_protocol::Coin;
    use chia_sdk_test::{Simulator, expect_spend};
    use clvm_traits::{ToClvm, clvm_list};
    use rstest::rstest;

    use crate::{Cat, CatSpend, SpendWithConditions, StandardLayer};

    use super::*;

    #[rstest]
    fn test_clawback_memo(#[values(false, true)] hinted: bool) -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let clawback =
            ClawbackV2::new(Bytes32::new([1; 32]), Bytes32::new([2; 32]), 100, 1, hinted);
        let memo = clawback.memo().to_clvm(&mut allocator)?;

        let roundtrip = ClawbackV2::from_memo(
            &allocator,
            memo,
            Bytes32::new([2; 32]),
            1,
            hinted,
            clawback.tree_hash().into(),
        );
        assert_eq!(roundtrip, Some(clawback));

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_recover_xch(
        #[values(false, true)] hinted: bool,
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, hinted);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        p2_alice.spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        clawback.recover_coin_spend(&mut ctx, clawback_coin, &p2_alice, Conditions::new())?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(
                sim.coin_state(Coin::new(clawback_coin.coin_id(), alice.puzzle_hash, 1).coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_force_xch(
        #[values(false, true)] hinted: bool,
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, hinted);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        p2_alice.spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        clawback.force_coin_spend(&mut ctx, clawback_coin, &p2_alice, Conditions::new())?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(
                sim.coin_state(Coin::new(clawback_coin.coin_id(), bob.puzzle_hash, 1).coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_finish_xch(
        #[values(false, true)] hinted: bool,
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);
        let p2_bob = StandardLayer::new(bob.pk);

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, hinted);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        p2_alice.spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        clawback.finish_coin_spend(&mut ctx, clawback_coin, &p2_bob, Conditions::new())?;

        expect_spend(sim.spend_coins(ctx.take(), &[bob.sk]), after_expiration);

        if after_expiration {
            assert!(
                sim.coin_state(Coin::new(clawback_coin.coin_id(), bob.puzzle_hash, 1).coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_push_through_xch(
        #[values(false, true)] hinted: bool,
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, hinted);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        p2_alice.spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        clawback.push_through_coin_spend(&mut ctx, clawback_coin)?;

        expect_spend(sim.spend_coins(ctx.take(), &[bob.sk]), after_expiration);

        if after_expiration {
            assert!(
                sim.coin_state(Coin::new(clawback_coin.coin_id(), bob.puzzle_hash, 1).coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_recover_cat(
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::single_issuance(
            &mut ctx,
            alice.coin.coin_id(),
            None,
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        let cat = cats[0];
        p2_alice.spend(&mut ctx, alice.coin, issue_cat)?;

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, true);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        let inner_spend = p2_alice.spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(cat, inner_spend)])?;

        let clawback_cat = cat.child(clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let clawback_spend = clawback.recover_spend(&mut ctx, &p2_alice, Conditions::new())?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(clawback_cat, clawback_spend)])?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(
                sim.coin_state(clawback_cat.child(alice.puzzle_hash, 1).coin.coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_force_cat(
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::single_issuance(
            &mut ctx,
            alice.coin.coin_id(),
            None,
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        let cat = cats[0];
        p2_alice.spend(&mut ctx, alice.coin, issue_cat)?;

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, true);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        let inner_spend = p2_alice.spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(cat, inner_spend)])?;

        let clawback_cat = cat.child(clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let clawback_spend = clawback.force_spend(&mut ctx, &p2_alice, Conditions::new())?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(clawback_cat, clawback_spend)])?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(
                sim.coin_state(clawback_cat.child(bob.puzzle_hash, 1).coin.coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_finish_cat(
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);
        let p2_bob = StandardLayer::new(bob.pk);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::single_issuance(
            &mut ctx,
            alice.coin.coin_id(),
            None,
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        let cat = cats[0];
        p2_alice.spend(
            &mut ctx,
            alice.coin,
            issue_cat.create_coin(alice.puzzle_hash, 0, Memos::None),
        )?;

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, true);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        let inner_spend = p2_alice.spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(cat, inner_spend)])?;

        let clawback_cat = cat.child(clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let clawback_spend = clawback.finish_spend(&mut ctx, &p2_bob, Conditions::new())?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(clawback_cat, clawback_spend)])?;

        expect_spend(sim.spend_coins(ctx.take(), &[bob.sk]), after_expiration);

        if after_expiration {
            assert!(
                sim.coin_state(clawback_cat.child(bob.puzzle_hash, 1).coin.coin_id())
                    .is_some()
            );
        }

        Ok(())
    }

    #[rstest]
    fn test_clawback_v2_push_through_cat(
        #[values(false, true)] after_expiration: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        if after_expiration {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let memos = ctx.hint(alice.puzzle_hash)?;
        let (issue_cat, cats) = Cat::single_issuance(
            &mut ctx,
            alice.coin.coin_id(),
            None,
            1,
            Conditions::new().create_coin(alice.puzzle_hash, 1, memos),
        )?;
        let cat = cats[0];
        p2_alice.spend(
            &mut ctx,
            alice.coin,
            issue_cat.create_coin(alice.puzzle_hash, 0, Memos::None),
        )?;

        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, true);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        let inner_spend = p2_alice.spend_with_conditions(
            &mut ctx,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, memos),
        )?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(cat, inner_spend)])?;

        let clawback_cat = cat.child(clawback_puzzle_hash, 1);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let clawback_spend = clawback.push_through_spend(&mut ctx)?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(clawback_cat, clawback_spend)])?;

        expect_spend(sim.spend_coins(ctx.take(), &[]), after_expiration);

        if after_expiration {
            assert!(
                sim.coin_state(clawback_cat.child(bob.puzzle_hash, 1).coin.coin_id())
                    .is_some()
            );
        }

        Ok(())
    }
}
