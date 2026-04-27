use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{MessageFlags, MessageSide, conditions::SendMessage};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultMessage {
    pub spent_coin_id: Bytes32,
    pub message: Bytes,
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
        message: condition.message,
    })
}
