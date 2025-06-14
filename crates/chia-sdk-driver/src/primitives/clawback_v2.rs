use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{
    conditions::{AssertBeforeSecondsAbsolute, AssertSecondsAbsolute, CreateCoin, Memos},
    puzzles::{AugmentedConditionArgs, AugmentedConditionSolution, P2OneOfManySolution},
    Conditions, MerkleTree, Mod,
};
use clvm_traits::{clvm_list, clvm_quote, match_list, FromClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, P2OneOfManyLayer, Spend, SpendContext, SpendWithConditions};

pub type PushThroughPath = (
    u8,
    match_list!(AssertSecondsAbsolute, CreateCoin<[Bytes32; 1]>),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClawbackV2 {
    pub sender_puzzle_hash: Bytes32,
    pub receiver_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub amount: u64,
    pub hinted: bool,
}

impl ClawbackV2 {
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

    pub fn memo(&self) -> (Bytes32, (u64, ())) {
        (self.sender_puzzle_hash, (self.seconds, ()))
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&[
            self.sender_path_hash(),
            self.receiver_path_hash(),
            self.push_through_path_hash(),
        ])
    }

    pub fn sender_path_hash(&self) -> Bytes32 {
        AugmentedConditionArgs::<TreeHash, TreeHash>::new(
            AssertBeforeSecondsAbsolute::new(self.seconds).into(),
            self.sender_puzzle_hash.into(),
        )
        .curry_tree_hash()
        .into()
    }

    pub fn receiver_path_hash(&self) -> Bytes32 {
        AugmentedConditionArgs::<TreeHash, TreeHash>::new(
            AssertSecondsAbsolute::new(self.seconds).into(),
            self.receiver_puzzle_hash.into(),
        )
        .curry_tree_hash()
        .into()
    }

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

    pub fn push_through_path_hash(&self) -> Bytes32 {
        self.push_through_path().tree_hash().into()
    }

    pub fn into_1_of_n(&self) -> P2OneOfManyLayer {
        P2OneOfManyLayer::new(self.merkle_tree().root())
    }

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
    use chia_protocol::Coin;
    use chia_sdk_test::{expect_spend, Simulator};
    use clvm_traits::{clvm_list, ToClvm};
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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        clawback.recover_coin_spend(&mut ctx, clawback_coin, &p2_alice, Conditions::new())?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(sim
                .coin_state(Coin::new(clawback_coin.coin_id(), alice.puzzle_hash, 1).coin_id())
                .is_some());
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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        clawback.force_coin_spend(&mut ctx, clawback_coin, &p2_alice, Conditions::new())?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(sim
                .coin_state(Coin::new(clawback_coin.coin_id(), bob.puzzle_hash, 1).coin_id())
                .is_some());
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
            assert!(sim
                .coin_state(Coin::new(clawback_coin.coin_id(), bob.puzzle_hash, 1).coin_id())
                .is_some());
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
            assert!(sim
                .coin_state(Coin::new(clawback_coin.coin_id(), bob.puzzle_hash, 1).coin_id())
                .is_some());
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
        let (issue_cat, cats) = Cat::issue_with_coin(
            &mut ctx,
            alice.coin.coin_id(),
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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let clawback_spend = clawback.recover_spend(&mut ctx, &p2_alice, Conditions::new())?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(clawback_cat, clawback_spend)])?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(sim
                .coin_state(clawback_cat.child(alice.puzzle_hash, 1).coin.coin_id())
                .is_some());
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
        let (issue_cat, cats) = Cat::issue_with_coin(
            &mut ctx,
            alice.coin.coin_id(),
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

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let clawback_spend = clawback.force_spend(&mut ctx, &p2_alice, Conditions::new())?;
        Cat::spend_all(&mut ctx, &[CatSpend::new(clawback_cat, clawback_spend)])?;

        expect_spend(sim.spend_coins(ctx.take(), &[alice.sk]), !after_expiration);

        if !after_expiration {
            assert!(sim
                .coin_state(clawback_cat.child(bob.puzzle_hash, 1).coin.coin_id())
                .is_some());
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
        let (issue_cat, cats) = Cat::issue_with_coin(
            &mut ctx,
            alice.coin.coin_id(),
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
            assert!(sim
                .coin_state(clawback_cat.child(bob.puzzle_hash, 1).coin.coin_id())
                .is_some());
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
        let (issue_cat, cats) = Cat::issue_with_coin(
            &mut ctx,
            alice.coin.coin_id(),
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
            assert!(sim
                .coin_state(clawback_cat.child(bob.puzzle_hash, 1).coin.coin_id())
                .is_some());
        }

        Ok(())
    }
}
