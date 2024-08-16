use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::standard::{StandardArgs, StandardSolution};
use chia_sdk_types::{
    AssertBeforeHeightAbsolute, AssertBeforeHeightRelative, AssertBeforeSecondsAbsolute,
    AssertBeforeSecondsRelative, AssertCoinAnnouncement, AssertHeightAbsolute,
    AssertHeightRelative, AssertPuzzleAnnouncement, AssertSecondsAbsolute, AssertSecondsRelative,
    Condition, CreateCoin, CreateCoinAnnouncement, CreatePuzzleAnnouncement, ReserveFee,
};

use clvm_traits::{ClvmEncoder, ToClvm, ToClvmError};
use clvm_utils::CurriedProgram;
use clvmr::{sha2::Sha256, NodePtr};

use crate::{Spend, SpendContext, SpendError};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct Conditions {
    conditions: Vec<Condition>,
}

impl Conditions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn condition(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn conditions(mut self, conditions: &[Condition]) -> Self {
        self.conditions.extend_from_slice(conditions);
        self
    }

    pub fn extend(mut self, conditions: impl IntoIterator<Item = Condition>) -> Self {
        self.conditions.extend(conditions);
        self
    }

    pub fn reserve_fee(self, fee: u64) -> Self {
        self.condition(Condition::ReserveFee(ReserveFee::new(fee)))
    }

    pub fn create_coin(self, puzzle_hash: Bytes32, amount: u64) -> Self {
        self.condition(Condition::CreateCoin(CreateCoin::new(puzzle_hash, amount)))
    }

    pub fn create_hinted_coin(self, puzzle_hash: Bytes32, amount: u64, hint: Bytes32) -> Self {
        self.condition(Condition::CreateCoin(CreateCoin::with_hint(
            puzzle_hash,
            amount,
            hint,
        )))
    }

    pub fn create_coin_announcement(self, message: Bytes) -> Self {
        self.condition(Condition::CreateCoinAnnouncement(
            CreateCoinAnnouncement::new(message),
        ))
    }

    pub fn assert_raw_coin_announcement(self, announcement_id: Bytes32) -> Self {
        self.condition(Condition::AssertCoinAnnouncement(
            AssertCoinAnnouncement::new(announcement_id),
        ))
    }

    pub fn assert_coin_announcement(self, coin_id: Bytes32, message: impl AsRef<[u8]>) -> Self {
        let mut announcement_id = Sha256::new();
        announcement_id.update(coin_id);
        announcement_id.update(message);
        self.assert_raw_coin_announcement(Bytes32::new(announcement_id.finalize().into()))
    }

    pub fn create_puzzle_announcement(self, message: Bytes) -> Self {
        self.condition(Condition::CreatePuzzleAnnouncement(
            CreatePuzzleAnnouncement::new(message),
        ))
    }

    pub fn assert_raw_puzzle_announcement(self, announcement_id: Bytes32) -> Self {
        self.condition(Condition::AssertPuzzleAnnouncement(
            AssertPuzzleAnnouncement::new(announcement_id),
        ))
    }

    pub fn assert_puzzle_announcement(
        self,
        puzzle_hash: Bytes32,
        message: impl AsRef<[u8]>,
    ) -> Self {
        let mut announcement_id = Sha256::new();
        announcement_id.update(puzzle_hash);
        announcement_id.update(message);
        self.assert_raw_puzzle_announcement(Bytes32::new(announcement_id.finalize().into()))
    }

    pub fn assert_before_seconds_relative(self, seconds: u64) -> Self {
        self.condition(Condition::AssertBeforeSecondsRelative(
            AssertBeforeSecondsRelative::new(seconds),
        ))
    }

    pub fn assert_seconds_relative(self, seconds: u64) -> Self {
        self.condition(Condition::AssertSecondsRelative(
            AssertSecondsRelative::new(seconds),
        ))
    }

    pub fn assert_seconds_absolute(self, seconds: u64) -> Self {
        self.condition(Condition::AssertSecondsAbsolute(
            AssertSecondsAbsolute::new(seconds),
        ))
    }

    pub fn assert_before_seconds_absolute(self, seconds: u64) -> Self {
        self.condition(Condition::AssertBeforeSecondsAbsolute(
            AssertBeforeSecondsAbsolute::new(seconds),
        ))
    }

    pub fn assert_before_height_relative(self, height: u32) -> Self {
        self.condition(Condition::AssertBeforeHeightRelative(
            AssertBeforeHeightRelative::new(height),
        ))
    }

    pub fn assert_before_height_absolute(self, height: u32) -> Self {
        self.condition(Condition::AssertBeforeHeightAbsolute(
            AssertBeforeHeightAbsolute::new(height),
        ))
    }

    pub fn assert_height_relative(self, height: u32) -> Self {
        self.condition(Condition::AssertHeightRelative(AssertHeightRelative::new(
            height,
        )))
    }

    pub fn assert_height_absolute(self, height: u32) -> Self {
        self.condition(Condition::AssertHeightAbsolute(AssertHeightAbsolute::new(
            height,
        )))
    }

    pub fn p2_spend(
        self,
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
    ) -> Result<Spend, SpendError> {
        let standard_puzzle = ctx.standard_puzzle()?;

        let puzzle = ctx.alloc(&CurriedProgram {
            program: standard_puzzle,
            args: StandardArgs::new(synthetic_key),
        })?;

        let solution = ctx.alloc(&StandardSolution::from_conditions(self))?;

        Ok(Spend::new(puzzle, solution))
    }
}

impl AsRef<[Condition]> for Conditions {
    fn as_ref(&self) -> &[Condition] {
        &self.conditions
    }
}

impl IntoIterator for Conditions {
    type Item = Condition;
    type IntoIter = std::vec::IntoIter<Condition>;

    fn into_iter(self) -> Self::IntoIter {
        self.conditions.into_iter()
    }
}

impl<E> ToClvm<E> for Conditions
where
    E: ClvmEncoder<Node = NodePtr>,
    NodePtr: ToClvm<E>,
{
    fn to_clvm(&self, encoder: &mut E) -> Result<NodePtr, ToClvmError> {
        self.conditions.to_clvm(encoder)
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::{secret_key, test_transaction, Simulator};

    use super::*;

    #[tokio::test]
    async fn test_standard_spend() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 1).await;

        ctx.spend_p2_coin(coin, pk, Conditions::new().create_coin(puzzle_hash, 1))?;

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

        Ok(())
    }
}
