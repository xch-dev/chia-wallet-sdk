use std::collections::HashSet;

use chia_bls::{Signature, aggregate_verify};
use chia_consensus::{
    allocator::make_allocator, flags::MEMPOOL_MODE, spendbundle_conditions::run_spendbundle,
    spendbundle_validation::get_flags_for_height_and_constants,
};
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program, SpendBundle};
use chia_puzzle_types::{Memos, singleton::SingletonArgs};
use chia_sdk_types::{
    Condition, Conditions, Mod,
    conditions::{ReceiveMessage, SendMessage},
    default_constants,
    puzzles::{AddDelegatedPuzzleWrapper, AddDelegatedPuzzleWrapperSolution, ForceCoinMessageMod},
};
use chia_sdk_utils::Bech32;
use chia_streamable_macro::streamable;
use chia_traits::Streamable;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash, tree_hash};
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use hex_literal::hex;
use indexmap::IndexMap;

use crate::{
    DelegatedPuzzleFeederLayer, DriverError, IndexWrapperLayer, Layer, Puzzle, SingletonLayer,
    Spend, SpendContext, Vault, VaultInfo, VaultOutput, is_vault_puzzle_message,
    parse_delegated_spend, parse_vault_message, parse_vault_puzzle_message,
};

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

/// The message mode that a vault sends to a vault it controls when the controlled vault's singleton member is
/// fast-forwardable, which is sent from the vault's puzzle to the controlled vault's puzzle hash rather than its coin id.
/// This is the mode that's curried into a fast-forwardable [`SingletonMemberWithMode`](chia_sdk_types::puzzles::SingletonMemberWithMode).
pub const VAULT_FAST_FORWARD_MESSAGE_MODE: u8 = 0x12;

/// The version byte that prefixes an encoded [`VaultMessageProof`] that has no controller spends.
pub const VAULT_MESSAGE_PROOF_VERSION: u8 = 1;

/// The version byte that prefixes an encoded [`VaultMessageProof`] that has controller spends, which is used for a vault
/// whose custody includes another vault. Proofs without controller spends are still encoded with
/// [`VAULT_MESSAGE_PROOF_VERSION`], so that they can be verified by older versions.
pub const VAULT_MESSAGE_PROOF_CONTROLLED_VERSION: u8 = 2;

/// The maximum number of controller spends in a [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_MAX_CONTROLLERS: usize = 16;

/// The bech32m prefix of the string form of a [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_PREFIX: &str = "vaultsig";

/// The maximum length of each message in a vault message envelope, which is the consensus limit for the message in a
/// `SEND_MESSAGE` condition.
pub const VAULT_MESSAGE_MAX_LENGTH: usize = 1024;

/// The default maximum cost of verifying a [`VaultMessageProof`], which is used by [`verify_vault_signed_message`]
/// unless another limit is given. This includes the cost of each byte of the spend, and a 3 of 4 vault with passkey
/// members costs about 214 million. In a release build, running a spend up to this cost takes about 300 milliseconds
/// in the worst case.
pub const VAULT_MESSAGE_PROOF_DEFAULT_MAX_COST: u64 = 1_000_000_000;

/// The maximum size of an encoded [`VaultMessageProof`] that can be decoded. A proof's spend can't cost more than a
/// block, and each byte costs 12,000, so a larger proof could never be verified.
pub const VAULT_MESSAGE_PROOF_MAX_SIZE: usize = 1_000_000;

/// The maximum number of delegated puzzle wrappers around the delegated spend in a [`VaultMessageProof`], which bounds
/// the work done to find the signed delegated spend.
pub const VAULT_MESSAGE_PROOF_MAX_WRAPPERS: usize = 32;

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
///
/// The messages are signed as is, with no added domain separation or nonce. See the replay section of
/// [`verify_vault_signed_message`] for what they should include.
pub fn vault_message_envelope(
    ctx: &mut SpendContext,
    vault: &Vault,
    messages: &[Bytes],
) -> Result<Spend, DriverError> {
    if messages.is_empty() {
        return Err(DriverError::EmptyVaultMessageEnvelope);
    }

    if messages
        .iter()
        .any(|message| message.len() > VAULT_MESSAGE_MAX_LENGTH)
    {
        return Err(DriverError::VaultMessageTooLong);
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

/// Builds the delegated spend that a vault signs to authorize the delegated spend of a vault that it controls, which is
/// a vault whose custody includes it as a singleton member. This is used to sign messages with the controlled vault,
/// by authorizing its [`vault_message_envelope`].
///
/// This recreates the controller with the same custody hash and amount, and sends the delegated puzzle hash to the
/// controlled vault's coin. The delegated puzzle hash is the one that the controlled vault's singleton member is
/// passed, which is the hash of its delegated puzzle, inside of any delegated puzzle wrappers of the member's path.
///
/// If the controlled vault's singleton member is fast-forwardable, the message is sent to the controlled vault's
/// puzzle hash instead, with [`VAULT_FAST_FORWARD_MESSAGE_MODE`], so that its spend can be fast-forwarded.
pub fn vault_controller_envelope(
    ctx: &mut SpendContext,
    controller: &Vault,
    controlled_coin: Coin,
    delegated_puzzle_hash: Bytes32,
    fast_forward: bool,
) -> Result<Spend, DriverError> {
    let (mode, receiver) = if fast_forward {
        (VAULT_FAST_FORWARD_MESSAGE_MODE, controlled_coin.puzzle_hash)
    } else {
        (VAULT_MESSAGE_MODE, controlled_coin.coin_id())
    };

    let receiver = ctx.alloc(&receiver)?;

    let conditions = Conditions::new()
        .create_coin(
            controller.info.custody_hash.into(),
            controller.coin.amount,
            Memos::None,
        )
        .with(SendMessage::new(
            mode,
            delegated_puzzle_hash.to_vec().into(),
            vec![receiver],
        ));

    ctx.delegated_spend(conditions)
}

/// The vault that a vault controller authorizes the spend of, and the controller that it recreates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VaultControllerEnvelope {
    pub(crate) vault_child: VaultOutput,
    pub(crate) controlled_vault: ControlledVaultReceiver,
}

/// How a vault controller's message identifies the controlled vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlledVaultReceiver {
    CoinId(Bytes32),
    /// The controlled vault's puzzle hash, for a fast-forwardable singleton member.
    PuzzleHash(Bytes32),
}

/// Parses the conditions of a vault's delegated spend as a [`vault_controller_envelope`].
///
/// Returns [`None`] unless the conditions are exactly a single `CREATE_COIN` with an odd amount that recreates the
/// vault, and a single message to a coin other than [`VAULT_MESSAGE_COIN_ID`] (or to a puzzle hash other than
/// [`VAULT_MESSAGE_PUZZLE_HASH`], for a fast-forwardable singleton member). This is shared by clear signing and
/// [`verify_controlled_vault_signed_message`], so that a controller can only authorize a vault message envelope by
/// doing nothing else.
pub(crate) fn parse_vault_controller_envelope(
    allocator: &Allocator,
    conditions: &[Condition],
) -> Result<Option<VaultControllerEnvelope>, DriverError> {
    let [first, second] = conditions else {
        return Ok(None);
    };

    let ((Condition::CreateCoin(create_coin), Condition::SendMessage(send_message))
    | (Condition::SendMessage(send_message), Condition::CreateCoin(create_coin))) = (first, second)
    else {
        return Ok(None);
    };

    if create_coin.amount % 2 != 1 {
        return Ok(None);
    }

    let controlled_vault = if is_vault_puzzle_message(send_message) {
        let message = parse_vault_puzzle_message(allocator, send_message.clone())?;

        if message.puzzle_hash == VAULT_MESSAGE_PUZZLE_HASH {
            return Ok(None);
        }

        ControlledVaultReceiver::PuzzleHash(message.puzzle_hash)
    } else {
        let message = parse_vault_message(allocator, send_message.clone())?;

        if message.spent_coin_id == VAULT_MESSAGE_COIN_ID {
            return Ok(None);
        }

        ControlledVaultReceiver::CoinId(message.spent_coin_id)
    };

    Ok(Some(VaultControllerEnvelope {
        vault_child: VaultOutput::new(create_coin.puzzle_hash, create_coin.amount),
        controlled_vault,
    }))
}

/// The messages signed by a vault message envelope, and the vault that it recreates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VaultMessageEnvelope {
    pub(crate) vault_child: VaultOutput,
    pub(crate) messages: Vec<Bytes>,
}

/// Parses the conditions of a vault's delegated spend as a vault message envelope.
///
/// Returns [`None`] if no messages are sent to [`VAULT_MESSAGE_COIN_ID`]. Otherwise, the only other condition allowed
/// is a single `CREATE_COIN` with an odd amount that recreates the vault, and anything else (including a missing vault
/// recreation, which would melt it) is a [`DriverError::MixedVaultMessageEnvelope`]. Each message can be at most
/// [`VAULT_MESSAGE_MAX_LENGTH`] bytes, since a longer one could never be verified. This is shared by clear signing and
/// [`verify_vault_signed_message`], so that they accept the same delegated spends.
pub(crate) fn parse_vault_message_envelope(
    allocator: &Allocator,
    conditions: &[Condition],
) -> Result<Option<VaultMessageEnvelope>, DriverError> {
    let mut vault_child = None;
    let mut messages = Vec::new();
    let mut is_mixed = false;

    for condition in conditions {
        match condition {
            Condition::SendMessage(condition) if is_vault_puzzle_message(condition) => {
                parse_vault_puzzle_message(allocator, condition.clone())?;
                is_mixed = true;
            }
            Condition::SendMessage(condition) => {
                let message = parse_vault_message(allocator, condition.clone())?;

                if message.spent_coin_id == VAULT_MESSAGE_COIN_ID {
                    if message.data.len() > VAULT_MESSAGE_MAX_LENGTH {
                        return Err(DriverError::VaultMessageTooLong);
                    }

                    messages.push(message.data);
                } else {
                    is_mixed = true;
                }
            }
            Condition::CreateCoin(condition)
                if condition.amount % 2 == 1 && vault_child.is_none() =>
            {
                vault_child = Some(VaultOutput::new(condition.puzzle_hash, condition.amount));
            }
            _ => is_mixed = true,
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    match vault_child {
        Some(vault_child) if !is_mixed => Ok(Some(VaultMessageEnvelope {
            vault_child,
            messages,
        })),
        _ => Err(DriverError::MixedVaultMessageEnvelope),
    }
}

/// A portable proof that a vault signed one or more messages, which can be verified with
/// [`verify_vault_signed_message`] (or [`verify_controlled_vault_signed_message`] if it has controller spends) without
/// any chain access, given the vault's launcher id and custody hash.
///
/// This is the signed spend of the vault coin with a [`vault_message_envelope`] as its delegated spend, exactly as
/// it would be submitted as a transaction. If the vault's custody includes other vaults, the spends of the controlling
/// vaults with a [`vault_controller_envelope`] as their delegated spend are included as well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultMessageProof {
    /// The spend of the vault coin.
    pub coin_spend: CoinSpend,
    /// The aggregated BLS signature of all of the spends, which is the default (infinity) signature if no BLS
    /// signatures are required.
    pub aggregated_signature: Signature,
    /// The spends of the vaults that control the vault (or that control another controller), if any.
    pub controller_spends: Vec<CoinSpend>,
}

/// The encoding of a [`VaultMessageProof`] without controller spends.
#[streamable]
struct VaultMessageProofV1 {
    coin_spend: CoinSpend,
    aggregated_signature: Signature,
}

/// The encoding of a [`VaultMessageProof`] with controller spends.
#[streamable]
struct VaultMessageProofV2 {
    coin_spend: CoinSpend,
    controller_spends: Vec<CoinSpend>,
    aggregated_signature: Signature,
}

impl VaultMessageProof {
    /// Creates a proof without any controller spends.
    pub fn new(coin_spend: CoinSpend, aggregated_signature: Signature) -> Self {
        Self {
            coin_spend,
            aggregated_signature,
            controller_spends: Vec::new(),
        }
    }

    /// Creates a proof with the spends of the vaults that control the vault.
    pub fn with_controllers(
        coin_spend: CoinSpend,
        controller_spends: Vec<CoinSpend>,
        aggregated_signature: Signature,
    ) -> Self {
        Self {
            coin_spend,
            aggregated_signature,
            controller_spends,
        }
    }

    /// Serializes the proof as a version byte followed by its streamable encoding. Proofs without controller spends are
    /// encoded as [`VAULT_MESSAGE_PROOF_VERSION`], and the rest as [`VAULT_MESSAGE_PROOF_CONTROLLED_VERSION`].
    pub fn encode(&self) -> Result<Bytes, DriverError> {
        let mut bytes = Vec::new();

        if self.controller_spends.is_empty() {
            bytes.push(VAULT_MESSAGE_PROOF_VERSION);
            VaultMessageProofV1::new(self.coin_spend.clone(), self.aggregated_signature.clone())
                .stream(&mut bytes)?;
        } else {
            bytes.push(VAULT_MESSAGE_PROOF_CONTROLLED_VERSION);
            VaultMessageProofV2::new(
                self.coin_spend.clone(),
                self.controller_spends.clone(),
                self.aggregated_signature.clone(),
            )
            .stream(&mut bytes)?;
        }

        Ok(bytes.into())
    }

    /// Deserializes a proof that was serialized with [`VaultMessageProof::encode`].
    ///
    /// Proofs that are too large to verify are rejected before they're deserialized.
    pub fn decode(bytes: &[u8]) -> Result<Self, DriverError> {
        if bytes.len() > VAULT_MESSAGE_PROOF_MAX_SIZE {
            return Err(DriverError::VaultMessageProofTooLarge);
        }

        let Some((&version, rest)) = bytes.split_first() else {
            return Err(DriverError::MissingVaultMessageProofVersion);
        };

        match version {
            VAULT_MESSAGE_PROOF_VERSION => {
                let proof = VaultMessageProofV1::from_bytes(rest)?;
                Ok(Self::new(proof.coin_spend, proof.aggregated_signature))
            }
            VAULT_MESSAGE_PROOF_CONTROLLED_VERSION => {
                let proof = VaultMessageProofV2::from_bytes(rest)?;

                // A proof without controller spends has a single encoding, so that the encoding of each proof is
                // unique.
                if proof.controller_spends.is_empty() {
                    return Err(DriverError::UnsupportedVaultMessageProofVersion(version));
                }

                Ok(Self::with_controllers(
                    proof.coin_spend,
                    proof.controller_spends,
                    proof.aggregated_signature,
                ))
            }
            _ => Err(DriverError::UnsupportedVaultMessageProofVersion(version)),
        }
    }

    /// Encodes the proof as a bech32m string with the [`VAULT_MESSAGE_PROOF_PREFIX`] prefix.
    pub fn encode_string(&self) -> Result<String, DriverError> {
        Ok(Bech32::new(self.encode()?, VAULT_MESSAGE_PROOF_PREFIX.to_string()).encode()?)
    }

    /// Decodes a proof that was encoded with [`VaultMessageProof::encode_string`].
    ///
    /// Proofs that are too large to verify are rejected before they're decoded.
    pub fn decode_string(proof: &str) -> Result<Self, DriverError> {
        // Each character of a bech32m string encodes 5 bits, and the prefix and checksum are much less than 1024
        // characters.
        if proof.len() > VAULT_MESSAGE_PROOF_MAX_SIZE * 8 / 5 + 1024 {
            return Err(DriverError::VaultMessageProofTooLarge);
        }

        let bytes = Bech32::decode(proof)?.expect_prefix(VAULT_MESSAGE_PROOF_PREFIX)?;
        Self::decode(&bytes)
    }
}

/// Verifies that the vault with the expected launcher id and custody hash signed the messages in the proof, and
/// returns the messages in order.
///
/// The vault's delegated spend is read from the proof, and must be a [`vault_message_envelope`] that recreates the
/// vault as it was. Then the proof's vault spend is validated with consensus rules in a spend bundle, alongside a spend
/// of [`vault_message_coin`] that receives each message from the vault's puzzle hash. This doesn't depend on which
/// kinds of members the vault has, since the vault's puzzle performs its own signature checks. However, a singleton
/// member, which receives a message from the vault that controls it, can only sign messages if the controller's spend
/// is included in the proof, which [`verify_controlled_vault_signed_message`] verifies. This function verifies proofs
/// without controller spends, and rejects the rest.
///
/// # What is verified
///
/// - The vault coin's puzzle hash, and the hash of the puzzle reveal, are the singleton puzzle hash for the expected
///   launcher id and custody hash.
/// - The custody is a top-level MIPS puzzle, and the delegated puzzle in the solution (inside of any delegated puzzle
///   wrappers) is a quoted list of conditions, which only sends messages to [`VAULT_MESSAGE_COIN_ID`] and recreates the
///   vault with the same custody hash and amount. This is the same check that clear signing does, so a delegated spend
///   that would be shown as a transaction can't be used as a message signature.
/// - The puzzle reveal runs successfully on the solution, with the consensus rules from before hard fork 2 in mempool
///   mode, so operators and conditions that aren't known or active yet are rejected. These rules will need to be
///   updated when hard fork 2 activates, so that members that depend on it can sign messages. This is where the vault's
///   secp256r1, passkey and BLS members, m-of-n thresholds, restrictions, and singleton rules are checked.
/// - The vault's actual output, after any delegated puzzle wrappers, creates no coin other than the recreated vault
///   and reserves no fee. Any other message that a wrapper sends to or receives from another coin fails, since it
///   isn't matched, although a message from the vault to itself is allowed.
/// - There's no [`ForceCoinMessageMod`] delegated puzzle wrapper, since the messages to [`VAULT_MESSAGE_COIN_ID`] would
///   satisfy it, even though the members it restricts are only meant to spend alongside another coin. Similarly,
///   custom restrictions that require messages to be sent shouldn't be used by members that can sign messages.
/// - Any assertions about the coin itself (such as `ASSERT_MY_PUZZLEHASH`, `ASSERT_MY_COIN_ID`, `ASSERT_MY_PARENT_ID`,
///   and `ASSERT_MY_AMOUNT`) hold for the coin in the proof.
/// - Conditions between the spends in the bundle hold. Every message must be received the right number of times, so
///   the vault must have sent exactly the returned messages to [`VAULT_MESSAGE_COIN_ID`] and no other messages.
/// - There are no assertions about height or time at all, such as `ASSERT_SECONDS_RELATIVE`,
///   `ASSERT_BEFORE_HEIGHT_ABSOLUTE`, or `ASSERT_MY_BIRTH_HEIGHT`, since they can't be checked without the chain.
///   Otherwise, a member that is only allowed to spend after a timelock (such as during recovery), or before a
///   deadline, could sign messages at any time. This fails with [`DriverError::VaultMessageTimelocked`], except that
///   a [`Timelock`](chia_sdk_types::puzzles::Timelock) used as a delegated puzzle wrapper fails to run with an
///   envelope at all, so it fails with a consensus error instead.
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
/// - There's no minimum fee, and the cost limit is the given maximum cost rather than what's left in a block after
///   other transactions.
/// - The coins created by the vault spend aren't checked to be new, and nothing is checked against other spends in
///   the same block or mempool, such as conflicting spends of the vault coin.
///
/// The expected custody hash must come from a trusted source, such as the vault's coin on chain. Without it, anyone
/// could make up a custody puzzle for any launcher id that doesn't require a signature. Since a vault's custody hash
/// changes when its signers are changed, this also determines which signers the proof is checked against.
///
/// # Denial of service
///
/// The proof is untrusted, and the work it can cause is bounded by `max_cost`, which defaults to
/// [`VAULT_MESSAGE_PROOF_DEFAULT_MAX_COST`] and can be at most the maximum cost of a block:
///
/// - The size of the puzzle reveal and solution is checked against the cost of their bytes before they're parsed.
///   Back-references aren't supported, so that the size of each program is the size of its serialization.
/// - Everything before the spends are run is linear in the size, including finding the signed delegated spend inside
///   of at most [`VAULT_MESSAGE_PROOF_MAX_WRAPPERS`] delegated puzzle wrappers.
/// - Running the spends is limited to the maximum cost, with a limited heap, which also limits the number of BLS
///   signatures that are checked.
///
/// The signatures are checked while running the spends, or after them for BLS signatures, so a proof without a valid
/// signature can still use the entire maximum cost. For example, it can run arbitrary code as a delegated puzzle
/// wrapper, or as a member that's revealed in an m-of-n solution, since MIPS runs the members before checking that
/// they're in the custody. Services that verify proofs from anyone should limit how many they verify, or choose a lower
/// maximum cost.
///
/// # Replay
///
/// A proof doesn't expire, isn't tied to the vault's current coin, and isn't tied to any particular application. It
/// stays valid for as long as the vault's custody hash is unchanged, and anyone who sees it can present it again.
/// The messages themselves should prevent replay when that matters, such as by including a domain or application
/// name, a nonce or challenge chosen by the verifier, and an expiration time, which the caller then checks.
pub fn verify_vault_signed_message(
    proof: &VaultMessageProof,
    expected_launcher_id: Bytes32,
    expected_custody_hash: Bytes32,
    genesis_challenge: Bytes32,
    max_cost: Option<u64>,
) -> Result<Vec<Bytes>, DriverError> {
    verify_controlled_vault_signed_message(
        proof,
        expected_launcher_id,
        expected_custody_hash,
        &[],
        genesis_challenge,
        max_cost,
    )
}

/// Verifies that the vault with the expected launcher id and custody hash signed the messages in the proof, like
/// [`verify_vault_signed_message`], but also allows the vault's custody to include other vaults (as singleton
/// members), which are authorized by the controller spends in the proof.
///
/// Each controller spend must be the spend of one of the given controllers, whose custody hashes must come from a
/// trusted source, just like the expected custody hash. Since the coins in the proof aren't checked to exist, anyone
/// could otherwise make up a coin with a controller's launcher id and a custody puzzle that doesn't require a
/// signature, which could then authorize the vault's spend. A controller can itself be controlled by other vaults,
/// in which case those vaults must be given and spent as well.
///
/// Each controller spend is checked like the vault's spend, except that its delegated spend must be exactly a
/// [`vault_controller_envelope`], which recreates the controller as it was and sends one message to another vault in
/// the proof. The consensus rules then check that every message is received by the vault it's sent to, which only a
/// singleton member of the controller does. Every spend in the proof is run together, so each controller's members,
/// restrictions and signatures are checked, and every spend must recreate its vault and create nothing else.
pub fn verify_controlled_vault_signed_message(
    proof: &VaultMessageProof,
    expected_launcher_id: Bytes32,
    expected_custody_hash: Bytes32,
    controllers: &[VaultInfo],
    genesis_challenge: Bytes32,
    max_cost: Option<u64>,
) -> Result<Vec<Bytes>, DriverError> {
    let coin_spend = &proof.coin_spend;
    let constants = default_constants(genesis_challenge, genesis_challenge);
    let max_cost = max_cost
        .unwrap_or(VAULT_MESSAGE_PROOF_DEFAULT_MAX_COST)
        .min(constants.max_block_cost_clvm);

    if proof.controller_spends.len() > VAULT_MESSAGE_PROOF_MAX_CONTROLLERS {
        return Err(DriverError::TooManyVaultControllers);
    }

    // Each byte of the spends has a cost, so spends that are too large for the maximum cost are rejected before
    // they're parsed.
    let size = std::iter::once(coin_spend)
        .chain(&proof.controller_spends)
        .map(|spend| spend.puzzle_reveal.len() + spend.solution.len())
        .sum::<usize>();

    if (size as u64).saturating_mul(constants.cost_per_byte) > max_cost {
        return Err(DriverError::VaultMessageProofTooLarge);
    }

    let vault_puzzle_hash: Bytes32 =
        SingletonArgs::curry_tree_hash(expected_launcher_id, TreeHash::from(expected_custody_hash))
            .into();

    if coin_spend.coin.puzzle_hash != vault_puzzle_hash {
        return Err(DriverError::WrongVaultPuzzleHash);
    }

    // The consensus rules from before hard fork 2 are used, in mempool mode, so that operators and conditions that
    // aren't known or active yet fail rather than being treated as no-ops.
    let flags = get_flags_for_height_and_constants(
        constants.hard_fork2_height.saturating_sub(1),
        &constants,
    ) | MEMPOOL_MODE;
    let mut allocator = make_allocator(flags);

    // The delegated spend is what the vault's signers signed, and its conditions must be exactly a vault message
    // envelope.
    let (launcher_id, conditions) = parse_signed_vault_spend(&mut allocator, coin_spend)?;
    debug_assert_eq!(launcher_id, expected_launcher_id);

    let envelope = parse_vault_message_envelope(&allocator, &conditions)?
        .ok_or(DriverError::EmptyVaultMessageEnvelope)?;

    if envelope.vault_child != VaultOutput::new(expected_custody_hash, coin_spend.coin.amount) {
        return Err(DriverError::VaultMessageEnvelopeChangesVault);
    }

    // The coin id and puzzle hash of each vault in the proof, which is what each of them must recreate.
    let mut vault_coins = IndexMap::new();
    vault_coins.insert(coin_spend.coin.coin_id(), vault_puzzle_hash);

    let mut launcher_ids = HashSet::from([expected_launcher_id]);
    let mut controlled_vaults = Vec::new();

    for controller_spend in &proof.controller_spends {
        let (launcher_id, conditions) = parse_signed_vault_spend(&mut allocator, controller_spend)?;

        if !launcher_ids.insert(launcher_id) {
            return Err(DriverError::DuplicateVaultControllerSpend);
        }

        let custody_hash = controllers
            .iter()
            .find(|controller| controller.launcher_id == launcher_id)
            .ok_or(DriverError::UnknownVaultController(launcher_id))?
            .custody_hash;

        let controller_puzzle_hash: Bytes32 =
            SingletonArgs::curry_tree_hash(launcher_id, custody_hash).into();

        if controller_spend.coin.puzzle_hash != controller_puzzle_hash {
            return Err(DriverError::WrongVaultPuzzleHash);
        }

        let envelope = parse_vault_controller_envelope(&allocator, &conditions)?
            .ok_or(DriverError::InvalidVaultControllerEnvelope)?;

        if envelope.vault_child
            != VaultOutput::new(custody_hash.into(), controller_spend.coin.amount)
        {
            return Err(DriverError::VaultMessageEnvelopeChangesVault);
        }

        vault_coins.insert(controller_spend.coin.coin_id(), controller_puzzle_hash);
        controlled_vaults.push(envelope.controlled_vault);
    }

    // A controller can only authorize the spend of another vault in the proof, which is then checked to receive its
    // message. Each vault in the proof has a different launcher id, and therefore a different puzzle hash.
    if !controlled_vaults
        .iter()
        .all(|controlled_vault| match controlled_vault {
            ControlledVaultReceiver::CoinId(coin_id) => vault_coins.contains_key(coin_id),
            ControlledVaultReceiver::PuzzleHash(puzzle_hash) => {
                vault_coins.values().any(|value| value == puzzle_hash)
            }
        })
    {
        return Err(DriverError::InvalidVaultControllerEnvelope);
    }

    let sender = allocator.new_atom(&vault_puzzle_hash)?;

    let mut receive_conditions = Conditions::new();

    for message in &envelope.messages {
        receive_conditions.push(ReceiveMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![sender],
        ));
    }

    let receive_conditions = receive_conditions.to_clvm(&mut allocator)?;

    let spend_bundle = SpendBundle::new(
        std::iter::once(coin_spend.clone())
            .chain(proof.controller_spends.iter().cloned())
            .chain(std::iter::once(CoinSpend::new(
                vault_message_coin(),
                // The puzzle `1`, which returns its solution as its conditions.
                Program::from(vec![1]),
                Program::from(node_to_bytes(&allocator, receive_conditions)?),
            )))
            .collect(),
        proof.aggregated_signature.clone(),
    );

    // No timeout is used, since measuring time isn't supported on every platform (such as WebAssembly).
    let (conditions, pairs) = run_spendbundle(
        &mut allocator,
        &spend_bundle,
        max_cost,
        flags,
        &constants,
        None,
    )?;

    if conditions.height_absolute > 0
        || conditions.seconds_absolute > 0
        || conditions.before_height_absolute.is_some()
        || conditions.before_seconds_absolute.is_some()
        || conditions.spends.iter().any(|spend| {
            spend.height_relative.is_some_and(|height| height > 0)
                || spend.seconds_relative.is_some_and(|seconds| seconds > 0)
                || spend.before_height_relative.is_some()
                || spend.before_seconds_relative.is_some()
                || spend.birth_height.is_some()
                || spend.birth_seconds.is_some()
        })
    {
        return Err(DriverError::VaultMessageTimelocked);
    }

    // Delegated puzzle wrappers can change a vault's output, so the output itself must also only recreate the vault.
    let recreates_vaults_only = conditions.reserve_fee == 0
        && conditions.spends.iter().all(|spend| {
            let coin_id: Bytes32 = *spend.coin_id;

            match vault_coins.get(&coin_id) {
                Some(&puzzle_hash) => {
                    let amount = spend_bundle
                        .coin_spends
                        .iter()
                        .find(|coin_spend| coin_spend.coin.coin_id() == coin_id)
                        .map(|coin_spend| coin_spend.coin.amount);

                    spend.create_coin.len() == 1
                        && spend.create_coin.iter().all(|coin| {
                            coin.puzzle_hash == puzzle_hash && Some(coin.amount) == amount
                        })
                }
                None => spend.create_coin.is_empty(),
            }
        });

    if !recreates_vaults_only {
        return Err(DriverError::MixedVaultMessageEnvelope);
    }

    if !aggregate_verify(
        &spend_bundle.aggregated_signature,
        pairs.iter().map(|(pk, msg)| (pk, msg.as_ref())),
    ) {
        return Err(DriverError::InvalidVaultMessageSignature);
    }

    Ok(envelope.messages)
}

/// Parses the launcher id of a vault spend in a proof, and the conditions of the delegated spend that it signed, after
/// checking that its custody is a top-level MIPS puzzle. The delegated spend is read from the same place in the
/// solution that the custody puzzle reads it from, inside of any delegated puzzle wrappers.
///
/// Nothing is trusted yet, so the coin's puzzle hash must then be checked against the expected custody hash, which
/// determines the custody puzzle.
fn parse_signed_vault_spend(
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
) -> Result<(Bytes32, Vec<Condition>), DriverError> {
    let puzzle = node_from_bytes(allocator, &coin_spend.puzzle_reveal)?;
    let solution = node_from_bytes(allocator, &coin_spend.solution)?;

    let puzzle = Puzzle::parse(allocator, puzzle);

    if puzzle.curried_puzzle_hash() != coin_spend.coin.puzzle_hash.into() {
        return Err(DriverError::WrongVaultPuzzleHash);
    }

    let Some(vault) = VaultLayers::parse_puzzle(allocator, puzzle)? else {
        return Err(DriverError::UnsupportedVaultCustody);
    };

    let solution = VaultLayers::parse_solution(allocator, solution)?;
    let delegated_spend = unwrap_delegated_spend(
        allocator,
        Spend::new(
            solution.inner_solution.delegated_puzzle,
            solution.inner_solution.delegated_solution,
        ),
    )?;

    Ok((
        vault.launcher_id,
        parse_delegated_spend(allocator, delegated_spend)?,
    ))
}

type VaultLayers = SingletonLayer<IndexWrapperLayer<NodePtr, DelegatedPuzzleFeederLayer<Puzzle>>>;

/// Removes each [`AddDelegatedPuzzleWrapper`] layer that restrictions add around the delegated spend, to get the
/// delegated spend that was originally signed.
fn unwrap_delegated_spend(
    allocator: &Allocator,
    mut delegated_spend: Spend,
) -> Result<Spend, DriverError> {
    for _ in 0..=VAULT_MESSAGE_PROOF_MAX_WRAPPERS {
        // Only the uncurried program is hashed for each layer, rather than the entire curried puzzle, so that the
        // work is linear in the number of layers.
        let Ok(curried) =
            CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, delegated_spend.puzzle)
        else {
            return Ok(delegated_spend);
        };

        if tree_hash(allocator, curried.program) != AddDelegatedPuzzleWrapper::<(), ()>::mod_hash()
        {
            return Ok(delegated_spend);
        }

        let wrapper =
            AddDelegatedPuzzleWrapper::<NodePtr, NodePtr>::from_clvm(allocator, curried.args)?;

        // The messages in the envelope would satisfy this wrapper, which is meant to require a message to another coin.
        if tree_hash(allocator, wrapper.wrapper) == ForceCoinMessageMod::mod_hash() {
            return Err(DriverError::UnsupportedDelegatedPuzzleWrapper);
        }

        let solution = AddDelegatedPuzzleWrapperSolution::<NodePtr, NodePtr>::from_clvm(
            allocator,
            delegated_spend.solution,
        )?;
        delegated_spend = Spend::new(wrapper.delegated_puzzle, solution.delegated_solution);
    }

    Err(DriverError::TooManyDelegatedPuzzleWrappers)
}

#[cfg(test)]
mod tests;
