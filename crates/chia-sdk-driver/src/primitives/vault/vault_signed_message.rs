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
    puzzles::{
        AddDelegatedPuzzleWrapper, AddDelegatedPuzzleWrapperSolution, Force1of2RestrictedVariable,
        ForceAssertCoinAnnouncementMod, ForceCoinMessageMod, ForceSingletonRecreationMod,
        PreventConditionOpcode, PreventMultipleCreateCoinsMod, Timelock,
    },
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

use crate::{
    DelegatedPuzzleFeederLayer, DriverError, IndexWrapperLayer, Layer, Puzzle, SingletonLayer,
    Spend, SpendContext, Vault, VaultOutput, parse_delegated_spend, parse_vault_message,
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

/// The version byte that prefixes an encoded [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_VERSION: u8 = 1;

/// The bech32m prefix of the string form of a [`VaultMessageProof`].
pub const VAULT_MESSAGE_PROOF_PREFIX: &str = "vaultsig";

/// The maximum length of each message in a vault message envelope, which is the consensus limit for the message in a
/// `SEND_MESSAGE` condition.
pub const VAULT_MESSAGE_MAX_LENGTH: usize = 1024;

/// The maximum combined size in bytes of the puzzle reveal and solution in a [`VaultMessageProof`], which bounds the
/// work done before the spend is run, and the data that the vault's puzzle runs on. A 3 of 4 vault with passkey members
/// is about 6.5 KB.
pub const VAULT_MESSAGE_PROOF_MAX_SIZE: usize = 65_536;

/// The maximum cost of running the spends in a [`VaultMessageProof`], which bounds the work done to verify it. This
/// includes the cost of each byte of the spend, and a 3 of 4 vault with passkey members costs about 214 million.
pub const VAULT_MESSAGE_PROOF_MAX_COST: u64 = 1_000_000_000;

/// The maximum size of an encoded [`VaultMessageProof`] that can be verified, which allows for the coin, the length
/// prefixes, the signature, and the version byte in addition to the puzzle reveal and solution.
const MAX_ENCODED_PROOF_SIZE: usize = VAULT_MESSAGE_PROOF_MAX_SIZE + 1024;

/// The maximum number of delegated puzzle wrappers around the delegated spend in a [`VaultMessageProof`]. Each one
/// must be one of the wrappers that MIPS restrictions use, such as [`PreventConditionOpcode`].
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
    ///
    /// Proofs that are too large to verify are rejected before they're deserialized.
    pub fn decode(bytes: &[u8]) -> Result<Self, DriverError> {
        if bytes.len() > MAX_ENCODED_PROOF_SIZE {
            return Err(DriverError::VaultMessageProofTooLarge);
        }

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
    ///
    /// Proofs that are too large to verify are rejected before they're decoded.
    pub fn decode_string(proof: &str) -> Result<Self, DriverError> {
        // Each character of a bech32m string encodes 5 bits, and the prefix and checksum are much less than 1024.
        if proof.len() > MAX_ENCODED_PROOF_SIZE * 8 / 5 + 1024 {
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
/// kinds of members the vault has, since the vault's puzzle performs its own signature checks.
///
/// # What is verified
///
/// - The vault coin's puzzle hash, and the hash of the puzzle reveal, are the singleton puzzle hash for the expected
///   launcher id and custody hash.
/// - The custody is a top-level MIPS puzzle, and the delegated puzzle in the solution (inside of any known MIPS
///   delegated puzzle wrappers) is a quoted list of conditions, which only sends messages to [`VAULT_MESSAGE_COIN_ID`] and recreates the
///   vault with the same custody hash and amount. This is the same check that clear signing does, so a delegated spend
///   that would be shown as a transaction can't be used as a message signature.
/// - The puzzle reveal runs successfully on the solution, with the consensus rules that are active today in mempool
///   mode, so operators and conditions that aren't known or active yet are rejected. This is where the vault's
///   secp256r1, passkey and BLS members, m-of-n thresholds, restrictions, and singleton rules are checked.
/// - The vault's actual output, after any delegated puzzle wrappers, creates no coin other than the recreated vault
///   and reserves no fee.
/// - Any assertions about the coin itself (such as `ASSERT_MY_PUZZLEHASH`, `ASSERT_MY_COIN_ID`, `ASSERT_MY_PARENT_ID`,
///   and `ASSERT_MY_AMOUNT`) hold for the coin in the proof.
/// - Conditions between the spends in the bundle hold. Every message must be received the right number of times, so
///   the vault must have sent exactly the returned messages to [`VAULT_MESSAGE_COIN_ID`] and no other messages.
/// - There are no assertions about height or time at all, such as `ASSERT_SECONDS_RELATIVE`,
///   `ASSERT_BEFORE_HEIGHT_ABSOLUTE`, or `ASSERT_MY_BIRTH_HEIGHT`, since they can't be checked without the chain.
///   Otherwise, a member that is only allowed to spend after a timelock (such as during recovery), or before a
///   deadline, could sign messages at any time.
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
/// - There's no minimum fee, and the limits are [`VAULT_MESSAGE_PROOF_MAX_SIZE`] and [`VAULT_MESSAGE_PROOF_MAX_COST`]
///   rather than what's left in a block after other transactions.
/// - The coins created by the vault spend aren't checked to be new, and nothing is checked against other spends in
///   the same block or mempool, such as conflicting spends of the vault coin.
///
/// The expected custody hash must come from a trusted source, such as the vault's coin on chain. Without it, anyone
/// could make up a custody puzzle for any launcher id that doesn't require a signature. Since a vault's custody hash
/// changes when its signers are changed, this also determines which signers the proof is checked against.
///
/// # Denial of service
///
/// The proof is untrusted, so the work it can cause is bounded before a signature is checked:
///
/// - Its puzzle reveal and solution are at most [`VAULT_MESSAGE_PROOF_MAX_SIZE`] bytes, which is checked before they're
///   parsed. Back-references aren't supported, so that the size of each program is the size of its serialization.
/// - Everything before the spends are run is linear in the size, including at most
///   [`VAULT_MESSAGE_PROOF_MAX_WRAPPERS`] delegated puzzle wrappers.
/// - The only code that runs is the vault's puzzle, which is committed to by the custody hash, the known MIPS
///   delegated puzzle wrappers, and the quoted delegated puzzle. So an unsigned proof can only choose the data that this
///   code runs on, rather than running arbitrary code.
/// - Running the spends is limited to [`VAULT_MESSAGE_PROOF_MAX_COST`], which also limits the number of BLS
///   signatures that are checked.
///
/// Custody with other kinds of delegated puzzle wrappers isn't supported.
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
) -> Result<Vec<Bytes>, DriverError> {
    let coin_spend = &proof.coin_spend;

    if coin_spend.puzzle_reveal.len() + coin_spend.solution.len() > VAULT_MESSAGE_PROOF_MAX_SIZE {
        return Err(DriverError::VaultMessageProofTooLarge);
    }

    let vault_puzzle_hash: Bytes32 =
        SingletonArgs::curry_tree_hash(expected_launcher_id, TreeHash::from(expected_custody_hash))
            .into();

    if coin_spend.coin.puzzle_hash != vault_puzzle_hash {
        return Err(DriverError::WrongVaultPuzzleHash);
    }

    let constants = default_constants(genesis_challenge, genesis_challenge);

    // The consensus rules that are active today are used, in mempool mode, so that operators and conditions that
    // aren't known or active yet fail rather than being treated as no-ops.
    let flags = get_flags_for_height_and_constants(
        constants.hard_fork2_height.saturating_sub(1),
        &constants,
    ) | MEMPOOL_MODE;
    let mut allocator = make_allocator(flags);

    let puzzle = node_from_bytes(&mut allocator, &coin_spend.puzzle_reveal)?;
    let solution = node_from_bytes(&mut allocator, &coin_spend.solution)?;

    // The puzzle reveal is checked before it's run. Since it matches the expected vault, its custody is the expected
    // custody puzzle.
    let puzzle = Puzzle::parse(&allocator, puzzle);

    if puzzle.curried_puzzle_hash() != vault_puzzle_hash.into() {
        return Err(DriverError::WrongVaultPuzzleHash);
    }

    let Some(vault) = VaultLayers::parse_puzzle(&allocator, puzzle)? else {
        return Err(DriverError::UnsupportedVaultCustody);
    };

    // The delegated spend is what the vault's signers signed. It's read from the same place in the solution that the
    // custody puzzle reads it from, and its conditions must be exactly a vault message envelope.
    let solution = VaultLayers::parse_solution(&allocator, solution)?;
    let delegated_spend = unwrap_delegated_spend(
        &allocator,
        Spend::new(
            solution.inner_solution.delegated_puzzle,
            solution.inner_solution.delegated_solution,
        ),
    )?;
    let conditions = parse_delegated_spend(&allocator, delegated_spend)?;
    let envelope = parse_vault_message_envelope(&allocator, &conditions)?
        .ok_or(DriverError::EmptyVaultMessageEnvelope)?;

    if envelope.vault_child != VaultOutput::new(expected_custody_hash, coin_spend.coin.amount) {
        return Err(DriverError::VaultMessageEnvelopeChangesVault);
    }

    debug_assert_eq!(vault.launcher_id, expected_launcher_id);

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
        vec![
            coin_spend.clone(),
            CoinSpend::new(
                vault_message_coin(),
                // The puzzle `1`, which returns its solution as its conditions.
                Program::from(vec![1]),
                Program::from(node_to_bytes(&allocator, receive_conditions)?),
            ),
        ],
        proof.aggregated_signature.clone(),
    );

    // No timeout is used, since measuring time isn't supported on every platform (such as WebAssembly).
    let (conditions, pairs) = run_spendbundle(
        &mut allocator,
        &spend_bundle,
        VAULT_MESSAGE_PROOF_MAX_COST,
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

    // Delegated puzzle wrappers can change the vault's output, so the output itself must also only recreate the vault.
    let recreates_vault_only = conditions.reserve_fee == 0
        && conditions.spends.iter().all(|spend| {
            let is_vault = *spend.coin_id == coin_spend.coin.coin_id();
            let expected = usize::from(is_vault);

            spend.create_coin.len() == expected
                && spend.create_coin.iter().all(|coin| {
                    coin.puzzle_hash == vault_puzzle_hash && coin.amount == coin_spend.coin.amount
                })
        });

    if !recreates_vault_only {
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

        if !is_known_wrapper(allocator, wrapper.wrapper) {
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

/// Whether the delegated puzzle wrapper is one that MIPS restrictions use.
///
/// The delegated puzzle and its wrappers are the only code in a proof that isn't committed to by the custody hash, so
/// allowing only known wrappers means that an unsigned proof can't make the verifier run arbitrary code. The curried
/// arguments are parsed with their types, which ensures that they're quoted values rather than code.
fn is_known_wrapper(allocator: &Allocator, wrapper: NodePtr) -> bool {
    let hash = tree_hash(allocator, wrapper);

    if [
        ForceAssertCoinAnnouncementMod::mod_hash(),
        ForceCoinMessageMod::mod_hash(),
        ForceSingletonRecreationMod::mod_hash(),
        PreventMultipleCreateCoinsMod::mod_hash(),
    ]
    .contains(&hash)
    {
        return true;
    }

    let Ok(curried) = CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, wrapper) else {
        return false;
    };

    let mod_hash = tree_hash(allocator, curried.program);

    if mod_hash == PreventConditionOpcode::mod_hash() {
        PreventConditionOpcode::from_clvm(allocator, curried.args).is_ok()
    } else if mod_hash == Timelock::mod_hash() {
        Timelock::from_clvm(allocator, curried.args).is_ok()
    } else if mod_hash == Force1of2RestrictedVariable::mod_hash() {
        Force1of2RestrictedVariable::from_clvm(allocator, curried.args).is_ok()
    } else {
        false
    }
}

#[cfg(test)]
mod tests;
