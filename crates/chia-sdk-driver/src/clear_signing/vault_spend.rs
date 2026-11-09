use chia_consensus::opcodes::SEND_MESSAGE;
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::Condition;
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use num_bigint::BigInt;

use crate::{
    DriverError, Facts, Spend, VaultMessage, VaultPuzzleMessage, is_vault_puzzle_message,
    parse_delegated_spend, parse_vault_message, parse_vault_message_envelope,
    parse_vault_puzzle_message,
};

/// What the vault's delegated spend does to the vault itself, as parsed by [`parse_vault_delegated_spend`].
#[derive(Debug, Clone)]
pub struct VaultSpendSummary {
    /// The recreated vault singleton, or `None` if the delegated spend doesn't create an odd coin (in which case the
    /// vault is melted, or the spend is invalid).
    pub child: Option<VaultOutput>,
    /// The coins with an even amount that the vault creates.
    pub drop_coins: Vec<DropCoin>,
    /// The messages that the vault sends to coin ids, to authorize their spends.
    pub messages: Vec<VaultMessage>,
    /// Messages to the puzzle hashes of controlled vaults, which must each match a revealed controlled vault.
    pub puzzle_messages: Vec<VaultPuzzleMessage>,
    /// The arbitrary messages being signed, in order, if the delegated spend is a
    /// [`vault_message_envelope`](crate::vault_message_envelope). In that case, every other field is empty except
    /// [`child`](Self::child).
    pub signed_messages: Vec<Bytes>,
}

/// The vault singleton created by a vault spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VaultOutput {
    /// The new custody puzzle hash (the singleton's inner puzzle hash). If it differs from the current one, the
    /// vault's custody configuration is changing.
    pub custody_hash: Bytes32,
    /// The new amount of the vault singleton.
    pub amount: u64,
}

impl VaultOutput {
    /// Creates a vault output.
    pub fn new(custody_hash: Bytes32, amount: u64) -> Self {
        Self {
            custody_hash,
            amount,
        }
    }
}

/// A coin with an even amount created directly by the vault singleton spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DropCoin {
    /// The puzzle hash of the coin, as written in the `CREATE_COIN` condition.
    pub puzzle_hash: Bytes32,
    /// The amount of the coin.
    pub amount: u64,
}

impl DropCoin {
    /// Creates a drop coin.
    pub fn new(puzzle_hash: Bytes32, amount: u64) -> Self {
        Self {
            puzzle_hash,
            amount,
        }
    }
}

/// Parses the conditions of the vault's delegated spend, which must be a quoted list of conditions.
///
/// If the delegated spend is a [`vault_message_envelope`](crate::vault_message_envelope), its messages are returned
/// in [`signed_messages`](VaultSpendSummary::signed_messages), along with the recreated vault, and nothing is recorded
/// in the [`Facts`]. An envelope that does anything other than recreate the vault returns
/// [`DriverError::MixedVaultMessageEnvelope`].
///
/// Otherwise, an odd `CREATE_COIN` is the recreated vault (if there's more than one, the last is used), and any other
/// `CREATE_COIN` is a [`DropCoin`]. Each `SEND_MESSAGE` is parsed with [`parse_vault_puzzle_message`] if it's sent to
/// a puzzle hash, or [`parse_vault_message`] otherwise, and a condition with the `SEND_MESSAGE` opcode that can't be
/// parsed returns [`DriverError::InvalidVaultMessageFormat`], so no message can be missed. Puzzle announcement and
/// concurrent spend assertions, `ASSERT_SECONDS_ABSOLUTE`, `ASSERT_BEFORE_SECONDS_ABSOLUTE`, and `RESERVE_FEE` are
/// recorded in the [`Facts`].
pub fn parse_vault_delegated_spend(
    facts: &mut Facts,
    allocator: &mut Allocator,
    delegated_spend: Spend,
) -> Result<VaultSpendSummary, DriverError> {
    let conditions = parse_delegated_spend(allocator, delegated_spend)?;

    // A delegated spend that signs messages can't do anything other than recreate the vault, so that signing a message
    // can't look like it also authorizes a transaction. The messages are returned separately, in their original order.
    if let Some(envelope) = parse_vault_message_envelope(allocator, &conditions)? {
        return Ok(VaultSpendSummary {
            child: Some(envelope.vault_child),
            drop_coins: Vec::new(),
            messages: Vec::new(),
            puzzle_messages: Vec::new(),
            signed_messages: envelope.messages,
        });
    }

    let mut child = None;
    let mut drop_coins = Vec::new();
    let mut messages = Vec::new();
    let mut puzzle_messages = Vec::new();

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
                facts.update_actual_earliest_time(condition.seconds);
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
                // they can be validated to be impossible to circumvent. A message to a puzzle hash can only authorize
                // the spend of a controlled vault, which is checked when the messages are matched.
                if is_vault_puzzle_message(&condition) {
                    puzzle_messages.push(parse_vault_puzzle_message(allocator, condition)?);
                } else {
                    messages.push(parse_vault_message(allocator, condition)?);
                }
            }
            Condition::Other(condition) => {
                let (opcode, _) = <(BigInt, NodePtr)>::from_clvm(allocator, condition)?;

                // If an unparseable opcode matches the SEND_MESSAGE opcode, we return an error.
                // This is to make sure that we don't accidentally let a valid message slip through the cracks.
                // If this happened, we wouldn't enforce the spend to be revealed, thus clear signing would be insecure.
                if opcode == BigInt::from(SEND_MESSAGE) {
                    return Err(DriverError::InvalidVaultMessageFormat);
                }
            }
            _ => {}
        }
    }

    Ok(VaultSpendSummary {
        child,
        drop_coins,
        messages,
        puzzle_messages,
        signed_messages: Vec::new(),
    })
}
