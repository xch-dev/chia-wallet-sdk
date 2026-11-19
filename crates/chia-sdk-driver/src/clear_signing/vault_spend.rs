use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::Condition;
use clvmr::Allocator;

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
/// Otherwise, an odd `CREATE_COIN` is the recreated vault, and any other `CREATE_COIN` is a [`DropCoin`]. The vault is
/// melted instead if it outputs a `MELT_SINGLETON` or `RUN_CAT_TAIL`. Like the singleton layer, more than one odd
/// output (including a melt) returns [`DriverError::InvalidDelegatedSpendFormat`]. Each `SEND_MESSAGE` is parsed with
/// [`parse_vault_puzzle_message`] if it's sent to a puzzle hash, or [`parse_vault_message`] otherwise, and the
/// conditions are parsed with [`parse_delegated_spend`], so no message can be missed. Puzzle announcement and
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
    let mut melted = false;
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
                // If there's no odd output, the singleton layer fails the spend, so it's safe to assume that
                // the vault is melted.
                if condition.amount % 2 == 1 {
                    // The singleton layer doesn't allow more than one odd output, including a melt.
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
                // they can be validated to be impossible to circumvent. A message to a puzzle hash can only authorize
                // the spend of a controlled vault, which is checked when the messages are matched.
                if is_vault_puzzle_message(&condition) {
                    puzzle_messages.push(parse_vault_puzzle_message(allocator, condition)?);
                } else {
                    messages.push(parse_vault_message(allocator, condition)?);
                }
            }
            // The singleton layer melts the vault if its odd output has the amount -113. A TAIL reveal has the same
            // amount, and the vault isn't a CAT, so it melts the vault too.
            Condition::MeltSingleton(_) | Condition::RunCatTail(_) => {
                if melted || child.is_some() {
                    return Err(DriverError::InvalidDelegatedSpendFormat);
                }

                melted = true;
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

#[cfg(test)]
mod tests {
    use clvm_traits::{ToClvm, clvm_quote};
    use clvmr::NodePtr;
    use rstest::rstest;

    use super::*;

    /// Parses a vault delegated spend from conditions written as hex atoms, with `()` for nil and `ph` or `other` for
    /// a puzzle hash, so that non-canonical encodings can be written out exactly.
    fn parse(conditions: &[&str]) -> Result<VaultSpendSummary, DriverError> {
        let mut allocator = Allocator::new();
        let mut list = Vec::new();

        for condition in conditions {
            let mut atoms = Vec::new();

            for atom in condition.split_whitespace() {
                let bytes = match atom {
                    "()" => Vec::new(),
                    "ph" => vec![1; 32],
                    "other" => vec![2; 32],
                    hex => hex::decode(hex).unwrap(),
                };
                atoms.push(allocator.new_atom(&bytes)?);
            }

            list.push(atoms.to_clvm(&mut allocator)?);
        }

        let puzzle = clvm_quote!(list).to_clvm(&mut allocator)?;

        parse_vault_delegated_spend(
            &mut Facts::default(),
            &mut allocator,
            Spend::new(puzzle, NodePtr::NIL),
        )
    }

    #[rstest]
    #[case::recreated(&["33 ph 01"], true)]
    #[case::non_canonical_opcode_is_ignored(&["0033 other 01", "33 ph 01"], true)]
    #[case::melt(&["33 () 8f"], false)]
    #[case::tail_reveal_melts(&["33 () 8f () ()"], false)]
    fn test_vault_spend_child(
        #[case] conditions: &[&str],
        #[case] recreated: bool,
    ) -> anyhow::Result<()> {
        let summary = parse(conditions)?;

        assert_eq!(
            summary.child,
            recreated.then(|| VaultOutput::new(Bytes32::new([1; 32]), 1))
        );

        Ok(())
    }

    #[rstest]
    #[case::two_odd_outputs(&["33 ph 01", "33 other 01"])]
    #[case::melt_then_odd_output(&["33 () 8f", "33 ph 01"])]
    #[case::odd_output_then_melt(&["33 ph 01", "33 () 8f"])]
    #[case::two_melts(&["33 () 8f", "33 () 8f () ()"])]
    #[case::negative_amount(&["33 ph fb"])]
    #[case::melt_with_puzzle_hash(&["33 ph 8f"])]
    #[case::non_canonical_melt(&["33 () ff8f"])]
    #[case::invalid_message(&["42 3f"])]
    fn test_vault_spend_rejected(#[case] conditions: &[&str]) {
        assert!(matches!(
            parse(conditions),
            Err(DriverError::InvalidDelegatedSpendFormat)
        ));
    }
}
