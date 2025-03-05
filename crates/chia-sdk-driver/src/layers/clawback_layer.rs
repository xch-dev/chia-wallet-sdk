use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{Conditions, ReceiveMessage};
use clvm_traits::FromClvm;
use clvmr::Allocator;

use crate::{DriverError, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2ClawbackLayer {
    pub sender_puzzle_hash: Bytes32,
    pub receiver_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub amount: u64,
    pub hinted: bool,
}

impl P2ClawbackLayer {
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

        let recover = &options[0];
        let force = &options[1];
        let finish = &options[2];

        if recover.len() != 3 || force.len() != 2 || finish.len() != 2 {
            return None;
        }

        let Some(ReceiveMessage {
            mode,
            message,
            data,
        }) = recover[0].as_receive_message()
        else {
            return None;
        };

        if *mode != 23 || message.as_ref() != &[1] || data.len() != 1 {
            return None;
        }

        let sender_puzzle_hash = Bytes32::from_clvm(allocator, data[0]).ok()?;

        Some(Self::new(
            sender_puzzle_hash,
            receiver_puzzle_hash,
            seconds,
            amount,
            hinted,
        ))
    }

    /// 1. The sender authorized the clawback
    /// 2. Send the coin back to the sender
    /// 3. The clawback hasn't expired yet
    pub fn recover_conditions(self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
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

    /// 1. The sender authorized the payment
    /// 2. Send the coin to the receiver early
    pub fn force_conditions(self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
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

    /// 1. Send the coin to the receiver
    /// 2. The clawback has expired
    pub fn finish_conditions(self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
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

    pub fn into_options(self, ctx: &mut SpendContext) -> Result<Vec<Conditions>, DriverError> {
        Ok(vec![
            self.recover_conditions(ctx)?,
            self.force_conditions(ctx)?,
            self.finish_conditions(ctx)?,
        ])
    }
}
