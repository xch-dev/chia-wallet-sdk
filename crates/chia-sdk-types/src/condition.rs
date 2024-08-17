use std::collections::HashSet;

use chia_protocol::{Coin, CoinSpend};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{
    reduction::{EvalErr, Reduction},
    Allocator, NodePtr,
};

mod agg_sig;
mod announcements;
mod coin_info;
mod concurrent;
mod output;
mod puzzles;
mod time;

pub use agg_sig::*;
pub use announcements::*;
pub use coin_info::*;
pub use concurrent::*;
pub use output::*;
pub use puzzles::*;
pub use time::*;

use crate::ConditionError;

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub enum Condition<T = NodePtr> {
    Remark(Remark<T>),
    AggSig(AggSig),
    CreateCoin(CreateCoin),
    ReserveFee(ReserveFee),
    CreateCoinAnnouncement(CreateCoinAnnouncement),
    AssertCoinAnnouncement(AssertCoinAnnouncement),
    CreatePuzzleAnnouncement(CreatePuzzleAnnouncement),
    AssertPuzzleAnnouncement(AssertPuzzleAnnouncement),
    AssertConcurrentSpend(AssertConcurrentSpend),
    AssertConcurrentPuzzle(AssertConcurrentPuzzle),
    AssertMyCoinId(AssertMyCoinId),
    AssertMyParentId(AssertMyParentId),
    AssertMyPuzzleHash(AssertMyPuzzleHash),
    AssertMyAmount(AssertMyAmount),
    AssertMyBirthSeconds(AssertMyBirthSeconds),
    AssertMyBirthHeight(AssertMyBirthHeight),
    AssertEphemeral(AssertEphemeral),
    AssertSecondsRelative(AssertSecondsRelative),
    AssertSecondsAbsolute(AssertSecondsAbsolute),
    AssertHeightRelative(AssertHeightRelative),
    AssertHeightAbsolute(AssertHeightAbsolute),
    AssertBeforeSecondsRelative(AssertBeforeSecondsRelative),
    AssertBeforeSecondsAbsolute(AssertBeforeSecondsAbsolute),
    AssertBeforeHeightRelative(AssertBeforeHeightRelative),
    AssertBeforeHeightAbsolute(AssertBeforeHeightAbsolute),
    Softfork(Softfork<T>),
    Other(T),
}

macro_rules! into {
    ( $name:ident -> $condition:ident $( < $generic:ident > )? ) => {
        pub fn $name(self) -> Option<$condition $( <$generic> )? > {
            match self {
                Condition::$condition(condition) => Some(condition),
                _ => None,
            }
        }
    };
}

impl<T> Condition<T> {
    into!(into_remark -> Remark<T>);
    into!(into_agg_sig -> AggSig);
    into!(into_create_coin -> CreateCoin);
    into!(into_reserve_fee -> ReserveFee);
    into!(into_create_coin_announcement -> CreateCoinAnnouncement);
    into!(into_assert_coin_announcement -> AssertCoinAnnouncement);
    into!(into_create_puzzle_announcement -> CreatePuzzleAnnouncement);
    into!(into_assert_puzzle_announcement -> AssertPuzzleAnnouncement);
    into!(into_assert_concurrent_spend -> AssertConcurrentSpend);
    into!(into_assert_concurrent_puzzle -> AssertConcurrentPuzzle);
    into!(into_assert_my_coin_id -> AssertMyCoinId);
    into!(into_assert_my_parent_id -> AssertMyParentId);
    into!(into_assert_my_puzzle_hash -> AssertMyPuzzleHash);
    into!(into_assert_my_amount -> AssertMyAmount);
    into!(into_assert_my_birth_seconds -> AssertMyBirthSeconds);
    into!(into_assert_my_birth_height -> AssertMyBirthHeight);
    into!(into_assert_ephemeral -> AssertEphemeral);
    into!(into_assert_seconds_relative -> AssertSecondsRelative);
    into!(into_assert_seconds_absolute -> AssertSecondsAbsolute);
    into!(into_assert_height_relative -> AssertHeightRelative);
    into!(into_assert_height_absolute -> AssertHeightAbsolute);
    into!(into_assert_before_seconds_relative -> AssertBeforeSecondsRelative);
    into!(into_assert_before_seconds_absolute -> AssertBeforeSecondsAbsolute);
    into!(into_assert_before_height_relative -> AssertBeforeHeightRelative);
    into!(into_assert_before_height_absolute -> AssertBeforeHeightAbsolute);
    into!(into_softfork -> Softfork<T>);

    pub fn into_other(self) -> Option<T> {
        match self {
            Condition::Other(ptr) => Some(ptr),
            _ => None,
        }
    }
}

pub fn run_puzzle(
    allocator: &mut Allocator,
    puzzle: NodePtr,
    solution: NodePtr,
) -> Result<NodePtr, EvalErr> {
    let Reduction(_cost, output) = clvmr::run_program(
        allocator,
        &clvmr::ChiaDialect::new(0),
        puzzle,
        solution,
        11_000_000_000,
    )?;
    Ok(output)
}

pub fn non_ephemeral_coins(coin_spends: &[CoinSpend]) -> Result<Vec<Coin>, ConditionError> {
    let mut allocator = Allocator::new();
    let mut created_coins = HashSet::new();

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
        let solution = coin_spend.solution.to_clvm(&mut allocator)?;
        let output = run_puzzle(&mut allocator, puzzle, solution)?;
        let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;

        for condition in conditions {
            if let Condition::CreateCoin(create_coin) = condition {
                created_coins.insert(Coin::new(
                    coin_spend.coin.coin_id(),
                    create_coin.puzzle_hash,
                    create_coin.amount,
                ));
            }
        }
    }

    let non_ephemeral = coin_spends
        .iter()
        .map(|cs| cs.coin)
        .filter(|coin| !created_coins.contains(coin))
        .collect();

    Ok(non_ephemeral)
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_protocol::{Bytes32, Program};
    use clvm_traits::{FromClvm, ToClvm};

    #[test]
    fn test_non_ephemeral_coins() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let coins: Vec<Coin> = (0..3)
            .map(|amount| Coin::new(Bytes32::default(), Bytes32::default(), amount))
            .collect();

        let puzzle = 1.to_clvm(&mut allocator)?;
        let puzzle_reveal = Program::from_clvm(&allocator, puzzle)?;
        let identity_solution = Program::from_clvm(&allocator, NodePtr::NIL)?;

        let mut coin_spends = Vec::new();

        for i in 0..3 {
            let create_coin = CreateCoin::new(Bytes32::new([i; 32]), u64::from(i));
            let solution = [&create_coin].to_clvm(&mut allocator)?;

            coin_spends.push(CoinSpend::new(
                Coin::new(
                    coins[i as usize].coin_id(),
                    create_coin.puzzle_hash,
                    create_coin.amount,
                ),
                puzzle_reveal.clone(),
                identity_solution.clone(),
            ));

            coin_spends.push(CoinSpend::new(
                coins[i as usize],
                puzzle_reveal.clone(),
                Program::from_clvm(&allocator, solution)?,
            ));
        }

        let non_ephemeral_coins = non_ephemeral_coins(&coin_spends)?;
        assert_eq!(non_ephemeral_coins, coins);

        Ok(())
    }
}
