use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::Conditions;
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

use super::{P2ConditionsOptionsLayer, P2ConditionsOptionsSolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClawbackLayer {
    pub sender_puzzle_hash: Bytes32,
    pub receiver_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub amount: u64,
    pub hinted: bool,
}

impl ClawbackLayer {
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
            || recover.hinted != force.hinted
            || recover.hinted != finish.hinted
            || force.receiver_puzzle_hash != finish.receiver_puzzle_hash
        {
            return None;
        }

        Some(Self {
            sender_puzzle_hash: recover.sender_puzzle_hash,
            receiver_puzzle_hash: force.receiver_puzzle_hash,
            seconds: recover.seconds,
            amount: recover.amount,
            hinted: recover.hinted,
        })
    }

    pub fn into_options(self, ctx: &mut SpendContext) -> Result<Vec<Conditions>, DriverError> {
        Ok(vec![
            RecoverOption {
                sender_puzzle_hash: self.sender_puzzle_hash,
                amount: self.amount,
                seconds: self.seconds,
                hinted: self.hinted,
            }
            .conditions(ctx)?,
            ForceOption {
                sender_puzzle_hash: self.sender_puzzle_hash,
                receiver_puzzle_hash: self.receiver_puzzle_hash,
                amount: self.amount,
                hinted: self.hinted,
            }
            .conditions(ctx)?,
            FinishOption {
                receiver_puzzle_hash: self.receiver_puzzle_hash,
                amount: self.amount,
                seconds: self.seconds,
                hinted: self.hinted,
            }
            .conditions(ctx)?,
        ])
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
        let options = self.into_options(ctx)?;
        P2ConditionsOptionsLayer::new(options).construct_puzzle(ctx)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        P2ConditionsOptionsLayer::new(self.into_options(ctx)?).construct_solution(
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

/// 1. The sender authorized the clawback
/// 2. Send the coin back to the sender
/// 3. The clawback hasn't expired yet
struct RecoverOption {
    sender_puzzle_hash: Bytes32,
    amount: u64,
    seconds: u64,
    hinted: bool,
}

impl RecoverOption {
    fn conditions(self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
        Ok(Conditions::new()
            .receive_message(
                23,
                vec![1].into(),
                vec![ctx.alloc(&self.sender_puzzle_hash)?],
            )
            .create_coin(
                self.sender_puzzle_hash,
                self.amount,
                if self.hinted {
                    Some(ctx.hint(self.sender_puzzle_hash)?)
                } else {
                    None
                },
            )
            .assert_before_seconds_absolute(self.seconds))
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

        let hinted = if let Some(memos) = create_coin.memos {
            let [hint] = <[Bytes32; 1]>::from_clvm(allocator, memos.value).ok()?;

            if hint != sender_puzzle_hash {
                return None;
            }

            true
        } else {
            false
        };

        let assert_before_seconds_absolute = conditions[2].as_assert_before_seconds_absolute()?;

        let seconds = assert_before_seconds_absolute.seconds;

        Some(Self {
            sender_puzzle_hash,
            amount,
            seconds,
            hinted,
        })
    }
}

/// 1. The sender authorized the payment
/// 2. Send the coin to the receiver early
struct ForceOption {
    sender_puzzle_hash: Bytes32,
    receiver_puzzle_hash: Bytes32,
    amount: u64,
    hinted: bool,
}

impl ForceOption {
    fn conditions(self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
        Ok(Conditions::new()
            .receive_message(
                23,
                Bytes::default(),
                vec![ctx.alloc(&self.sender_puzzle_hash)?],
            )
            .create_coin(
                self.receiver_puzzle_hash,
                self.amount,
                if self.hinted {
                    Some(ctx.hint(self.receiver_puzzle_hash)?)
                } else {
                    None
                },
            ))
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

        let hinted = if let Some(memos) = create_coin.memos {
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
            hinted,
        })
    }
}

/// 1. Send the coin to the receiver
/// 2. The clawback has expired
struct FinishOption {
    receiver_puzzle_hash: Bytes32,
    amount: u64,
    seconds: u64,
    hinted: bool,
}

impl FinishOption {
    fn conditions(self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
        Ok(Conditions::new()
            .create_coin(
                self.receiver_puzzle_hash,
                self.amount,
                if self.hinted {
                    Some(ctx.hint(self.receiver_puzzle_hash)?)
                } else {
                    None
                },
            )
            .assert_seconds_absolute(self.seconds))
    }

    fn parse(allocator: &Allocator, conditions: &Conditions) -> Option<Self> {
        if conditions.len() != 2 {
            return None;
        }

        let create_coin = conditions[0].as_create_coin()?;

        let receiver_puzzle_hash = create_coin.puzzle_hash;
        let amount = create_coin.amount;

        let hinted = if let Some(memos) = create_coin.memos {
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
            hinted,
        })
    }
}
