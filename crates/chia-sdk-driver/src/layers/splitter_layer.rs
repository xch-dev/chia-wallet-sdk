use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod, announcement_id,
    puzzles::{SPLITTER_PUZZLE_HASH, SplitterArgs, SplitterShare, SplitterSolution},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The amount a recipient receives when a [`SplitterLayer`] coin is spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitterPayout {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

/// The splitter [`Layer`] is the CHIP-0008 puzzle, which anyone can spend to divide a coin between
/// the recipients of a payout scheme. The scheme is curried in, so it can't change once funds are
/// sent to the puzzle, and the order of the recipients is part of the puzzle hash.
///
/// The puzzle reissues the full amount of the coin, so any fee must come from another coin. That
/// spend can be tied to this one by asserting [`SplitterLayer::announcement_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct SplitterLayer {
    shares: Vec<SplitterShare>,
    total_shares: u64,
}

impl SplitterLayer {
    /// Validates the payout scheme, since the puzzle itself can't reject one until it's spent.
    ///
    /// There must be at least one recipient, every share must be nonzero, and the shares must sum
    /// to at most [`u64::MAX`]. Puzzle hashes must also be unique, because two recipients being
    /// paid the same amount would create the same coin twice, making the spend invalid.
    ///
    /// The number of recipients isn't limited here since the cost also depends on the amounts and
    /// asset being split, but no more than around 2,300 can fit in a mempool transaction.
    pub fn new(shares: Vec<SplitterShare>) -> Result<Self, DriverError> {
        if shares.is_empty() {
            return Err(DriverError::InvalidSplitter);
        }

        let mut puzzle_hashes = HashSet::with_capacity(shares.len());
        let mut total_shares = 0u64;

        for share in &shares {
            if share.share == 0 || !puzzle_hashes.insert(share.puzzle_hash) {
                return Err(DriverError::InvalidSplitter);
            }

            total_shares = total_shares
                .checked_add(share.share)
                .ok_or(DriverError::InvalidSplitter)?;
        }

        Ok(Self {
            shares,
            total_shares,
        })
    }

    pub fn shares(&self) -> &[SplitterShare] {
        &self.shares
    }

    pub fn total_shares(&self) -> u64 {
        self.total_shares
    }

    /// Calculates what each recipient receives from a coin of the given amount. Amounts are
    /// rounded down, and the last recipient also receives the remainder.
    pub fn payouts(&self, amount: u64) -> Vec<SplitterPayout> {
        let mut remaining = amount;

        self.shares
            .iter()
            .enumerate()
            .map(|(index, share)| {
                let payout = if index == self.shares.len() - 1 {
                    remaining
                } else {
                    let payout = u128::from(amount) * u128::from(share.share)
                        / u128::from(self.total_shares);
                    u64::try_from(payout).expect("a share can't exceed the total")
                };

                remaining -= payout;

                SplitterPayout {
                    puzzle_hash: share.puzzle_hash,
                    amount: payout,
                }
            })
            .collect()
    }

    /// The coin announcement made when the coin with the given ID is split.
    pub fn announcement_id(coin_id: Bytes32) -> Bytes32 {
        announcement_id(coin_id, [])
    }

    /// Creates the spend of this puzzle for a coin of the given amount.
    pub fn inner_spend(&self, ctx: &mut SpendContext, amount: u64) -> Result<Spend, DriverError> {
        self.construct_spend(ctx, SplitterSolution::new(amount, self.total_shares))
    }
}

impl Layer for SplitterLayer {
    type Solution = SplitterSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != SPLITTER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = SplitterArgs::from_clvm(allocator, puzzle.args)?;
        let layer = Self::new(args.payout_scheme)?;

        // Non-canonical integers and trailing values would otherwise be lost when reserialized.
        if layer.tree_hash() != puzzle.curried_puzzle_hash {
            return Err(DriverError::InvalidSplitter);
        }

        Ok(Some(layer))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(SplitterSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(SplitterArgs::new(self.shares.clone()))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for SplitterLayer {
    fn tree_hash(&self) -> TreeHash {
        SplitterArgs::new(self.shares.clone()).curry_tree_hash()
    }
}

#[cfg(test)]
mod tests {
    use chia_consensus::validation_error::ErrorCode;
    use chia_protocol::{Bytes, Coin};
    use chia_sdk_test::{Simulator, SimulatorError};
    use chia_sdk_types::{Condition, Conditions, puzzles::P2OneOfManyArgs};
    use clvm_traits::{ToClvm, clvm_curried_args};
    use clvm_utils::CurriedProgram;
    use hex_literal::hex;
    use rstest::rstest;

    use crate::{Cat, CatSpend, StandardLayer};

    use super::*;

    const F00D: Bytes32 = Bytes32::new([0xf0; 32]);
    const BEEF: Bytes32 = Bytes32::new([0xbe; 32]);
    const CAFE: Bytes32 = Bytes32::new([0xca; 32]);

    fn splitter(shares: &[(Bytes32, u64)]) -> anyhow::Result<SplitterLayer> {
        Ok(SplitterLayer::new(
            shares
                .iter()
                .map(|&(puzzle_hash, share)| SplitterShare::new(puzzle_hash, share))
                .collect(),
        )?)
    }

    fn amounts(splitter: &SplitterLayer, amount: u64) -> Vec<u64> {
        splitter
            .payouts(amount)
            .iter()
            .map(|payout| payout.amount)
            .collect()
    }

    #[rstest]
    #[case(&[(F00D, 5000), (BEEF, 5000)], 10_000, &[5000, 5000])]
    #[case(&[(F00D, 5000), (BEEF, 5000)], 10_001, &[5000, 5001])]
    #[case(&[(F00D, 5000), (BEEF, 5000)], 1, &[0, 1])]
    #[case(&[(F00D, 80), (BEEF, 20)], 1003, &[802, 201])]
    #[case(&[(F00D, 1), (BEEF, 1), (CAFE, 1)], 100, &[33, 33, 34])]
    #[case(&[(F00D, 1)], 7, &[7])]
    #[case(&[(F00D, u64::MAX - 1), (BEEF, 1)], u64::MAX, &[u64::MAX - 1, 1])]
    fn test_payouts_match_puzzle(
        #[case] shares: &[(Bytes32, u64)],
        #[case] amount: u64,
        #[case] expected: &[u64],
    ) -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let splitter = splitter(shares)?;
        assert_eq!(amounts(&splitter, amount), expected);

        let spend = splitter.inner_spend(ctx, amount)?;
        let output = ctx.run(spend.puzzle, spend.solution)?;
        let conditions: Vec<Condition> = ctx.extract(output)?;

        let announcement = conditions[0]
            .clone()
            .into_create_coin_announcement()
            .expect("the first condition is an announcement");
        assert!(announcement.message.is_empty());
        assert_eq!(
            conditions[1]
                .clone()
                .into_assert_my_amount()
                .map(|c| c.amount),
            Some(amount)
        );

        let create_coins: Vec<_> = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .map(|create_coin| (create_coin.puzzle_hash, create_coin.amount))
            .collect();

        let payouts: Vec<_> = splitter
            .payouts(amount)
            .into_iter()
            .map(|payout| (payout.puzzle_hash, payout.amount))
            .collect();

        assert_eq!(create_coins, payouts);

        Ok(())
    }

    #[rstest]
    #[case::empty(&[])]
    #[case::zero_share(&[(F00D, 1), (BEEF, 0)])]
    #[case::duplicate_puzzle_hash(&[(F00D, 1), (F00D, 1)])]
    #[case::total_overflow(&[(F00D, u64::MAX), (BEEF, 1)])]
    fn test_invalid_schemes(#[case] shares: &[(Bytes32, u64)]) {
        assert!(matches!(
            splitter(shares).map_err(anyhow::Error::downcast::<DriverError>),
            Err(Ok(DriverError::InvalidSplitter))
        ));
    }

    #[test]
    fn test_wrong_total_shares_is_rejected() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let splitter = splitter(&[(F00D, 80), (BEEF, 20)])?;
        let puzzle = splitter.construct_puzzle(ctx)?;
        let solution = ctx.alloc(&SplitterSolution::new(100, 99))?;

        assert!(ctx.run(puzzle, solution).is_err());

        Ok(())
    }

    #[test]
    fn test_spend_xch() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let splitter = splitter(&[(F00D, 80), (BEEF, 20)])?;
        let puzzle_hash = splitter.tree_hash().into();

        let alice = sim.bls(1003);
        let alice_p2 = StandardLayer::new(alice.pk);
        let memos = ctx.hint(puzzle_hash)?;
        alice_p2.spend(
            ctx,
            alice.coin,
            Conditions::new().create_coin(puzzle_hash, 1003, memos),
        )?;

        let coin = Coin::new(alice.coin.coin_id(), puzzle_hash, 1003);
        let spend = splitter.inner_spend(ctx, coin.amount)?;
        ctx.spend(coin, spend)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let f00d = Coin::new(coin.coin_id(), F00D, 802);
        let beef = Coin::new(coin.coin_id(), BEEF, 201);
        assert!(sim.coin_state(f00d.coin_id()).is_some());
        assert!(sim.coin_state(beef.coin_id()).is_some());
        assert_eq!(sim.hinted_coins(F00D), [f00d.coin_id()]);
        assert_eq!(sim.hinted_coins(BEEF), [beef.coin_id()]);

        Ok(())
    }

    #[test]
    fn test_spend_with_fee_asserts_announcement() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let splitter = splitter(&[(F00D, 1), (BEEF, 1)])?;
        let puzzle_hash = splitter.tree_hash().into();

        let alice = sim.bls(101);
        let alice_p2 = StandardLayer::new(alice.pk);
        let coin = Coin::new(alice.coin.coin_id(), puzzle_hash, 100);

        let memos = ctx.hint(puzzle_hash)?;
        alice_p2.spend(
            ctx,
            alice.coin,
            Conditions::new()
                .create_coin(puzzle_hash, 100, memos)
                .reserve_fee(1)
                .assert_coin_announcement(SplitterLayer::announcement_id(coin.coin_id())),
        )?;
        let spend = splitter.inner_spend(ctx, coin.amount)?;
        ctx.spend(coin, spend)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert!(sim.coin_state(coin.coin_id()).is_some());

        Ok(())
    }

    #[test]
    fn test_spend_cat() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let splitter = splitter(&[(F00D, 5000), (BEEF, 5000)])?;
        let puzzle_hash = splitter.tree_hash().into();

        let alice = sim.bls(10_001);
        let alice_p2 = StandardLayer::new(alice.pk);

        let memos = ctx.hint(puzzle_hash)?;
        let (issue_cat, cats) = Cat::single_issuance(
            ctx,
            alice.coin.coin_id(),
            None,
            10_001,
            Conditions::new().create_coin(puzzle_hash, 10_001, memos),
        )?;
        alice_p2.spend(ctx, alice.coin, issue_cat)?;

        let spend = splitter.inner_spend(ctx, cats[0].coin.amount)?;
        let children = Cat::spend_all(ctx, &[CatSpend::new(cats[0], spend)])?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(children.len(), 2);
        assert_eq!(children[0].info.p2_puzzle_hash, F00D);
        assert_eq!(children[0].coin.amount, 5000);
        assert_eq!(children[1].info.p2_puzzle_hash, BEEF);
        assert_eq!(children[1].coin.amount, 5001);

        for child in children {
            assert!(sim.coin_state(child.coin.coin_id()).is_some());
        }

        Ok(())
    }

    /// Spends a splitter with the given number of recipients in a transaction of its own, which is
    /// validated against the mempool's cost limit of half a block.
    fn spend_recipients(
        recipients: u64,
        share: u64,
        amount_each: u64,
        cat: bool,
    ) -> anyhow::Result<Result<(), SimulatorError>> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let splitter = SplitterLayer::new(
            (0..recipients)
                .map(|index| {
                    let mut puzzle_hash = [0xff; 32];
                    puzzle_hash[..8].copy_from_slice(&index.to_be_bytes());
                    SplitterShare::new(puzzle_hash.into(), share)
                })
                .collect(),
        )?;
        let puzzle_hash = splitter.tree_hash().into();
        let amount = recipients * amount_each;

        if cat {
            let alice = sim.bls(amount);
            let alice_p2 = StandardLayer::new(alice.pk);

            let memos = ctx.hint(puzzle_hash)?;
            let (issue_cat, cats) = Cat::single_issuance(
                ctx,
                alice.coin.coin_id(),
                None,
                amount,
                Conditions::new().create_coin(puzzle_hash, amount, memos),
            )?;
            alice_p2.spend(ctx, alice.coin, issue_cat)?;
            sim.spend_coins(ctx.take(), &[alice.sk])?;

            let spend = splitter.inner_spend(ctx, cats[0].coin.amount)?;
            Cat::spend_all(ctx, &[CatSpend::new(cats[0], spend)])?;
        } else {
            let coin = sim.new_coin(puzzle_hash, amount);
            let spend = splitter.inner_spend(ctx, coin.amount)?;
            ctx.spend(coin, spend)?;
        }

        Ok(sim.spend_coins(ctx.take(), &[]).map(|_| ()))
    }

    // The limit depends on the size of the shares and amounts, as well as the asset being split.
    // These recipient counts leave no room for anything else, such as a spend that pays a fee.
    #[rstest]
    #[case::xch_small_values(2428, 1, 1000, false)]
    #[case::xch_large_values(2389, 1_000_000, 1_000_000_000_000, false)]
    #[case::cat_small_values(2369, 1, 1000, true)]
    #[case::cat_large_values(2332, 1_000_000, 1_000_000_000_000, true)]
    fn test_max_recipients_in_mempool_transaction(
        #[case] max_recipients: u64,
        #[case] share: u64,
        #[case] amount_each: u64,
        #[case] cat: bool,
    ) -> anyhow::Result<()> {
        spend_recipients(max_recipients, share, amount_each, cat)??;

        assert!(matches!(
            spend_recipients(max_recipients + 1, share, amount_each, cat)?,
            Err(SimulatorError::Validation(ErrorCode::CostExceeded))
        ));

        Ok(())
    }

    #[test]
    fn test_parse() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let splitter = splitter(&[(F00D, 80), (BEEF, 20)])?;
        let puzzle = splitter.construct_puzzle(ctx)?;
        let puzzle = Puzzle::parse(ctx, puzzle);
        assert_eq!(SplitterLayer::parse_puzzle(ctx, puzzle)?, Some(splitter));

        let solution = ctx.alloc(&SplitterSolution::new(100, 100))?;
        assert_eq!(
            SplitterLayer::parse_solution(ctx, solution)?,
            SplitterSolution::new(100, 100)
        );

        Ok(())
    }

    #[test]
    fn test_parse_other_puzzles() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let raw = ctx.alloc(&1)?;
        let raw = Puzzle::parse(ctx, raw);
        assert_eq!(SplitterLayer::parse_puzzle(ctx, raw)?, None);

        let curried = ctx.curry(P2OneOfManyArgs::new(F00D))?;
        let curried = Puzzle::parse(ctx, curried);
        assert_eq!(SplitterLayer::parse_puzzle(ctx, curried)?, None);

        Ok(())
    }

    fn parse_curried<T>(
        payout_scheme: T,
    ) -> anyhow::Result<Result<Option<SplitterLayer>, DriverError>>
    where
        T: ToClvm<Allocator>,
    {
        let ctx = &mut SpendContext::new();

        let program = ctx.alloc_mod::<SplitterArgs>()?;
        let args = clvm_curried_args!(payout_scheme);
        let puzzle = ctx.alloc(&CurriedProgram { program, args })?;
        let puzzle = Puzzle::parse(ctx, puzzle);

        Ok(SplitterLayer::parse_puzzle(ctx, puzzle))
    }

    #[test]
    fn test_parse_invalid_scheme() -> anyhow::Result<()> {
        assert!(matches!(
            parse_curried(Vec::<SplitterShare>::new())?,
            Err(DriverError::InvalidSplitter)
        ));
        assert!(matches!(
            parse_curried(vec![SplitterShare::new(F00D, 0)])?,
            Err(DriverError::InvalidSplitter)
        ));
        assert!(matches!(
            parse_curried(vec![
                SplitterShare::new(F00D, 1),
                SplitterShare::new(F00D, 1),
            ])?,
            Err(DriverError::InvalidSplitter)
        ));
        assert!(matches!(
            parse_curried(vec![
                SplitterShare::new(F00D, u64::MAX),
                SplitterShare::new(BEEF, 1),
            ])?,
            Err(DriverError::InvalidSplitter)
        ));

        Ok(())
    }

    #[test]
    fn test_parse_malformed_scheme() -> anyhow::Result<()> {
        assert!(matches!(
            parse_curried(vec![(F00D, ((1, 2), ()))])?,
            Err(DriverError::FromClvm(_))
        ));

        // The puzzle ignores trailing values, but they're part of the puzzle hash.
        assert!(matches!(
            parse_curried(vec![(F00D, (80, (0, ())))])?,
            Err(DriverError::InvalidSplitter)
        ));

        Ok(())
    }

    #[test]
    fn test_parse_non_canonical_share() -> anyhow::Result<()> {
        let share = Bytes::new(vec![0x00, 0x50]);

        assert!(matches!(
            parse_curried(vec![(F00D, (share, ()))])?,
            Err(DriverError::InvalidSplitter)
        ));

        Ok(())
    }

    // The payout scheme from the example in CHIP-0008.
    #[test]
    fn test_chip_example_shares_are_relative() -> anyhow::Result<()> {
        let first = Bytes32::new(hex!(
            "3816a97150946c7745457f45a4fed6c16890637847b2df95f383e439758732d7"
        ));
        let second = Bytes32::new(hex!(
            "09731fc56bff8031dc9a16f60dcb6f6462960cd76d4b5233dd5bc21697d7fca7"
        ));

        for shares in [[80, 20], [4, 1], [8000, 2000]] {
            let splitter = splitter(&[(first, shares[0]), (second, shares[1])])?;
            assert_eq!(amounts(&splitter, 10_000), [8000, 2000]);
        }

        Ok(())
    }
}
