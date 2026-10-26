use chia_protocol::{Bytes, Bytes32, Program};
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{
    Condition, Conditions, Mod,
    conditions::SendMessage,
    puzzles::{
        AddDelegatedPuzzleWrapper, DelegatedPuzzleFeederArgs, DelegatedPuzzleFeederSolution,
        IndexWrapperArgs,
    },
    run_puzzle,
};
use chia_sdk_utils::Bech32;
use chia_streamable_macro::streamable;
use chia_traits::Streamable;
use clvm_traits::FromClvm;
use clvm_utils::tree_hash;
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};

use crate::{DriverError, Puzzle, Spend, SpendContext, parse_delegated_spend, parse_vault_message};

/// The coin id that every message in a vault message envelope is sent to. A coin with this id can never be spent,
/// since it would require a SHA-256 preimage of zero, so the envelope can never be valid on chain.
pub const VAULT_MESSAGE_COIN_ID: Bytes32 = Bytes32::new([0; 32]);

/// The message mode used by a vault message envelope, which is sent from the vault's puzzle to a coin id.
pub const VAULT_MESSAGE_MODE: u8 = 0x17;

/// The version byte that prefixes an encoded [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_VERSION: u8 = 1;

/// The bech32m prefix of the string form of a [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_PREFIX: &str = "vaultsig";

/// Builds the delegated spend that a vault signs in order to sign one or more arbitrary messages off-chain.
///
/// The delegated puzzle is `(q . ((SEND_MESSAGE 0x17 <message> 0x00…00) …))`, with one message per item in order,
/// and nothing else.
pub fn vault_message_envelope(
    ctx: &mut SpendContext,
    messages: &[Bytes],
) -> Result<Spend, DriverError> {
    if messages.is_empty() {
        return Err(DriverError::EmptyVaultMessageEnvelope);
    }

    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;

    let mut conditions = Conditions::new();

    for message in messages {
        conditions.push(SendMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![coin_id],
        ));
    }

    ctx.delegated_spend(conditions)
}

/// Returns `true` if the vault message is sent to [`VAULT_MESSAGE_COIN_ID`], meaning that it's part of a vault
/// message envelope rather than a message to a linked coin spend.
pub fn is_vault_message_envelope_coin_id(coin_id: Bytes32) -> bool {
    coin_id == VAULT_MESSAGE_COIN_ID
}

/// Parses the messages out of a vault message envelope, in order. Every condition must be a message to
/// [`VAULT_MESSAGE_COIN_ID`], and there must be at least one of them.
pub fn parse_vault_message_envelope(
    allocator: &Allocator,
    delegated_spend: Spend,
) -> Result<Vec<Bytes>, DriverError> {
    let conditions = parse_delegated_spend(allocator, delegated_spend)?;

    if conditions.is_empty() {
        return Err(DriverError::EmptyVaultMessageEnvelope);
    }

    conditions
        .into_iter()
        .map(|condition| {
            let Condition::SendMessage(condition) = condition else {
                return Err(DriverError::InvalidVaultMessageEnvelope);
            };

            let message = parse_vault_message(allocator, condition)
                .map_err(|_| DriverError::InvalidVaultMessageEnvelope)?;

            if !is_vault_message_envelope_coin_id(message.spent_coin_id) {
                return Err(DriverError::InvalidVaultMessageEnvelope);
            }

            Ok(message.data)
        })
        .collect()
}

/// A portable proof that a vault signed one or more messages with its custody path, which can be verified
/// with [`verify_vault_signed_message`] without any chain access, given the vault's launcher id and custody hash.
///
/// The custody puzzle reveal and solution are the inner puzzle and inner solution of the vault singleton,
/// as they would be in a spend of the [`vault_message_envelope`].
#[streamable]
pub struct VaultMessageProof {
    /// The launcher id of the vault that signed the messages.
    pub launcher_id: Bytes32,
    /// The serialized custody (inner) puzzle of the vault.
    pub custody_puzzle_reveal: Program,
    /// The serialized solution to the custody puzzle, whose delegated spend is the vault message envelope.
    pub custody_solution: Program,
}

impl VaultMessageProof {
    /// Creates a proof from the custody spend of the vault, as returned by [`MipsSpend::spend`](crate::MipsSpend::spend)
    /// when the delegated spend is a [`vault_message_envelope`].
    pub fn from_custody_spend(
        ctx: &SpendContext,
        launcher_id: Bytes32,
        custody_spend: Spend,
    ) -> Result<Self, DriverError> {
        Ok(Self::new(
            launcher_id,
            node_to_bytes(ctx, custody_spend.puzzle)?.into(),
            node_to_bytes(ctx, custody_spend.solution)?.into(),
        ))
    }

    /// Serializes the proof as a version byte followed by its streamable encoding.
    pub fn encode(&self) -> Result<Bytes, DriverError> {
        let mut bytes = vec![VAULT_MESSAGE_PROOF_VERSION];
        self.stream(&mut bytes)?;
        Ok(bytes.into())
    }

    /// Deserializes a proof that was serialized with [`VaultMessageProof::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, DriverError> {
        let Some((&version, rest)) = bytes.split_first() else {
            return Err(DriverError::MissingVaultMessageProofVersion);
        };

        if version != VAULT_MESSAGE_PROOF_VERSION {
            return Err(DriverError::UnsupportedVaultMessageProofVersion(version));
        }

        Ok(Self::from_bytes(rest)?)
    }

    /// Encodes the proof as a bech32m string with the [`VAULT_MESSAGE_PROOF_PREFIX`] prefix.
    pub fn encode_string(&self) -> Result<String, DriverError> {
        Ok(Bech32::new(self.encode()?, VAULT_MESSAGE_PROOF_PREFIX.to_string()).encode()?)
    }

    /// Decodes a proof that was encoded with [`VaultMessageProof::encode_string`].
    pub fn decode_string(proof: &str) -> Result<Self, DriverError> {
        let bytes = Bech32::decode(proof)?.expect_prefix(VAULT_MESSAGE_PROOF_PREFIX)?;
        Self::decode(&bytes)
    }
}

/// Verifies that the vault with the expected launcher id and custody hash signed the messages in the proof using its
/// custody path, and returns the messages in order.
///
/// This runs the revealed custody puzzle against the vault message envelope, so all of the vault's signature checks
/// (secp256r1 and passkey members, m-of-n thresholds, and restrictions) are performed by the puzzle itself.
///
/// The expected custody hash must come from a trusted source, such as the vault's coin on chain. The proof reveals
/// the custody puzzle, so without this check anyone could make up a custody puzzle for any launcher id that doesn't
/// require a signature. Since a vault's custody hash changes when its signers are changed, this also determines
/// which signers the proof is checked against.
pub fn verify_vault_signed_message(
    proof: &VaultMessageProof,
    expected_launcher_id: Bytes32,
    expected_custody_hash: Bytes32,
) -> Result<Vec<Bytes>, DriverError> {
    if proof.launcher_id != expected_launcher_id {
        return Err(DriverError::WrongVaultLauncherId);
    }

    let mut allocator = Allocator::new();

    let custody_puzzle = node_from_bytes(&mut allocator, &proof.custody_puzzle_reveal)?;
    let custody_solution = node_from_bytes(&mut allocator, &proof.custody_solution)?;

    let custody_hash = tree_hash(&allocator, custody_puzzle);

    if custody_hash != expected_custody_hash.into() {
        return Err(DriverError::WrongVaultCustodyHash);
    }

    let vault_puzzle_hash: Bytes32 =
        SingletonArgs::curry_tree_hash(proof.launcher_id, custody_hash).into();

    // The custody puzzle must be a top level MIPS puzzle, so that the first item of the solution
    // is the delegated puzzle that every member signs.
    if !is_top_level_mips_puzzle(&allocator, custody_puzzle)? {
        return Err(DriverError::InvalidVaultMessageCustody);
    }

    let solution = DelegatedPuzzleFeederSolution::<NodePtr, NodePtr, NodePtr>::from_clvm(
        &allocator,
        custody_solution,
    )?;

    // Delegated puzzle wrappers are allowed, since the output of the custody puzzle is checked below.
    let delegated_puzzle = unwrap_delegated_puzzle(&allocator, solution.delegated_puzzle)?;
    let messages =
        parse_vault_message_envelope(&allocator, Spend::new(delegated_puzzle, NodePtr::NIL))?;

    // The delegated puzzle must be exactly the envelope, rather than something that only parses as one.
    let mut ctx = SpendContext::from(allocator);
    let envelope = vault_message_envelope(&mut ctx, &messages)?;

    if ctx.tree_hash(envelope.puzzle) != ctx.tree_hash(delegated_puzzle) {
        return Err(DriverError::InvalidVaultMessageEnvelope);
    }

    let output = run_puzzle(&mut ctx, custody_puzzle, custody_solution)?;
    let conditions = Vec::<Condition>::from_clvm(&ctx, output)?;

    let mut output_messages = Vec::new();
    let mut asserts_puzzle_hash = false;

    for condition in conditions {
        match condition {
            Condition::SendMessage(condition) => {
                let message = parse_vault_message(&ctx, condition)?;

                if !is_vault_message_envelope_coin_id(message.spent_coin_id) {
                    return Err(DriverError::UnexpectedVaultMessageCondition);
                }

                output_messages.push(message.data);
            }
            Condition::AssertMyPuzzleHash(condition) => {
                if condition.puzzle_hash != vault_puzzle_hash {
                    return Err(DriverError::VaultPuzzleHashMismatch);
                }

                asserts_puzzle_hash = true;
            }
            // There's no aggregate signature in the proof, and BLS members are not supported.
            condition if condition.is_agg_sig() => {
                return Err(DriverError::UnsupportedVaultMessageSignature);
            }
            _ => return Err(DriverError::UnexpectedVaultMessageCondition),
        }
    }

    // Each member that signs with the vault's puzzle hash asserts it, which is what binds the signatures to this vault.
    // If nothing asserts it, nothing was signed for this vault.
    if !asserts_puzzle_hash {
        return Err(DriverError::UnsignedVaultMessage);
    }

    if output_messages != messages {
        return Err(DriverError::VaultMessageMismatch);
    }

    Ok(messages)
}

fn is_top_level_mips_puzzle(allocator: &Allocator, puzzle: NodePtr) -> Result<bool, DriverError> {
    let Some(index_wrapper) = Puzzle::parse(allocator, puzzle).as_curried() else {
        return Ok(false);
    };

    if index_wrapper.mod_hash != IndexWrapperArgs::<(), ()>::mod_hash() {
        return Ok(false);
    }

    let args = IndexWrapperArgs::<NodePtr, NodePtr>::from_clvm(allocator, index_wrapper.args)?;

    let Some(feeder) = Puzzle::parse(allocator, args.inner_puzzle).as_curried() else {
        return Ok(false);
    };

    Ok(feeder.mod_hash == DelegatedPuzzleFeederArgs::<()>::mod_hash())
}

fn unwrap_delegated_puzzle(
    allocator: &Allocator,
    mut delegated_puzzle: NodePtr,
) -> Result<NodePtr, DriverError> {
    while let Some(curried) = Puzzle::parse(allocator, delegated_puzzle).as_curried()
        && curried.mod_hash == AddDelegatedPuzzleWrapper::<(), ()>::mod_hash()
    {
        delegated_puzzle =
            AddDelegatedPuzzleWrapper::<NodePtr, NodePtr>::from_clvm(allocator, curried.args)?
                .delegated_puzzle;
    }

    Ok(delegated_puzzle)
}

#[cfg(test)]
mod tests;
