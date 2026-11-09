use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{MessageFlags, MessageSide, conditions::SendMessage};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

/// A message sent by the vault's puzzle to a coin id, which usually authorizes that coin's spend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultMessage {
    /// The id of the coin that must receive the message.
    pub spent_coin_id: Bytes32,
    /// The message. For a coin of the vault, it's the tree hash of the delegated puzzle the coin is authorized to
    /// run. For a CAT issuance, it's the extra delta (see [`get_extra_delta_message`](crate::get_extra_delta_message)).
    /// For a controlled vault, it's the delegated puzzle hash that its singleton member is passed. In a vault message
    /// envelope, which sends it to [`VAULT_MESSAGE_COIN_ID`](crate::VAULT_MESSAGE_COIN_ID), it's an arbitrary message
    /// being signed.
    pub data: Bytes,
}

/// Parses a `SEND_MESSAGE` condition from the vault's delegated spend into a [`VaultMessage`].
///
/// The message must be sent from the vault's puzzle to a single coin id, or
/// [`DriverError::InvalidVaultMessageFormat`] is returned.
///
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

/// A message from a vault's puzzle to the puzzle hash of a vault that it controls.
///
/// This is sent rather than a [`VaultMessage`] when the controlled vault's singleton member is fast-forwardable, so
/// that the controlled vault's spend can be fast-forwarded onto a newer coin. Any coin with the puzzle hash can receive
/// the message, so it's only allowed to be sent to a vault, whose puzzle hash commits to its launcher id and custody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPuzzleMessage {
    /// The puzzle hash of the controlled vault's coin that must receive the message.
    pub puzzle_hash: Bytes32,
    /// The message, which is the delegated puzzle hash that the controlled vault's singleton member is passed.
    pub data: Bytes,
}

/// Whether a `SEND_MESSAGE` condition is sent to a puzzle hash, in which case it's parsed with
/// [`parse_vault_puzzle_message`] rather than [`parse_vault_message`].
pub fn is_vault_puzzle_message(condition: &SendMessage<NodePtr>) -> bool {
    MessageFlags::decode(condition.mode, MessageSide::Receiver) == MessageFlags::PUZZLE
}

/// Parses a `SEND_MESSAGE` condition from the vault's delegated spend that's sent to a puzzle hash into a
/// [`VaultPuzzleMessage`].
///
/// Like [`parse_vault_message`], this expects a rigid format: the message must be sent from the vault's puzzle to a
/// single puzzle hash, or [`DriverError::InvalidVaultMessageFormat`] is returned.
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
