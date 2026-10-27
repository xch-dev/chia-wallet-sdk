use chia_bls::{Signature, aggregate_verify};
use chia_consensus::{
    allocator::make_allocator, flags::ConsensusFlags, spendbundle_conditions::run_spendbundle,
};
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program, SpendBundle};
use chia_puzzle_types::{Memos, singleton::SingletonArgs};
use chia_sdk_types::{
    Condition, Conditions,
    conditions::{ReceiveMessage, SendMessage},
    default_constants, run_puzzle,
};
use chia_sdk_utils::Bech32;
use chia_streamable_macro::streamable;
use chia_traits::Streamable;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{
    Allocator,
    serde::{node_from_bytes, node_to_bytes},
};
use hex_literal::hex;

use crate::{DriverError, Spend, SpendContext, Vault, parse_vault_message};

/// The puzzle hash of the coin that every message in a vault message envelope is sent to, which is the tree hash
/// of the puzzle `1`. Its solution is the list of conditions that receive the messages.
pub const VAULT_MESSAGE_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2"
));

/// The coin id of [`vault_message_coin`], which every message in a vault message envelope is sent to.
pub const VAULT_MESSAGE_COIN_ID: Bytes32 = Bytes32::new(hex!(
    "059dd91d99d28fc1408e19689ecab815eb2041728965adc6651f63e9f40d97ea"
));

/// The message mode used by a vault message envelope, which is sent from the vault's puzzle to a coin id.
pub const VAULT_MESSAGE_MODE: u8 = 0x17;

/// The version byte that prefixes an encoded [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_VERSION: u8 = 1;

/// The bech32m prefix of the string form of a [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_PREFIX: &str = "vaultsig";

/// The coin that receives the messages in a vault message envelope.
///
/// Its parent coin id is all zeros, which would require a SHA-256 preimage of zero, so this coin can never exist on
/// chain. A spend that sends it a message can never be valid on chain, but can still be validated with consensus
/// rules by spending it alongside the vault in [`verify_vault_signed_message`].
pub fn vault_message_coin() -> Coin {
    Coin::new(Bytes32::default(), VAULT_MESSAGE_PUZZLE_HASH, 0)
}

/// Builds the delegated spend that a vault signs in order to sign one or more arbitrary messages off-chain.
///
/// This is a normal vault spend, which recreates the vault with the same custody hash and amount, and sends each
/// message in order to [`VAULT_MESSAGE_COIN_ID`]. Because that coin can never exist, the spend can never be valid
/// on chain.
pub fn vault_message_envelope(
    ctx: &mut SpendContext,
    vault: &Vault,
    messages: &[Bytes],
) -> Result<Spend, DriverError> {
    if messages.is_empty() {
        return Err(DriverError::EmptyVaultMessageEnvelope);
    }

    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;

    let mut conditions = Conditions::new().create_coin(
        vault.info.custody_hash.into(),
        vault.coin.amount,
        Memos::None,
    );

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

/// A portable proof that a vault signed one or more messages, which can be verified with
/// [`verify_vault_signed_message`] without any chain access, given the vault's launcher id and custody hash.
///
/// This is the signed spend of the vault coin with a [`vault_message_envelope`] as its delegated spend, exactly as
/// it would be submitted as a transaction.
#[streamable]
pub struct VaultMessageProof {
    /// The spend of the vault coin.
    pub coin_spend: CoinSpend,
    /// The aggregated BLS signature of the spend, which is the default (infinity) signature if no BLS signatures
    /// are required.
    pub aggregated_signature: Signature,
}

impl VaultMessageProof {
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

/// Verifies that the vault with the expected launcher id and custody hash signed the messages in the proof, and
/// returns the messages in order.
///
/// The proof's vault spend is validated with consensus rules in a spend bundle, alongside a spend of
/// [`vault_message_coin`] that receives each message from the vault's puzzle hash. This doesn't depend on which
/// kinds of members the vault has, since the vault's puzzle performs its own signature checks.
///
/// # What is verified
///
/// - The vault coin's puzzle hash is the singleton puzzle hash for the expected launcher id and custody hash.
/// - The puzzle reveal matches the coin's puzzle hash, and runs successfully on the solution within the maximum
///   block cost. This is where the vault's secp256r1 and passkey signatures, m-of-n thresholds, restrictions, and
///   singleton rules (such as recreating the vault) are checked.
/// - The output conditions are well formed, and any assertions about the coin itself (such as `ASSERT_MY_PUZZLEHASH`,
///   `ASSERT_MY_COIN_ID`, `ASSERT_MY_PARENT_ID`, and `ASSERT_MY_AMOUNT`) hold for the coin in the proof.
/// - Conditions between the spends in the bundle hold. Every message must be received the right number of times, so
///   the vault must have sent exactly the returned messages to [`VAULT_MESSAGE_COIN_ID`] and no other messages.
///   Announcement assertions, `ASSERT_CONCURRENT_SPEND`, `RESERVE_FEE`, and `ASSERT_EPHEMERAL` must also be
///   satisfied by the bundle alone.
/// - Absolute and relative timelocks don't conflict with each other, such as an `ASSERT_HEIGHT_ABSOLUTE` that's
///   later than an `ASSERT_BEFORE_HEIGHT_ABSOLUTE`.
/// - The aggregated BLS signature is valid for every `AGG_SIG_*` condition, using the given genesis challenge as
///   the additional data for `AGG_SIG_ME` and the other coin-bound variants.
///
/// # What is not verified
///
/// The proof is never valid on chain, since [`vault_message_coin`] can't exist. Compared to including the spend in a
/// block, these checks are skipped:
///
/// - The vault coin isn't checked to exist, or to be unspent. The proof can be for any coin with the vault's puzzle
///   hash, including one that was spent long ago or never existed, and its lineage proof only has to match the coin's
///   parent coin id in the proof rather than the chain's history. This is why the custody hash must be trusted.
/// - Timelocks aren't checked against the chain's height or timestamp, or the coin's creation height or timestamp.
/// - Mempool rules aren't applied. For example, unknown condition opcodes are allowed (with their cost), and there's
///   no minimum fee or mempool cost limit. The cost limit is the maximum cost of a block, rather than what's left in
///   one after other transactions.
/// - The coins created by the vault spend aren't checked to be new, and nothing is checked against other spends in
///   the same block or mempool, such as conflicting spends of the vault coin.
///
/// The expected custody hash must come from a trusted source, such as the vault's coin on chain. Without it, anyone
/// could make up a custody puzzle for any launcher id that doesn't require a signature. Since a vault's custody hash
/// changes when its signers are changed, this also determines which signers the proof is checked against.
pub fn verify_vault_signed_message(
    proof: &VaultMessageProof,
    expected_launcher_id: Bytes32,
    expected_custody_hash: Bytes32,
    genesis_challenge: Bytes32,
) -> Result<Vec<Bytes>, DriverError> {
    let vault_puzzle_hash: Bytes32 =
        SingletonArgs::curry_tree_hash(expected_launcher_id, TreeHash::from(expected_custody_hash))
            .into();

    if proof.coin_spend.coin.puzzle_hash != vault_puzzle_hash {
        return Err(DriverError::WrongVaultPuzzleHash);
    }

    let mut allocator = Allocator::new();

    // Find the messages that the vault sends to the receiving coin, in order. Consensus rules would only check that
    // they're received, regardless of the order.
    let puzzle = node_from_bytes(&mut allocator, &proof.coin_spend.puzzle_reveal)?;
    let solution = node_from_bytes(&mut allocator, &proof.coin_spend.solution)?;
    let output = run_puzzle(&mut allocator, puzzle, solution)?;
    let conditions = Vec::<Condition>::from_clvm(&allocator, output)?;

    let mut messages = Vec::new();

    for condition in conditions {
        let Condition::SendMessage(condition) = condition else {
            continue;
        };

        // Any other message wouldn't be received, so it fails consensus validation below.
        if let Ok(message) = parse_vault_message(&allocator, condition)
            && is_vault_message_envelope_coin_id(message.spent_coin_id)
        {
            messages.push(message.data);
        }
    }

    if messages.is_empty() {
        return Err(DriverError::EmptyVaultMessageEnvelope);
    }

    let sender = allocator.new_atom(&vault_puzzle_hash)?;

    let mut receive_conditions = Conditions::new();

    for message in &messages {
        receive_conditions.push(ReceiveMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![sender],
        ));
    }

    let receive_conditions = receive_conditions.to_clvm(&mut allocator)?;
    let receiver_puzzle = allocator.one();

    let spend_bundle = SpendBundle::new(
        vec![
            proof.coin_spend.clone(),
            CoinSpend::new(
                vault_message_coin(),
                Program::from(node_to_bytes(&allocator, receiver_puzzle)?),
                Program::from(node_to_bytes(&allocator, receive_conditions)?),
            ),
        ],
        proof.aggregated_signature.clone(),
    );

    let constants = default_constants(genesis_challenge, genesis_challenge);
    let mut allocator = make_allocator(ConsensusFlags::LIMIT_HEAP);

    // No timeout is used, since measuring time isn't supported on every platform (such as WebAssembly).
    let (_, pairs) = run_spendbundle(
        &mut allocator,
        &spend_bundle,
        constants.max_block_cost_clvm,
        ConsensusFlags::ENABLE_KECCAK_OPS_OUTSIDE_GUARD,
        &constants,
        None,
    )?;

    if !aggregate_verify(
        &spend_bundle.aggregated_signature,
        pairs.iter().map(|(pk, msg)| (pk, msg.as_ref())),
    ) {
        return Err(DriverError::InvalidVaultMessageSignature);
    }

    Ok(messages)
}

#[cfg(test)]
mod tests;
