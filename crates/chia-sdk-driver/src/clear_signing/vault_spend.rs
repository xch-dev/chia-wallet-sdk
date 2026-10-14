use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::Bytes32;
use chia_sdk_types::Condition;
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use num_bigint::BigInt;

use crate::{
    DriverError, Facts, Spend, VaultMessage, parse_delegated_spend_with_melt, parse_vault_message,
};

#[derive(Debug, Clone)]
pub struct VaultSpendSummary {
    pub child: Option<VaultOutput>,
    pub drop_coins: Vec<DropCoin>,
    pub messages: Vec<VaultMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VaultOutput {
    pub custody_hash: Bytes32,
    pub amount: u64,
}

impl VaultOutput {
    pub fn new(custody_hash: Bytes32, amount: u64) -> Self {
        Self {
            custody_hash,
            amount,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DropCoin {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

impl DropCoin {
    pub fn new(puzzle_hash: Bytes32, amount: u64) -> Self {
        Self {
            puzzle_hash,
            amount,
        }
    }
}

pub fn parse_vault_delegated_spend(
    facts: &mut Facts,
    allocator: &mut Allocator,
    delegated_spend: Spend,
) -> Result<VaultSpendSummary, DriverError> {
    let (conditions, melted) = parse_delegated_spend_with_melt(allocator, delegated_spend)?;

    let mut child = None;
    let mut drop_coins = Vec::new();
    let mut messages = Vec::new();

    for condition in conditions {
        match condition {
            Condition::AssertPuzzleAnnouncement(condition) => {
                facts.assert_puzzle_announcement(condition.announcement_id);
            }
            Condition::AssertConcurrentSpend(condition) => {
                facts.assert_spend(condition.coin_id);
            }
            Condition::AssertBeforeSecondsAbsolute(condition) => {
                facts.update_actual_expiration_time(condition.seconds);
            }
            Condition::AssertSecondsAbsolute(condition) => {
                facts.update_actual_start_time(condition.seconds);
            }
            Condition::ReserveFee(condition) => {
                facts.add_reserved_fees(condition.amount);
            }
            Condition::CreateCoin(condition) => {
                // If a child of a singleton (the vault in this case) is odd, due to the way the singleton
                // puzzle works, we know that it will be the new coin for this singleton. And because this
                // parsing is running at the delegated spend layer (i.e., inner puzzle), we know that the
                // puzzle hash is actually the custody hash, rather than the singleton's full puzzle hash.
                //
                // Note that if a CREATE_COIN condition with an odd child is not included in the delegated
                // spend, we assume that the singleton is being melted. This is technically only true if
                // the special (puzzle specific) MELT_SINGLETON condition is included, but if it's not, then
                // the transaction is invalid anyways.
                if condition.amount % 2 == 1 {
                    // The singleton layer doesn't allow more than one odd output, including the melt.
                    if melted || child.is_some() {
                        return Err(DriverError::InvalidDelegatedSpendFormat);
                    }

                    child = Some(VaultOutput {
                        custody_hash: condition.puzzle_hash,
                        amount: condition.amount,
                    });
                } else {
                    drop_coins.push(DropCoin {
                        puzzle_hash: condition.puzzle_hash,
                        amount: condition.amount,
                    });
                }
            }
            Condition::SendMessage(condition) => {
                // If the vault sends a message to a coin, we must first validate the message's format, and then ensure
                // that the corresponding coin spend is revealed and matches the delegated puzzle hash in the message.
                // These coins are considered "linked", meaning that their conditions should also be treated as fact if
                // they can be validated to be impossible to circumvent.
                let vault_message = parse_vault_message(allocator, condition)?;
                messages.push(vault_message);
            }
            // These conditions aren't included in the summary, so we can't allow them.
            Condition::CreateCoinAnnouncement(_)
            | Condition::CreatePuzzleAnnouncement(_)
            | Condition::ReceiveMessage(_)
            // The vault isn't a CAT, and this would melt the vault.
            | Condition::RunCatTail(_) => {
                return Err(DriverError::UnsupportedClearSigningCondition);
            }
            Condition::Other(condition) => {
                let (opcode, _) = <(BigInt, NodePtr)>::from_clvm(allocator, condition)?;

                // If an unparseable opcode matches the SEND_MESSAGE opcode, we return an error.
                // This is to make sure that we don't accidentally let a valid message slip through the cracks.
                // If this happened, we wouldn't enforce the spend to be revealed, thus clear signing would be insecure.
                if opcode == BigInt::from(SEND_MESSAGE) {
                    return Err(DriverError::InvalidVaultMessageFormat);
                }

                if [
                    CREATE_COIN_ANNOUNCEMENT,
                    CREATE_PUZZLE_ANNOUNCEMENT,
                    RECEIVE_MESSAGE,
                ]
                .into_iter()
                .any(|unsupported| opcode == BigInt::from(unsupported))
                {
                    return Err(DriverError::UnsupportedClearSigningCondition);
                }
            }
            _ => {}
        }
    }

    Ok(VaultSpendSummary {
        child,
        drop_coins,
        messages,
    })
}
