use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{MessageFlags, MessageSide, conditions::SendMessage};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultMessage {
    pub spent_coin_id: Bytes32,
    pub data: Bytes,
}

/// We intentionally expect a pretty rigid format here, out of an abundance of caution.
/// If the message format is violated, we return an error. This can be expanded in the future if needed.
pub fn parse_vault_message(
    allocator: &Allocator,
    condition: SendMessage<NodePtr>,
) -> Result<VaultMessage, DriverError> {
    let sender = MessageFlags::decode(condition.mode, MessageSide::Sender);
    let receiver = MessageFlags::decode(condition.mode, MessageSide::Receiver);

    if sender != MessageFlags::PUZZLE || receiver != MessageFlags::COIN || condition.data.len() != 1
    {
        return Err(DriverError::InvalidVaultMessageFormat);
    }

    let coin_id = Bytes32::from_clvm(allocator, condition.data[0])?;

    Ok(VaultMessage {
        spent_coin_id: coin_id,
        data: condition.message,
    })
}

/// A message from a vault to the puzzle hash of a vault that it controls, which is sent rather than a [`VaultMessage`]
/// when the controlled vault's singleton member is fast-forwardable, so that the controlled vault's spend can be
/// fast-forwarded onto a newer coin. Any coin with the puzzle hash can receive the message, so it's only allowed to be
/// sent to a vault, whose puzzle hash commits to its launcher id and custody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPuzzleMessage {
    pub puzzle_hash: Bytes32,
    pub data: Bytes,
}

/// Whether a message is sent to a puzzle hash, which is parsed with [`parse_vault_puzzle_message`] rather than
/// [`parse_vault_message`].
pub fn is_vault_puzzle_message(condition: &SendMessage<NodePtr>) -> bool {
    MessageFlags::decode(condition.mode, MessageSide::Receiver) == MessageFlags::PUZZLE
}

/// Like [`parse_vault_message`], this expects a rigid format, and returns an error if it's violated.
pub fn parse_vault_puzzle_message(
    allocator: &Allocator,
    condition: SendMessage<NodePtr>,
) -> Result<VaultPuzzleMessage, DriverError> {
    let sender = MessageFlags::decode(condition.mode, MessageSide::Sender);
    let receiver = MessageFlags::decode(condition.mode, MessageSide::Receiver);

    if sender != MessageFlags::PUZZLE
        || receiver != MessageFlags::PUZZLE
        || condition.data.len() != 1
    {
        return Err(DriverError::InvalidVaultMessageFormat);
    }

    let puzzle_hash = Bytes32::from_clvm(allocator, condition.data[0])?;

    Ok(VaultPuzzleMessage {
        puzzle_hash,
        data: condition.message,
    })
}
