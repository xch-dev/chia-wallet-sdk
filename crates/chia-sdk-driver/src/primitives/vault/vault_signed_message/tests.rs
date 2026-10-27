use anyhow::Result;
use chia_bls::SecretKey;
use chia_consensus::opcodes::CREATE_COIN_ANNOUNCEMENT;
use chia_consensus::validation_error::{ErrorCode, ValidationErr};
use chia_protocol::{Bytes, Bytes32, CoinSpend, Program, SpendBundle};
use chia_puzzle_types::Memos;
use chia_sdk_test::{BlsPair, R1Pair, Simulator, SimulatorError, sign_transaction};
use chia_sdk_types::MerkleTree;
use chia_sdk_types::{
    Conditions, MessageFlags, MessageSide, Mod, TESTNET11_CONSTANTS,
    conditions::SendMessage,
    puzzles::{
        BlsMember, MofNArgs, MofNSolution, PasskeyMemberPuzzleAssert,
        PasskeyMemberPuzzleAssertSolution, PreventConditionOpcode, R1MemberPuzzleAssert,
        R1MemberPuzzleAssertSolution, Timelock,
    },
};
use chia_secp::{R1PublicKey, R1SecretKey};
use chia_sha2::Sha256;
use clvm_traits::{clvm_list, clvm_quote};
use clvm_utils::{TreeHash, tree_hash};
use clvmr::NodePtr;
use hex_literal::hex;
use rstest::rstest;

use crate::{
    InnerPuzzleSpend, Launcher, MipsSpend, MipsSpendKind, MofN, Restriction, RestrictionKind,
    Reveals, StandardLayer, calculate_vault_puzzle_message, m_of_n_proof, mips_puzzle_hash,
    parse_vault_transaction,
};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    R1,
    Passkey,
}

#[derive(Debug, Clone)]
struct Key {
    kind: Kind,
    pair: R1Pair,
}

impl Key {
    fn member_puzzle_hash(&self) -> TreeHash {
        match self.kind {
            Kind::R1 => R1MemberPuzzleAssert::new(self.pair.pk).curry_tree_hash(),
            Kind::Passkey => PasskeyMemberPuzzleAssert::new(self.pair.pk).curry_tree_hash(),
        }
    }

    fn mips_hash(&self) -> TreeHash {
        mips_puzzle_hash(0, vec![], self.member_puzzle_hash(), false)
    }
}

/// A vault custody configuration in the same shape that Cloud Wallet uses, which is a 1 of 2 at the top level
/// between the custody signers and a BLS recovery key.
struct TestCustody {
    keys: Vec<Key>,
    threshold: usize,
    recovery: BlsPair,
}

impl TestCustody {
    fn new(kinds: &[Kind], threshold: usize) -> Self {
        Self::with_seed(kinds, threshold, 0)
    }

    fn with_seed(kinds: &[Kind], threshold: usize, seed: u64) -> Self {
        let keys = kinds
            .iter()
            .enumerate()
            .map(|(i, &kind)| Key {
                kind,
                pair: R1Pair::new(seed + i as u64),
            })
            .collect();

        Self {
            keys,
            threshold,
            recovery: BlsPair::new(seed),
        }
    }

    fn signer_hashes(&self) -> Vec<TreeHash> {
        let mut hashes: Vec<TreeHash> = self.keys.iter().map(Key::mips_hash).collect();
        hashes.sort();
        hashes
    }

    fn signers(&self) -> MofN {
        MofN::new(self.threshold, self.signer_hashes())
    }

    fn signers_hash(&self) -> TreeHash {
        if self.keys.len() == 1 {
            self.keys[0].mips_hash()
        } else {
            mips_puzzle_hash(0, vec![], self.signers().inner_puzzle_hash(), false)
        }
    }

    fn recovery_hash(&self) -> TreeHash {
        mips_puzzle_hash(
            0,
            vec![],
            BlsMember::new(self.recovery.pk).curry_tree_hash(),
            false,
        )
    }

    fn top_level(&self) -> MofN {
        MofN::new(1, vec![self.signers_hash(), self.recovery_hash()])
    }

    fn custody_hash(&self) -> TreeHash {
        mips_puzzle_hash(0, vec![], self.top_level().inner_puzzle_hash(), true)
    }

    fn mint(&self, sim: &mut Simulator, ctx: &mut SpendContext) -> Result<Vault> {
        mint_vault(sim, ctx, self.custody_hash())
    }

    fn top_level_spend(&self, delegated_spend: Spend) -> MipsSpend {
        let mut spend = MipsSpend::new(delegated_spend);
        let top_level = self.top_level();
        spend.members.insert(
            self.custody_hash(),
            InnerPuzzleSpend::m_of_n(0, vec![], top_level.required, top_level.items),
        );
        spend
    }

    /// Creates a [`MipsSpend`] of the custody path, where each of the selected signers signs the delegated puzzle hash
    /// for the given vault puzzle hash.
    fn mips_spend(
        &self,
        ctx: &mut SpendContext,
        delegated_spend: Spend,
        signed: Signed,
        selected: &[usize],
    ) -> Result<MipsSpend> {
        let mut spend = self.top_level_spend(delegated_spend);

        if self.keys.len() > 1 {
            let signers = self.signers();
            spend.members.insert(
                self.signers_hash(),
                InnerPuzzleSpend::m_of_n(0, vec![], signers.required, signers.items),
            );
        }

        for &index in selected {
            let key = &self.keys[index];
            let member_spend = sign_member(ctx, key.kind, key.pair.pk, &key.pair.sk, signed)?;
            spend.members.insert(
                key.mips_hash(),
                InnerPuzzleSpend::new(0, vec![], member_spend),
            );
        }

        Ok(spend)
    }

    /// Creates a [`MipsSpend`] of the recovery path, which requires a BLS signature.
    fn recovery_spend(&self, ctx: &mut SpendContext, delegated_spend: Spend) -> Result<MipsSpend> {
        let mut spend = self.top_level_spend(delegated_spend);
        let puzzle = ctx.curry(BlsMember::new(self.recovery.pk))?;
        spend.members.insert(
            self.recovery_hash(),
            InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, NodePtr::NIL)),
        );
        Ok(spend)
    }

    fn signed(ctx: &SpendContext, vault: &Vault, delegated_spend: Spend) -> Signed {
        Signed {
            delegated_puzzle_hash: ctx.tree_hash(delegated_spend.puzzle).into(),
            vault_puzzle_hash: vault.coin.puzzle_hash,
        }
    }

    fn sign_messages(
        &self,
        ctx: &mut SpendContext,
        vault: &Vault,
        messages: &[Bytes],
        selected: &[usize],
    ) -> Result<VaultMessageProof> {
        let envelope = vault_message_envelope(ctx, vault, messages)?;
        let signed = Self::signed(ctx, vault, envelope);
        let spend = self.mips_spend(ctx, envelope, signed, selected)?;
        proof(ctx, vault, &spend, &[])
    }

    fn verify(&self, proof: &VaultMessageProof, vault: &Vault) -> Result<Vec<Bytes>, DriverError> {
        verify_vault_signed_message(
            proof,
            vault.info.launcher_id,
            self.custody_hash().into(),
            TESTNET11_CONSTANTS.genesis_challenge,
        )
    }
}

fn mint_vault(
    sim: &mut Simulator,
    ctx: &mut SpendContext,
    custody_hash: TreeHash,
) -> Result<Vault> {
    let alice = sim.bls(1);
    let (mint_vault, vault) =
        Launcher::new(alice.coin.coin_id(), 1).mint_vault(ctx, custody_hash, ())?;
    StandardLayer::new(alice.pk).spend(ctx, alice.coin, mint_vault)?;
    sim.spend_coins(ctx.take(), &[alice.sk])?;
    Ok(vault)
}

/// A vault custody with a single member at the top level, which has the given restrictions.
struct RestrictedCustody {
    member: NodePtr,
    secret_keys: Vec<SecretKey>,
    restrictions: Vec<(Restriction, Spend)>,
}

impl RestrictedCustody {
    fn new(
        ctx: &mut SpendContext,
        member: NodePtr,
        secret_keys: Vec<SecretKey>,
        restrictions: Vec<(RestrictionKind, NodePtr)>,
    ) -> Self {
        let restrictions = restrictions
            .into_iter()
            .map(|(kind, puzzle)| {
                let restriction = Restriction {
                    kind,
                    puzzle_hash: ctx.tree_hash(puzzle),
                };
                (restriction, Spend::new(puzzle, NodePtr::NIL))
            })
            .collect();

        Self {
            member,
            secret_keys,
            restrictions,
        }
    }

    fn bls(ctx: &mut SpendContext, restrictions: Vec<(RestrictionKind, NodePtr)>) -> Result<Self> {
        let pair = BlsPair::new(0);
        let member = ctx.curry(BlsMember::new(pair.pk))?;
        Ok(Self::new(ctx, member, vec![pair.sk], restrictions))
    }

    fn pair_pk(&self) -> chia_bls::PublicKey {
        self.secret_keys[0].public_key()
    }

    fn custody_hash(&self, ctx: &SpendContext) -> TreeHash {
        mips_puzzle_hash(
            0,
            self.restrictions
                .iter()
                .map(|(restriction, _)| *restriction)
                .collect(),
            ctx.tree_hash(self.member),
            true,
        )
    }

    fn mint(&self, sim: &mut Simulator, ctx: &mut SpendContext) -> Result<Vault> {
        let custody_hash = self.custody_hash(ctx);
        mint_vault(sim, ctx, custody_hash)
    }

    fn sign_messages(
        &self,
        ctx: &mut SpendContext,
        vault: &Vault,
        messages: &[Bytes],
    ) -> Result<VaultMessageProof> {
        let envelope = vault_message_envelope(ctx, vault, messages)?;
        let mut spend = MipsSpend::new(envelope);

        spend.members.insert(
            self.custody_hash(ctx),
            InnerPuzzleSpend::new(
                0,
                self.restrictions
                    .iter()
                    .map(|(restriction, _)| *restriction)
                    .collect(),
                Spend::new(self.member, NodePtr::NIL),
            ),
        );

        for (restriction, restriction_spend) in &self.restrictions {
            spend
                .restrictions
                .insert(restriction.puzzle_hash, *restriction_spend);
        }

        proof(ctx, vault, &spend, &self.secret_keys)
    }

    fn verify(
        &self,
        ctx: &SpendContext,
        proof: &VaultMessageProof,
        vault: &Vault,
    ) -> Result<Vec<Bytes>, DriverError> {
        verify_vault_signed_message(
            proof,
            vault.info.launcher_id,
            self.custody_hash(ctx).into(),
            TESTNET11_CONSTANTS.genesis_challenge,
        )
    }
}

/// Spends the vault and signs it like any other transaction, then turns the spend into a proof.
fn proof(
    ctx: &mut SpendContext,
    vault: &Vault,
    spend: &MipsSpend,
    secret_keys: &[SecretKey],
) -> Result<VaultMessageProof> {
    vault.spend(ctx, spend)?;
    let coin_spends = ctx.take();
    // The signer runs the spend to find the required signatures, so it's skipped for spends that are meant to be
    // invalid and don't need any BLS signatures.
    let signature = if secret_keys.is_empty() {
        Signature::default()
    } else {
        sign_transaction(&coin_spends, secret_keys)?
    };
    assert_eq!(coin_spends.len(), 1);
    Ok(VaultMessageProof::new(
        coin_spends.into_iter().next().unwrap(),
        signature,
    ))
}

/// What a member signs, which is the delegated puzzle hash and the vault puzzle hash.
#[derive(Debug, Clone, Copy)]
struct Signed {
    delegated_puzzle_hash: Bytes32,
    vault_puzzle_hash: Bytes32,
}

impl Signed {
    fn message_hash(self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(calculate_vault_puzzle_message(
            self.delegated_puzzle_hash,
            self.vault_puzzle_hash,
        ));
        hasher.finalize()
    }
}

fn sign_member(
    ctx: &mut SpendContext,
    kind: Kind,
    public_key: R1PublicKey,
    secret_key: &R1SecretKey,
    signed: Signed,
) -> Result<Spend> {
    match kind {
        Kind::R1 => {
            let signature = secret_key.sign_prehashed(&signed.message_hash())?;
            let puzzle = ctx.curry(R1MemberPuzzleAssert::new(public_key))?;
            let solution = ctx.alloc(&R1MemberPuzzleAssertSolution::new(
                signed.vault_puzzle_hash,
                signature,
            ))?;
            Ok(Spend::new(puzzle, solution))
        }
        Kind::Passkey => {
            let challenge = base64_url(&signed.message_hash());
            let client_data_json = format!(
                r#"{{"type":"webauthn.get","challenge":"{challenge}","origin":"https://vault.example.com","crossOrigin":false}}"#
            );
            let challenge_index = client_data_json.find(r#""challenge":"#).unwrap();
            let authenticator_data = Bytes::new([0x49; 37].to_vec());

            let mut client_data_hasher = Sha256::new();
            client_data_hasher.update(client_data_json.as_bytes());

            let mut hasher = Sha256::new();
            hasher.update(&authenticator_data);
            hasher.update(client_data_hasher.finalize());
            let signature = secret_key.sign_prehashed(&hasher.finalize())?;

            let puzzle = ctx.curry(PasskeyMemberPuzzleAssert::new(public_key))?;
            let solution = ctx.alloc(&PasskeyMemberPuzzleAssertSolution {
                authenticator_data,
                client_data_json: client_data_json.into_bytes().into(),
                challenge_index,
                signature,
                puzzle_hash: signed.vault_puzzle_hash,
            })?;
            Ok(Spend::new(puzzle, solution))
        }
    }
}

/// Unpadded `base64url`, which is how `WebAuthn` encodes the challenge in the client data JSON.
fn base64_url(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut output = String::new();

    for chunk in bytes.chunks(3) {
        let mut buffer = [0; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);
        let value = u32::from(buffer[0]) << 16 | u32::from(buffer[1]) << 8 | u32::from(buffer[2]);

        for i in 0..=chunk.len() {
            output.push(CHARSET[(value >> (18 - i * 6)) as usize & 63] as char);
        }
    }

    output
}

fn messages() -> Vec<Bytes> {
    vec![
        Bytes::new(b"Sign in to example.com\nNonce: 4f9a2c\nExpires: 2026-11-01".to_vec()),
        Bytes::new(vec![]),
        Bytes::new(vec![0xff, 0x00, 0x80]),
    ]
}

fn envelope_conditions(
    ctx: &mut SpendContext,
    vault: &Vault,
    messages: &[Bytes],
) -> Result<Conditions> {
    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let mut conditions = Conditions::new();
    for message in messages {
        conditions.push(SendMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![coin_id],
        ));
    }
    Ok(conditions.create_coin(
        vault.info.custody_hash.into(),
        vault.coin.amount,
        Memos::None,
    ))
}

/// Whether the vault's puzzle failed when it was run by consensus, such as because of a failed signature check.
fn is_eval_error(result: &Result<Vec<Bytes>, DriverError>) -> bool {
    matches!(result, Err(DriverError::Validation(ValidationErr::Eval(_))))
}

#[test]
fn test_vault_message_constants() -> Result<()> {
    let mut ctx = SpendContext::new();

    let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
        | MessageFlags::COIN.encode(MessageSide::Receiver);
    assert_eq!(mode, VAULT_MESSAGE_MODE);

    let puzzle = ctx.alloc(&1)?;
    assert_eq!(
        VAULT_MESSAGE_PUZZLE_HASH,
        Bytes32::from(tree_hash(&ctx, puzzle))
    );
    assert_eq!(VAULT_MESSAGE_COIN_ID, vault_message_coin().coin_id());
    assert_eq!(vault_message_coin().parent_coin_info, Bytes32::default());

    Ok(())
}

#[test]
fn test_vault_message_envelope() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    let messages = messages();
    let envelope = vault_message_envelope(&mut ctx, &vault, &messages)?;

    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let mut conditions =
        Conditions::new().create_coin(custody.custody_hash().into(), 1, Memos::None);
    for message in &messages {
        conditions.push(SendMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![coin_id],
        ));
    }
    let expected = ctx.delegated_spend(conditions)?;

    assert_eq!(
        ctx.tree_hash(envelope.puzzle),
        ctx.tree_hash(expected.puzzle)
    );
    assert_eq!(envelope.solution, NodePtr::NIL);

    assert!(matches!(
        vault_message_envelope(&mut ctx, &vault, &[]),
        Err(DriverError::EmptyVaultMessageEnvelope)
    ));

    Ok(())
}

#[rstest]
#[case::r1_1_of_1(&[Kind::R1], 1, &[0])]
#[case::r1_1_of_2(&[Kind::R1, Kind::R1], 1, &[1])]
#[case::r1_2_of_3(&[Kind::R1, Kind::R1, Kind::R1], 2, &[0, 2])]
#[case::r1_3_of_3(&[Kind::R1, Kind::R1, Kind::R1], 3, &[0, 1, 2])]
#[case::passkey_1_of_1(&[Kind::Passkey], 1, &[0])]
#[case::mixed_2_of_3(&[Kind::R1, Kind::Passkey, Kind::R1], 2, &[0, 1])]
#[case::mixed_2_of_3_passkey_only(&[Kind::Passkey, Kind::R1, Kind::Passkey], 2, &[0, 2])]
#[case::mixed_3_of_4(&[Kind::R1, Kind::Passkey, Kind::R1, Kind::Passkey], 3, &[1, 2, 3])]
fn test_verify_vault_signed_message(
    #[case] kinds: &[Kind],
    #[case] threshold: usize,
    #[case] selected: &[usize],
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(kinds, threshold);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages(), selected)?;

    assert_eq!(custody.verify(&proof, &vault)?, messages());

    Ok(())
}

#[test]
fn test_verify_bls_signed_message() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey], 2);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    // The recovery path is signed with an aggregated BLS signature, rather than in the solution.
    let envelope = vault_message_envelope(&mut ctx, &vault, &messages())?;
    let spend = custody.recovery_spend(&mut ctx, envelope)?;
    let signed = proof(
        &mut ctx,
        &vault,
        &spend,
        std::slice::from_ref(&custody.recovery.sk),
    )?;
    assert_eq!(custody.verify(&signed, &vault)?, messages());

    // Without the signature, it's invalid.
    let unsigned = VaultMessageProof::new(signed.coin_spend.clone(), Signature::default());
    assert!(matches!(
        custody.verify(&unsigned, &vault),
        Err(DriverError::InvalidVaultMessageSignature)
    ));

    // AGG_SIG_ME is bound to the network, so it's invalid with a different genesis challenge.
    assert!(matches!(
        verify_vault_signed_message(
            &signed,
            vault.info.launcher_id,
            custody.custody_hash().into(),
            Bytes32::new([1; 32]),
        ),
        Err(DriverError::InvalidVaultMessageSignature)
    ));

    Ok(())
}

#[test]
fn test_vault_message_round_trip() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let messages = messages();

    let envelope = vault_message_envelope(&mut ctx, &vault, &messages)?;

    // Clear signing returns the messages in order, and the vault is recreated as it was.
    let tx = parse_vault_transaction(
        Reveals::default(),
        &mut ctx,
        vault.info.launcher_id,
        envelope,
    )?;
    assert_eq!(tx.signed_messages, messages);
    assert_eq!(
        tx.vault_child,
        Some(VaultOutput::new(custody.custody_hash().into(), 1))
    );
    assert!(tx.spends.is_empty());
    assert!(tx.drop_coins.is_empty());
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(
        tx.delegated_puzzle_hash,
        ctx.tree_hash(envelope.puzzle).into()
    );

    let proof = custody.sign_messages(&mut ctx, &vault, &messages, &[0, 1])?;

    let bytes = proof.encode()?;
    assert_eq!(bytes[0], VAULT_MESSAGE_PROOF_VERSION);
    let decoded = VaultMessageProof::decode(&bytes)?;
    assert_eq!(decoded, proof);

    let string = proof.encode_string()?;
    assert!(string.starts_with("vaultsig1"));
    let decoded = VaultMessageProof::decode_string(&string)?;
    assert_eq!(decoded, proof);

    assert_eq!(custody.verify(&decoded, &vault)?, messages);

    Ok(())
}

#[test]
fn test_vault_message_proof_codec_errors() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages(), &[0])?;

    assert!(matches!(
        VaultMessageProof::decode(&[]),
        Err(DriverError::MissingVaultMessageProofVersion)
    ));

    let mut bytes = proof.encode()?.to_vec();
    bytes[0] = 2;
    assert!(matches!(
        VaultMessageProof::decode(&bytes),
        Err(DriverError::UnsupportedVaultMessageProofVersion(2))
    ));

    let bytes = proof.encode()?;
    assert!(VaultMessageProof::decode(&bytes[..bytes.len() - 1]).is_err());

    let wrong_prefix = Bech32::new(proof.encode()?, "offer".to_string()).encode()?;
    assert!(VaultMessageProof::decode_string(&wrong_prefix).is_err());

    Ok(())
}

#[test]
fn test_reject_below_threshold() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    let envelope = vault_message_envelope(&mut ctx, &vault, &messages())?;
    let signed = TestCustody::signed(&ctx, &vault, envelope);

    // The driver refuses to build a spend with too few signers.
    let spend = custody.mips_spend(&mut ctx, envelope, signed, &[0])?;
    assert!(matches!(
        spend.spend(&mut ctx, custody.custody_hash()),
        Err(DriverError::InvalidSubpathSpendCount)
    ));

    // So build the m of n solution by hand, with only one of the two required members revealed.
    let signers = custody.signers();
    let key = &custody.keys[0];
    let member_spend = sign_member(&mut ctx, key.kind, key.pair.pk, &key.pair.sk, signed)?;
    let member_spend = InnerPuzzleSpend::new(0, vec![], member_spend).spend(
        &mut ctx,
        &spend,
        &mut Vec::new(),
        false,
    )?;

    let leaves = signers
        .items
        .iter()
        .map(|&item| {
            let revealed = item == key.mips_hash();
            (item.into(), revealed.then_some(member_spend))
        })
        .collect::<Vec<(Bytes32, Option<Spend>)>>();
    let leaf_hashes = signers
        .items
        .iter()
        .map(|&item| item.into())
        .collect::<Vec<Bytes32>>();

    let merkle_proof = m_of_n_proof(&mut ctx, &leaves)?;
    let puzzle = ctx.curry(MofNArgs::new(2, MerkleTree::new(&leaf_hashes).root()))?;
    let solution = ctx.alloc(&MofNSolution::new(merkle_proof))?;

    let mut spend = spend;
    spend.members.insert(
        custody.signers_hash(),
        InnerPuzzleSpend {
            nonce: 0,
            restrictions: vec![],
            kind: MipsSpendKind::Member(Spend::new(puzzle, solution)),
        },
    );

    let proof = proof(&mut ctx, &vault, &spend, &[])?;
    assert!(is_eval_error(&custody.verify(&proof, &vault)));

    Ok(())
}

#[rstest]
#[case::r1(Kind::R1)]
#[case::passkey(Kind::Passkey)]
fn test_reject_invalid_signature(#[case] kind: Kind) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, kind, Kind::R1], 2);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    let envelope = vault_message_envelope(&mut ctx, &vault, &messages())?;
    let signed = TestCustody::signed(&ctx, &vault, envelope);

    let mut spend = custody.mips_spend(&mut ctx, envelope, signed, &[0, 1])?;

    // The second member is signed by a key that isn't a member of the vault.
    let key = &custody.keys[1];
    let member_spend = sign_member(
        &mut ctx,
        key.kind,
        key.pair.pk,
        &R1Pair::new(1000).sk,
        signed,
    )?;
    spend.members.insert(
        key.mips_hash(),
        InnerPuzzleSpend::new(0, vec![], member_spend),
    );

    let proof = proof(&mut ctx, &vault, &spend, &[])?;
    assert!(is_eval_error(&custody.verify(&proof, &vault)));

    Ok(())
}

#[test]
fn test_reject_wrong_vault() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey], 1);
    let other = TestCustody::with_seed(&[Kind::R1, Kind::Passkey], 1, 100);

    let vault = custody.mint(&mut sim, &mut ctx)?;
    let other_vault = custody.mint(&mut sim, &mut ctx)?;

    let proof = custody.sign_messages(&mut ctx, &vault, &messages(), &[1])?;

    // The proof is for a different vault than the one expected.
    assert!(matches!(
        custody.verify(&proof, &other_vault),
        Err(DriverError::WrongVaultPuzzleHash)
    ));

    // Or for a different custody configuration of the same vault.
    assert!(matches!(
        other.verify(&proof, &vault),
        Err(DriverError::WrongVaultPuzzleHash)
    ));

    // A proof that claims to be for the expected vault, but reveals a puzzle with a different hash. It's rejected
    // before the puzzle is run.
    let other_vault = other.mint(&mut sim, &mut ctx)?;
    let mut forged = other.sign_messages(&mut ctx, &other_vault, &messages(), &[0])?;
    forged.coin_spend.coin.puzzle_hash = vault.coin.puzzle_hash;
    forged.coin_spend.coin.parent_coin_info = vault.coin.parent_coin_info;
    assert!(matches!(
        custody.verify(&forged, &vault),
        Err(DriverError::WrongVaultPuzzleHash)
    ));

    // Even if the puzzle reveal would run until it reaches the maximum cost, since it's never run. This is the
    // program `(a 1 1)`, which runs its solution with itself as the solution.
    let infinite_loop = Program::from(hex!("ff02ff01ff0180").to_vec());
    let mut forged = proof.clone();
    forged.coin_spend.puzzle_reveal = infinite_loop.clone();
    forged.coin_spend.solution = infinite_loop;
    assert!(matches!(
        custody.verify(&forged, &vault),
        Err(DriverError::WrongVaultPuzzleHash)
    ));

    Ok(())
}

#[rstest]
#[case::changed(vec![b"hello".to_vec(), b"world".to_vec()], vec![b"hello".to_vec(), b"World".to_vec()])]
#[case::reordered(vec![b"hello".to_vec(), b"world".to_vec()], vec![b"world".to_vec(), b"hello".to_vec()])]
#[case::removed(vec![b"hello".to_vec(), b"world".to_vec()], vec![b"hello".to_vec()])]
#[case::added(vec![b"hello".to_vec()], vec![b"hello".to_vec(), b"world".to_vec()])]
fn test_reject_changed_messages(
    #[case] signed_messages: Vec<Vec<u8>>,
    #[case] revealed_messages: Vec<Vec<u8>>,
) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    let signed_messages: Vec<Bytes> = signed_messages.into_iter().map(Bytes::new).collect();
    let revealed_messages: Vec<Bytes> = revealed_messages.into_iter().map(Bytes::new).collect();

    let signed_envelope = vault_message_envelope(&mut ctx, &vault, &signed_messages)?;
    let revealed_envelope = vault_message_envelope(&mut ctx, &vault, &revealed_messages)?;

    let signed = TestCustody::signed(&ctx, &vault, signed_envelope);
    let spend = custody.mips_spend(&mut ctx, revealed_envelope, signed, &[0, 1])?;
    let proof = proof(&mut ctx, &vault, &spend, &[])?;

    assert!(is_eval_error(&custody.verify(&proof, &vault)));

    Ok(())
}

#[test]
fn test_reject_vault_not_recreated() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    // Only the messages, without recreating the vault, which would melt it.
    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let delegated_spend = ctx.delegated_spend(Conditions::new().with(SendMessage::new(
        VAULT_MESSAGE_MODE,
        Bytes::new(b"hello".to_vec()),
        vec![coin_id],
    )))?;

    // Otherwise, clear signing would show the vault being melted.
    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            vault.info.launcher_id,
            delegated_spend
        ),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;

    vault.spend(&mut ctx, &spend)?;
    let coin_spend = ctx.take().remove(0);
    let proof = VaultMessageProof::new(coin_spend, Signature::default());

    assert!(matches!(
        custody.verify(&proof, &vault),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    Ok(())
}

#[test]
fn test_reject_without_messages() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    // A normal spend that only recreates the vault doesn't sign any messages.
    let conditions = envelope_conditions(&mut ctx, &vault, &[])?;
    let delegated_spend = ctx.delegated_spend(conditions)?;
    let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    let proof = proof(&mut ctx, &vault, &spend, &[])?;

    assert!(matches!(
        custody.verify(&proof, &vault),
        Err(DriverError::EmptyVaultMessageEnvelope)
    ));

    Ok(())
}

#[test]
fn test_reject_message_to_real_coin() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    let real = ctx.alloc(&Bytes32::new([1; 32]))?;
    let conditions = envelope_conditions(&mut ctx, &vault, &messages())?.with(SendMessage::new(
        VAULT_MESSAGE_MODE,
        Bytes::new(b"world".to_vec()),
        vec![real],
    ));
    let delegated_spend = ctx.delegated_spend(conditions)?;

    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            vault.info.launcher_id,
            delegated_spend
        ),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    let proof = proof(&mut ctx, &vault, &spend, &[])?;

    // The verifier requires the same envelope as clear signing.
    assert!(matches!(
        custody.verify(&proof, &vault),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    Ok(())
}

#[rstest]
#[case::create_coin(Conditions::new().create_coin(Bytes32::new([1; 32]), 0, Memos::None))]
#[case::reserve_fee(Conditions::new().reserve_fee(1))]
#[case::assertion(Conditions::new().assert_seconds_absolute(100))]
#[case::remark(Conditions::new().remark(NodePtr::NIL))]
#[case::second_child(Conditions::new().create_coin(Bytes32::new([1; 32]), 1, Memos::None))]
fn test_reject_extra_condition(#[case] extra: Conditions) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    let conditions = envelope_conditions(&mut ctx, &vault, &messages())?.extend(extra);
    let delegated_spend = ctx.delegated_spend(conditions)?;

    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            vault.info.launcher_id,
            delegated_spend
        ),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    // Even if the extra conditions are signed and valid, the verifier rejects them.
    let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    let proof = proof(&mut ctx, &vault, &spend, &[])?;
    assert!(matches!(
        custody.verify(&proof, &vault),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    Ok(())
}

#[test]
fn test_reject_changed_vault() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let other = TestCustody::with_seed(&[Kind::R1], 1, 100);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    for (custody_hash, amount) in [(other.custody_hash(), 1), (custody.custody_hash(), 3)] {
        let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
        let conditions = Conditions::new()
            .create_coin(custody_hash.into(), amount, Memos::None)
            .with(SendMessage::new(
                VAULT_MESSAGE_MODE,
                Bytes::new(b"hello".to_vec()),
                vec![coin_id],
            ));
        let delegated_spend = ctx.delegated_spend(conditions)?;

        // Clear signing doesn't know the current custody hash, so it shows the vault being recreated as signed.
        let tx = parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            vault.info.launcher_id,
            delegated_spend,
        )?;
        assert_eq!(
            tx.vault_child,
            Some(VaultOutput::new(custody_hash.into(), amount))
        );

        let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
        let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
        let proof = proof(&mut ctx, &vault, &spend, &[])?;

        assert!(matches!(
            custody.verify(&proof, &vault),
            Err(DriverError::VaultMessageEnvelopeChangesVault)
        ));
    }

    Ok(())
}

#[test]
fn test_reject_unquoted_delegated_puzzle() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    // A signature for the delegated puzzle `1` would allow anyone to choose the conditions in its solution, so a
    // delegated puzzle that isn't a quoted list of conditions is rejected, even if it outputs a valid envelope.
    let puzzle = ctx.alloc(&1)?;
    let solution = envelope_conditions(&mut ctx, &vault, &messages())?;
    let solution = ctx.alloc(&solution)?;
    let delegated_spend = Spend::new(puzzle, solution);

    let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    let proof = proof(&mut ctx, &vault, &spend, &[])?;

    assert!(matches!(
        custody.verify(&proof, &vault),
        Err(DriverError::InvalidDelegatedSpendFormat)
    ));

    Ok(())
}

#[test]
fn test_verify_wrapped_delegated_puzzle() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    // The delegated puzzle is wrapped by a restriction, which the verifier looks inside of.
    let wrapper = ctx.curry(PreventConditionOpcode::new(CREATE_COIN_ANNOUNCEMENT))?;
    let custody = RestrictedCustody::bls(
        &mut ctx,
        vec![(RestrictionKind::DelegatedPuzzleWrapper, wrapper)],
    )?;
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages())?;

    assert_eq!(custody.verify(&ctx, &proof, &vault)?, messages());

    Ok(())
}

#[test]
fn test_reject_timelocked_member() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    // A member that can spend the vault without a signature once a timelock has passed, like the member that
    // finishes a recovery. The verifier can't know whether the timelock has passed, so it can't sign messages.
    let member = ctx.alloc(&clvm_quote!(Conditions::new().assert_seconds_relative(100)))?;
    let timelock = ctx.curry(Timelock::new(100))?;
    let custody = RestrictedCustody::new(
        &mut ctx,
        member,
        vec![],
        vec![(RestrictionKind::MemberCondition, timelock)],
    );
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages())?;

    assert!(matches!(
        custody.verify(&ctx, &proof, &vault),
        Err(DriverError::VaultMessageTimelocked)
    ));

    Ok(())
}

#[rstest]
#[case::seconds_relative(Conditions::new().assert_seconds_relative(100))]
#[case::height_relative(Conditions::new().assert_height_relative(100))]
#[case::seconds_absolute(Conditions::new().assert_seconds_absolute(100))]
#[case::height_absolute(Conditions::new().assert_height_absolute(100))]
#[case::before_seconds_relative(Conditions::new().assert_before_seconds_relative(100))]
#[case::before_height_relative(Conditions::new().assert_before_height_relative(100))]
#[case::before_seconds_absolute(Conditions::new().assert_before_seconds_absolute(100))]
#[case::before_height_absolute(Conditions::new().assert_before_height_absolute(100))]
#[case::birth_seconds(Conditions::new().assert_my_birth_seconds(100))]
#[case::birth_height(Conditions::new().assert_my_birth_height(100))]
fn test_reject_height_or_time_assertion(#[case] conditions: Conditions) -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    // A member that can only spend after a timelock, or before a deadline, could otherwise sign messages at any time.
    let member = ctx.alloc(&clvm_quote!(conditions))?;
    let custody = RestrictedCustody::new(&mut ctx, member, vec![], vec![]);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages())?;

    assert!(matches!(
        custody.verify(&ctx, &proof, &vault),
        Err(DriverError::VaultMessageTimelocked)
    ));

    Ok(())
}

#[test]
fn test_reject_wrapper_that_changes_output() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    // The delegated spend is a valid envelope, but a delegated puzzle wrapper adds a coin to the vault's output, which
    // is `(c (q 51 <puzzle hash> 0) 2)`. The coin has no value, so consensus alone would allow it, but the wrapper isn't
    // a known MIPS wrapper.
    let wrapper = ctx.alloc(&clvm_list!(
        4,
        clvm_quote!(clvm_list!(51, Bytes32::new([2; 32]), 0)),
        2
    ))?;
    let custody = RestrictedCustody::bls(
        &mut ctx,
        vec![(RestrictionKind::DelegatedPuzzleWrapper, wrapper)],
    )?;
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages())?;

    assert!(matches!(
        custody.verify(&ctx, &proof, &vault),
        Err(DriverError::UnsupportedDelegatedPuzzleWrapper)
    ));

    Ok(())
}

#[test]
fn test_reject_unsigned_proof_that_never_finishes() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = RestrictedCustody::bls(&mut ctx, vec![])?;
    let vault = custody.mint(&mut sim, &mut ctx)?;

    // Anyone can wrap the envelope in a program that never finishes, which is `(a (q . (a 2 1)) (q . ((a 2 1))))`.
    // A BLS member's signature is only checked after the spend is run, so unknown wrappers are rejected before then.
    let program = ctx.alloc(&clvm_list!(2, 2, 1))?;
    let wrapper = ctx.alloc(&clvm_list!(
        2,
        clvm_quote!(program),
        clvm_quote!(clvm_list!(program))
    ))?;
    let envelope = vault_message_envelope(&mut ctx, &vault, &messages())?;
    let puzzle = ctx.curry(AddDelegatedPuzzleWrapper::new(wrapper, envelope.puzzle))?;
    let solution = ctx.alloc(&AddDelegatedPuzzleWrapperSolution::new(
        NodePtr::NIL,
        envelope.solution,
    ))?;

    let mut spend = MipsSpend::new(Spend::new(puzzle, solution));
    let member = ctx.curry(BlsMember::new(custody.pair_pk()))?;
    spend.members.insert(
        custody.custody_hash(&ctx),
        InnerPuzzleSpend::new(0, vec![], Spend::new(member, NodePtr::NIL)),
    );
    let proof = proof(&mut ctx, &vault, &spend, &[])?;

    assert!(matches!(
        custody.verify(&ctx, &proof, &vault),
        Err(DriverError::UnsupportedDelegatedPuzzleWrapper)
    ));

    Ok(())
}

#[test]
fn test_reject_too_many_wrappers() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let wrappers = (0..=VAULT_MESSAGE_PROOF_MAX_WRAPPERS)
        .map(|i| {
            let opcode = 1000 + u16::try_from(i)?;
            let wrapper = ctx.curry(PreventConditionOpcode::new(opcode))?;
            Ok((RestrictionKind::DelegatedPuzzleWrapper, wrapper))
        })
        .collect::<Result<Vec<_>>>()?;
    let custody = RestrictedCustody::bls(&mut ctx, wrappers)?;
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages())?;

    assert!(matches!(
        custody.verify(&ctx, &proof, &vault),
        Err(DriverError::TooManyDelegatedPuzzleWrappers)
    ));

    Ok(())
}

#[test]
fn test_reject_large_proof() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let mut proof = custody.sign_messages(&mut ctx, &vault, &messages(), &[0])?;

    // The size is checked before anything is parsed.
    proof.coin_spend.solution = Program::from(vec![0x80; VAULT_MESSAGE_PROOF_MAX_SIZE]);
    assert!(matches!(
        custody.verify(&proof, &vault),
        Err(DriverError::VaultMessageProofTooLarge)
    ));

    // And before it's decoded.
    assert!(matches!(
        VaultMessageProof::decode(&proof.encode()?),
        Err(DriverError::VaultMessageProofTooLarge)
    ));
    assert!(matches!(
        VaultMessageProof::decode_string(&"a".repeat(1_000_000)),
        Err(DriverError::VaultMessageProofTooLarge)
    ));

    Ok(())
}

#[test]
fn test_message_length_limit() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;

    // The longest message that consensus allows can be signed and verified.
    let longest = vec![Bytes::new(vec![1; VAULT_MESSAGE_MAX_LENGTH])];
    let proof = custody.sign_messages(&mut ctx, &vault, &longest, &[0])?;
    assert_eq!(custody.verify(&proof, &vault)?, longest);

    // A longer one could never be verified, so it can't be signed.
    let too_long = vec![Bytes::new(vec![1; VAULT_MESSAGE_MAX_LENGTH + 1])];
    assert!(matches!(
        vault_message_envelope(&mut ctx, &vault, &too_long),
        Err(DriverError::VaultMessageTooLong)
    ));

    let conditions = envelope_conditions(&mut ctx, &vault, &too_long)?;
    let delegated_spend = ctx.delegated_spend(conditions)?;
    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            vault.info.launcher_id,
            delegated_spend
        ),
        Err(DriverError::VaultMessageTooLong)
    ));

    Ok(())
}

#[test]
fn test_vault_message_proof_spend_fails() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let messages = messages();

    let proof = custody.sign_messages(&mut ctx, &vault, &messages, &[0, 1])?;
    assert_eq!(custody.verify(&proof, &vault)?, messages);

    // The proof is a valid vault spend, but the messages can't be received on chain.
    let spend_bundle = SpendBundle::new(
        vec![proof.coin_spend.clone()],
        proof.aggregated_signature.clone(),
    );
    assert!(matches!(
        sim.new_transaction(spend_bundle),
        Err(SimulatorError::Validation(
            ErrorCode::MessageNotSentOrReceived
        ))
    ));

    // And the coin that receives them doesn't exist.
    let puzzle = ctx.alloc(&1)?;
    let coin_id = ctx.alloc(&vault.coin.puzzle_hash)?;
    let mut receive = Conditions::new();
    for message in &messages {
        receive.push(chia_sdk_types::conditions::ReceiveMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![coin_id],
        ));
    }
    let solution = ctx.alloc(&receive)?;
    let receiver = CoinSpend::new(
        vault_message_coin(),
        ctx.serialize(&puzzle)?,
        ctx.serialize(&solution)?,
    );
    let spend_bundle = SpendBundle::new(
        vec![proof.coin_spend.clone(), receiver],
        proof.aggregated_signature.clone(),
    );
    assert!(matches!(
        sim.new_transaction(spend_bundle),
        Err(SimulatorError::Validation(ErrorCode::UnknownUnspent))
    ));

    Ok(())
}

#[test]
fn test_proof_does_not_check_coin_state() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let vault = custody.mint(&mut sim, &mut ctx)?;
    let proof = custody.sign_messages(&mut ctx, &vault, &messages(), &[0])?;

    // After the vault coin is spent, the proof is still valid, since verification doesn't use the chain.
    let conditions = envelope_conditions(&mut ctx, &vault, &[])?;
    let delegated_spend = ctx.delegated_spend(conditions)?;
    let signed = TestCustody::signed(&ctx, &vault, delegated_spend);
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    vault.spend(&mut ctx, &spend)?;
    sim.spend_coins(ctx.take(), &[])?;

    assert!(
        sim.coin_state(vault.coin.coin_id())
            .unwrap()
            .spent_height
            .is_some()
    );
    assert_eq!(custody.verify(&proof, &vault)?, messages());

    Ok(())
}

#[test]
fn test_normal_vault_messages_unchanged() -> Result<()> {
    let mut ctx = SpendContext::new();

    // A message to a real coin still requires the coin spend to be revealed.
    let real = ctx.alloc(&Bytes32::new([1; 32]))?;
    let delegated_spend = ctx.delegated_spend(Conditions::new().with(SendMessage::new(
        VAULT_MESSAGE_MODE,
        Bytes::new(vec![0; 32]),
        vec![real],
    )))?;

    assert!(matches!(
        parse_vault_transaction(
            Reveals::default(),
            &mut ctx,
            Bytes32::default(),
            delegated_spend
        ),
        Err(DriverError::MissingSpend)
    ));

    // A delegated spend without any messages doesn't sign any messages.
    let delegated_spend = ctx.delegated_spend(Conditions::new().reserve_fee(1))?;
    let tx = parse_vault_transaction(
        Reveals::default(),
        &mut ctx,
        Bytes32::default(),
        delegated_spend,
    )?;
    assert!(tx.signed_messages.is_empty());

    Ok(())
}
