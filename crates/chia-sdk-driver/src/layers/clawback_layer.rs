use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{Conditions, Memos, Mod, SendMessage};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

use super::{P2ConditionsOptionsArgs, P2ConditionsOptionsLayer, P2ConditionsOptionsSolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClawbackLayer {
    pub sender_puzzle_hash: Bytes32,
    pub receiver_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub amount: u64,
    pub receiver_hinted: bool,
}

impl ClawbackLayer {
    pub fn new(
        sender_puzzle_hash: Bytes32,
        receiver_puzzle_hash: Bytes32,
        seconds: u64,
        amount: u64,
        receiver_hinted: bool,
    ) -> Self {
        Self {
            sender_puzzle_hash,
            receiver_puzzle_hash,
            seconds,
            amount,
            receiver_hinted,
        }
    }

    pub fn from_memo(
        allocator: &Allocator,
        memos: NodePtr,
        receiver_puzzle_hash: Bytes32,
        amount: u64,
        receiver_hinted: bool,
        expected_puzzle_hash: Bytes32,
    ) -> Option<Self> {
        let (sender_puzzle_hash, seconds) = <(Bytes32, u64)>::from_clvm(allocator, memos).ok()?;

        let clawback = Self {
            sender_puzzle_hash,
            receiver_puzzle_hash,
            seconds,
            amount,
            receiver_hinted,
        };

        if clawback.tree_hash() != expected_puzzle_hash.into() {
            return None;
        }

        Some(clawback)
    }

    pub fn memo(&self) -> (Bytes32, u64) {
        (self.sender_puzzle_hash, self.seconds)
    }

    pub fn from_options(allocator: &Allocator, options: &[Conditions]) -> Option<Self> {
        if options.len() != 3 {
            return None;
        }

        let recover = RecoverOption::parse(allocator, &options[0])?;
        let force = ForceOption::parse(allocator, &options[1])?;
        let finish = FinishOption::parse(allocator, &options[2])?;

        if recover.sender_puzzle_hash != force.sender_puzzle_hash
            || recover.amount != force.amount
            || recover.amount != finish.amount
            || recover.seconds != finish.seconds
            || force.receiver_hinted != finish.receiver_hinted
            || force.receiver_puzzle_hash != finish.receiver_puzzle_hash
        {
            return None;
        }

        Some(Self {
            sender_puzzle_hash: recover.sender_puzzle_hash,
            receiver_puzzle_hash: force.receiver_puzzle_hash,
            seconds: recover.seconds,
            amount: recover.amount,
            receiver_hinted: force.receiver_hinted,
        })
    }

    pub fn into_options(self) -> Vec<Conditions<Bytes32>> {
        vec![
            RecoverOption {
                sender_puzzle_hash: self.sender_puzzle_hash,
                amount: self.amount,
                seconds: self.seconds,
            }
            .conditions(),
            ForceOption {
                sender_puzzle_hash: self.sender_puzzle_hash,
                receiver_puzzle_hash: self.receiver_puzzle_hash,
                amount: self.amount,
                receiver_hinted: self.receiver_hinted,
            }
            .conditions(),
            FinishOption {
                receiver_puzzle_hash: self.receiver_puzzle_hash,
                amount: self.amount,
                seconds: self.seconds,
                receiver_hinted: self.receiver_hinted,
            }
            .conditions(),
        ]
    }

    pub fn recover_message(
        &self,
        ctx: &mut SpendContext,
        coin_id: Bytes32,
    ) -> Result<SendMessage<NodePtr>, DriverError> {
        Ok(SendMessage::new(
            23,
            vec![1].into(),
            vec![ctx.alloc(&coin_id)?],
        ))
    }

    pub fn force_message(
        &self,
        ctx: &mut SpendContext,
        coin_id: Bytes32,
    ) -> Result<SendMessage<NodePtr>, DriverError> {
        Ok(SendMessage::new(
            23,
            Bytes::default(),
            vec![ctx.alloc(&coin_id)?],
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClawbackSolution {
    Recover,
    Force,
    Finish,
}

impl Layer for ClawbackLayer {
    type Solution = ClawbackSolution;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let options = self.into_options();
        P2ConditionsOptionsLayer::new(options).construct_puzzle(ctx)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        P2ConditionsOptionsLayer::new(self.into_options()).construct_solution(
            ctx,
            P2ConditionsOptionsSolution {
                option: match solution {
                    ClawbackSolution::Recover => 0,
                    ClawbackSolution::Force => 1,
                    ClawbackSolution::Finish => 2,
                },
            },
        )
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(layer) = P2ConditionsOptionsLayer::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };
        Ok(Self::from_options(allocator, &layer.options))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        match P2ConditionsOptionsLayer::<NodePtr>::parse_solution(allocator, solution)?.option {
            0 => Ok(ClawbackSolution::Recover),
            1 => Ok(ClawbackSolution::Force),
            2 => Ok(ClawbackSolution::Finish),
            _ => Err(DriverError::NonStandardLayer),
        }
    }
}

impl ToTreeHash for ClawbackLayer {
    fn tree_hash(&self) -> TreeHash {
        P2ConditionsOptionsArgs::new(self.into_options()).curry_tree_hash()
    }
}

/// 1. The sender authorized the clawback
/// 2. Send the coin back to the sender
/// 3. The clawback hasn't expired yet
struct RecoverOption {
    sender_puzzle_hash: Bytes32,
    amount: u64,
    seconds: u64,
}

impl RecoverOption {
    fn conditions(self) -> Conditions<Bytes32> {
        Conditions::default()
            .receive_message(23, vec![1].into(), vec![self.sender_puzzle_hash])
            .create_coin(self.sender_puzzle_hash, self.amount, None)
            .assert_before_seconds_absolute(self.seconds)
    }

    fn parse(allocator: &Allocator, conditions: &Conditions) -> Option<Self> {
        if conditions.len() != 3 {
            return None;
        }

        let receive_message = conditions[0].as_receive_message()?;

        if receive_message.mode != 23
            || receive_message.message.as_ref() != [1]
            || receive_message.data.len() != 1
        {
            return None;
        }

        let sender_puzzle_hash = Bytes32::from_clvm(allocator, receive_message.data[0]).ok()?;

        let create_coin = conditions[1].as_create_coin()?;

        if create_coin.puzzle_hash != sender_puzzle_hash {
            return None;
        }

        let amount = create_coin.amount;

        if create_coin.memos.is_some() {
            return None;
        }

        let assert_before_seconds_absolute = conditions[2].as_assert_before_seconds_absolute()?;

        let seconds = assert_before_seconds_absolute.seconds;

        Some(Self {
            sender_puzzle_hash,
            amount,
            seconds,
        })
    }
}

/// 1. The sender authorized the payment
/// 2. Send the coin to the receiver early
struct ForceOption {
    sender_puzzle_hash: Bytes32,
    receiver_puzzle_hash: Bytes32,
    amount: u64,
    receiver_hinted: bool,
}

impl ForceOption {
    fn conditions(self) -> Conditions<Bytes32> {
        Conditions::default()
            .receive_message(23, Bytes::default(), vec![self.sender_puzzle_hash])
            .create_coin(
                self.receiver_puzzle_hash,
                self.amount,
                if self.receiver_hinted {
                    Some(Memos::new(self.receiver_puzzle_hash))
                } else {
                    None
                },
            )
    }

    fn parse(allocator: &Allocator, conditions: &Conditions) -> Option<Self> {
        if conditions.len() != 2 {
            return None;
        }

        let receive_message = conditions[0].as_receive_message()?;

        if receive_message.mode != 23
            || !receive_message.message.is_empty()
            || receive_message.data.len() != 1
        {
            return None;
        }

        let sender_puzzle_hash = Bytes32::from_clvm(allocator, receive_message.data[0]).ok()?;

        let create_coin = conditions[1].as_create_coin()?;

        let receiver_puzzle_hash = create_coin.puzzle_hash;

        let amount = create_coin.amount;

        let receiver_hinted = if let Some(memos) = create_coin.memos {
            let [hint] = <[Bytes32; 1]>::from_clvm(allocator, memos.value).ok()?;

            if hint != receiver_puzzle_hash {
                return None;
            }

            true
        } else {
            false
        };

        Some(Self {
            sender_puzzle_hash,
            receiver_puzzle_hash,
            amount,
            receiver_hinted,
        })
    }
}

/// 1. Send the coin to the receiver
/// 2. The clawback has expired
struct FinishOption {
    receiver_puzzle_hash: Bytes32,
    amount: u64,
    seconds: u64,
    receiver_hinted: bool,
}

impl FinishOption {
    fn conditions(self) -> Conditions<Bytes32> {
        Conditions::default()
            .create_coin(
                self.receiver_puzzle_hash,
                self.amount,
                if self.receiver_hinted {
                    Some(Memos::new(self.receiver_puzzle_hash))
                } else {
                    None
                },
            )
            .assert_seconds_absolute(self.seconds)
    }

    fn parse(allocator: &Allocator, conditions: &Conditions) -> Option<Self> {
        if conditions.len() != 2 {
            return None;
        }

        let create_coin = conditions[0].as_create_coin()?;

        let receiver_puzzle_hash = create_coin.puzzle_hash;
        let amount = create_coin.amount;

        let receiver_hinted = if let Some(memos) = create_coin.memos {
            let [hint] = <[Bytes32; 1]>::from_clvm(allocator, memos.value).ok()?;

            if hint != receiver_puzzle_hash {
                return None;
            }

            true
        } else {
            false
        };

        let assert_seconds_absolute = conditions[1].as_assert_seconds_absolute()?;

        let seconds = assert_seconds_absolute.seconds;

        Some(Self {
            receiver_puzzle_hash,
            amount,
            seconds,
            receiver_hinted,
        })
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_sdk_test::Simulator;
    use clvm_traits::clvm_list;
    use rstest::rstest;

    use crate::StandardLayer;

    use super::*;

    #[rstest]
    fn test_clawback_layer_recover(#[values(false, true)] hinted: bool) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);
        let p2_alice = StandardLayer::new(alice.pk);

        let bob = sim.bls(0);

        let clawback = ClawbackLayer::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, hinted);
        let clawback_puzzle_hash = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        p2_alice.spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(clawback_puzzle_hash, 1, Some(memos)),
        )?;
        let clawback_coin = Coin::new(alice.coin.coin_id(), clawback_puzzle_hash, 1);

        // Child authorizes parent
        let coin_spend =
            clawback.construct_coin_spend(&mut ctx, clawback_coin, ClawbackSolution::Recover)?;
        ctx.insert(coin_spend);

        let intermediate_coin = Coin::new(clawback_coin.coin_id(), alice.puzzle_hash, 1);
        let recover_conditions = Conditions::new()
            .create_coin(alice.puzzle_hash, 1, None)
            .with(clawback.recover_message(&mut ctx, clawback_coin.coin_id())?);
        p2_alice.spend(&mut ctx, intermediate_coin, recover_conditions)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
