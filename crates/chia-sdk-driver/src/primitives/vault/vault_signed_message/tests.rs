use anyhow::Result;
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::{Memos, singleton::SingletonArgs};
use chia_sdk_test::{BlsPair, R1Pair, Simulator, SimulatorError};
use chia_sdk_types::{
    Conditions, MerkleTree, MessageFlags, MessageSide, Mod,
    conditions::SendMessage,
    puzzles::{
        BlsMember, MofNArgs, MofNSolution, PasskeyMemberPuzzleAssert,
        PasskeyMemberPuzzleAssertSolution, R1MemberPuzzleAssert, R1MemberPuzzleAssertSolution,
    },
};
use chia_secp::{R1PublicKey, R1SecretKey};
use chia_sha2::Sha256;
use clvm_utils::TreeHash;
use clvmr::NodePtr;
use rstest::rstest;

use crate::{
    InnerPuzzleSpend, Launcher, MipsSpend, MipsSpendKind, MofN, Reveals, StandardLayer,
    calculate_vault_puzzle_message, m_of_n_proof, mips_puzzle_hash, parse_vault_transaction,
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
/// between the custody signers and the recovery signers.
struct TestCustody {
    keys: Vec<Key>,
    threshold: usize,
    recovery_hash: TreeHash,
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

        let recovery_hash = mips_puzzle_hash(
            0,
            vec![],
            BlsMember::new(BlsPair::new(seed).pk).curry_tree_hash(),
            false,
        );

        Self {
            keys,
            threshold,
            recovery_hash,
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

    fn top_level(&self) -> MofN {
        MofN::new(1, vec![self.signers_hash(), self.recovery_hash])
    }

    fn custody_hash(&self) -> TreeHash {
        mips_puzzle_hash(0, vec![], self.top_level().inner_puzzle_hash(), true)
    }

    fn vault_puzzle_hash(&self, launcher_id: Bytes32) -> Bytes32 {
        SingletonArgs::curry_tree_hash(launcher_id, self.custody_hash()).into()
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
        let mut spend = MipsSpend::new(delegated_spend);

        let top_level = self.top_level();
        spend.members.insert(
            self.custody_hash(),
            InnerPuzzleSpend::m_of_n(0, vec![], top_level.required, top_level.items),
        );

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

    fn sign_messages(
        &self,
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        messages: &[Bytes],
        selected: &[usize],
    ) -> Result<VaultMessageProof> {
        let envelope = vault_message_envelope(ctx, messages)?;
        let signed = Signed {
            delegated_puzzle_hash: ctx.tree_hash(envelope.puzzle).into(),
            vault_puzzle_hash: self.vault_puzzle_hash(launcher_id),
        };
        let spend = self.mips_spend(ctx, envelope, signed, selected)?;
        self.proof(ctx, launcher_id, &spend)
    }

    fn proof(
        &self,
        ctx: &mut SpendContext,
        launcher_id: Bytes32,
        spend: &MipsSpend,
    ) -> Result<VaultMessageProof> {
        let custody_spend = spend.spend(ctx, self.custody_hash())?;
        Ok(VaultMessageProof::from_custody_spend(
            ctx,
            launcher_id,
            custody_spend,
        )?)
    }

    fn verify(
        &self,
        proof: &VaultMessageProof,
        launcher_id: Bytes32,
    ) -> Result<Vec<Bytes>, DriverError> {
        verify_vault_signed_message(proof, launcher_id, self.custody_hash().into())
    }
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

fn launcher_id() -> Bytes32 {
    Bytes32::new([7; 32])
}

fn messages() -> Vec<Bytes> {
    vec![
        Bytes::new(b"Sign in to example.com\nNonce: 4f9a2c\nExpires: 2026-11-01".to_vec()),
        Bytes::new(vec![]),
        Bytes::new(vec![0xff, 0x00, 0x80]),
    ]
}

#[test]
fn test_vault_message_mode() {
    let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
        | MessageFlags::COIN.encode(MessageSide::Receiver);
    assert_eq!(mode, VAULT_MESSAGE_MODE);
}

#[test]
fn test_vault_message_envelope() -> Result<()> {
    let mut ctx = SpendContext::new();

    let messages = messages();
    let envelope = vault_message_envelope(&mut ctx, &messages)?;

    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let mut conditions = Conditions::new();
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
    assert_eq!(parse_vault_message_envelope(&ctx, envelope)?, messages);

    assert!(matches!(
        vault_message_envelope(&mut ctx, &[]),
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
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(kinds, threshold);
    let proof = custody.sign_messages(&mut ctx, launcher_id(), &messages(), selected)?;

    assert_eq!(custody.verify(&proof, launcher_id())?, messages());

    Ok(())
}

#[test]
fn test_vault_message_round_trip() -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let messages = messages();

    let envelope = vault_message_envelope(&mut ctx, &messages)?;

    // Clear signing returns the messages in order.
    let tx = parse_vault_transaction(Reveals::default(), &mut ctx, launcher_id(), envelope)?;
    assert_eq!(tx.signed_messages, messages);
    assert_eq!(tx.vault_child, None);
    assert!(tx.spends.is_empty());
    assert!(tx.drop_coins.is_empty());
    assert_eq!(tx.reserved_fee, 0);
    assert_eq!(
        tx.delegated_puzzle_hash,
        ctx.tree_hash(envelope.puzzle).into()
    );

    let proof = custody.sign_messages(&mut ctx, launcher_id(), &messages, &[0, 1])?;

    let bytes = proof.encode()?;
    assert_eq!(bytes[0], VAULT_MESSAGE_PROOF_VERSION);
    let decoded = VaultMessageProof::decode(&bytes)?;
    assert_eq!(decoded, proof);

    let string = proof.encode_string()?;
    assert!(string.starts_with("vaultsig1"));
    let decoded = VaultMessageProof::decode_string(&string)?;
    assert_eq!(decoded, proof);

    assert_eq!(custody.verify(&decoded, launcher_id())?, messages);

    Ok(())
}

#[test]
fn test_vault_message_proof_codec_errors() -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);
    let proof = custody.sign_messages(&mut ctx, launcher_id(), &messages(), &[0])?;

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
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let messages = messages();

    let envelope = vault_message_envelope(&mut ctx, &messages)?;
    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(envelope.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };

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

    let proof = custody.proof(&mut ctx, launcher_id(), &spend)?;
    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::Eval(_))
    ));

    Ok(())
}

#[rstest]
#[case::r1(Kind::R1)]
#[case::passkey(Kind::Passkey)]
fn test_reject_invalid_signature(#[case] kind: Kind) -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, kind, Kind::R1], 2);
    let messages = messages();

    let envelope = vault_message_envelope(&mut ctx, &messages)?;
    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(envelope.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };

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

    let proof = custody.proof(&mut ctx, launcher_id(), &spend)?;
    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::Eval(_))
    ));

    Ok(())
}

#[test]
fn test_reject_wrong_launcher_id() -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey], 1);
    let other_launcher_id = Bytes32::new([8; 32]);

    // The proof is for a different vault than the one expected.
    let proof = custody.sign_messages(&mut ctx, launcher_id(), &messages(), &[1])?;
    assert!(matches!(
        custody.verify(&proof, other_launcher_id),
        Err(DriverError::WrongVaultLauncherId)
    ));

    // The proof claims to be for the expected vault, but the signatures are for a vault with a different
    // launcher id and the same custody configuration.
    let proof = VaultMessageProof::new(
        other_launcher_id,
        proof.custody_puzzle_reveal.clone(),
        proof.custody_solution.clone(),
    );
    assert!(matches!(
        custody.verify(&proof, other_launcher_id),
        Err(DriverError::VaultPuzzleHashMismatch)
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
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);

    let signed_messages: Vec<Bytes> = signed_messages.into_iter().map(Bytes::new).collect();
    let revealed_messages: Vec<Bytes> = revealed_messages.into_iter().map(Bytes::new).collect();

    let signed_envelope = vault_message_envelope(&mut ctx, &signed_messages)?;
    let revealed_envelope = vault_message_envelope(&mut ctx, &revealed_messages)?;

    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(signed_envelope.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };

    let spend = custody.mips_spend(&mut ctx, revealed_envelope, signed, &[0, 1])?;
    let proof = custody.proof(&mut ctx, launcher_id(), &spend)?;

    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::Eval(_))
    ));

    Ok(())
}

#[test]
fn test_reject_mismatched_custody() -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey], 2);
    let other = TestCustody::with_seed(&[Kind::R1, Kind::Passkey], 2, 100);

    // A valid proof for a different custody configuration of the same vault.
    let proof = other.sign_messages(&mut ctx, launcher_id(), &messages(), &[0, 1])?;
    assert_eq!(other.verify(&proof, launcher_id())?, messages());
    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::WrongVaultCustodyHash)
    ));

    // The other custody configuration signs for this vault's puzzle hash.
    let envelope = vault_message_envelope(&mut ctx, &messages())?;
    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(envelope.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };
    let spend = other.mips_spend(&mut ctx, envelope, signed, &[0, 1])?;
    let proof = other.proof(&mut ctx, launcher_id(), &spend)?;
    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::WrongVaultCustodyHash)
    ));
    assert!(matches!(
        other.verify(&proof, launcher_id()),
        Err(DriverError::VaultPuzzleHashMismatch)
    ));

    Ok(())
}

#[test]
fn test_reject_custody_without_signature() -> Result<()> {
    let mut ctx = SpendContext::new();

    let messages = messages();
    let envelope = vault_message_envelope(&mut ctx, &messages)?;

    // A custody puzzle that just outputs whatever the solution says, which doesn't require any signature.
    let custody_puzzle = ctx.alloc(&1)?;
    let custody_hash: Bytes32 = ctx.tree_hash(custody_puzzle).into();
    let vault_puzzle_hash: Bytes32 =
        SingletonArgs::curry_tree_hash(launcher_id(), custody_hash.into()).into();

    let conditions = ctx.run(envelope.puzzle, envelope.solution)?;
    let conditions = ctx.alloc(&(
        chia_sdk_types::conditions::AssertMyPuzzleHash::new(vault_puzzle_hash),
        conditions,
    ))?;

    let proof = VaultMessageProof::from_custody_spend(
        &ctx,
        launcher_id(),
        Spend::new(custody_puzzle, conditions),
    )?;

    // It's rejected because it isn't the vault's custody puzzle.
    assert!(matches!(
        verify_vault_signed_message(&proof, launcher_id(), Bytes32::new([1; 32])),
        Err(DriverError::WrongVaultCustodyHash)
    ));

    // And even if a caller trusted it, it isn't a MIPS puzzle.
    assert!(matches!(
        verify_vault_signed_message(&proof, launcher_id(), custody_hash),
        Err(DriverError::InvalidVaultMessageCustody)
    ));

    Ok(())
}

#[rstest]
#[case::create_coin(Conditions::new().create_coin(Bytes32::new([1; 32]), 1, Memos::None))]
#[case::reserve_fee(Conditions::new().reserve_fee(1))]
#[case::assertion(Conditions::new().assert_seconds_absolute(100))]
#[case::remark(Conditions::new().remark(NodePtr::NIL))]
fn test_reject_extra_envelope_condition(#[case] extra: Conditions) -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);
    let messages = messages();

    let coin_id = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let mut conditions = Conditions::new();
    for message in &messages {
        conditions.push(SendMessage::new(
            VAULT_MESSAGE_MODE,
            message.clone(),
            vec![coin_id],
        ));
    }
    let delegated_spend = ctx.delegated_spend(conditions.extend(extra))?;

    assert!(matches!(
        parse_vault_message_envelope(&ctx, delegated_spend),
        Err(DriverError::InvalidVaultMessageEnvelope)
    ));
    assert!(matches!(
        parse_vault_transaction(Reveals::default(), &mut ctx, launcher_id(), delegated_spend),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(delegated_spend.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0, 1])?;
    let proof = custody.proof(&mut ctx, launcher_id(), &spend)?;

    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::InvalidVaultMessageEnvelope)
    ));

    Ok(())
}

#[test]
fn test_reject_message_to_real_coin() -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);

    let zero = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let real = ctx.alloc(&Bytes32::new([1; 32]))?;
    let delegated_spend = ctx.delegated_spend(
        Conditions::new()
            .with(SendMessage::new(
                VAULT_MESSAGE_MODE,
                Bytes::new(b"hello".to_vec()),
                vec![zero],
            ))
            .with(SendMessage::new(
                VAULT_MESSAGE_MODE,
                Bytes::new(b"world".to_vec()),
                vec![real],
            )),
    )?;

    assert!(matches!(
        parse_vault_message_envelope(&ctx, delegated_spend),
        Err(DriverError::InvalidVaultMessageEnvelope)
    ));
    assert!(matches!(
        parse_vault_transaction(Reveals::default(), &mut ctx, launcher_id(), delegated_spend),
        Err(DriverError::MixedVaultMessageEnvelope)
    ));

    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(delegated_spend.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    let proof = custody.proof(&mut ctx, launcher_id(), &spend)?;

    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::InvalidVaultMessageEnvelope)
    ));

    Ok(())
}

#[test]
fn test_reject_non_canonical_envelope() -> Result<()> {
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1], 1);

    // The message has an extra data item after the coin id, so it isn't a valid envelope message.
    let zero = ctx.alloc(&VAULT_MESSAGE_COIN_ID)?;
    let delegated_spend = ctx.delegated_spend(Conditions::new().with(SendMessage::new(
        VAULT_MESSAGE_MODE,
        Bytes::new(b"hello".to_vec()),
        vec![zero, NodePtr::NIL],
    )))?;

    assert!(matches!(
        parse_vault_message_envelope(&ctx, delegated_spend),
        Err(DriverError::InvalidVaultMessageEnvelope)
    ));

    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(delegated_spend.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(launcher_id()),
    };
    let spend = custody.mips_spend(&mut ctx, delegated_spend, signed, &[0])?;
    let proof = custody.proof(&mut ctx, launcher_id(), &spend)?;

    assert!(matches!(
        custody.verify(&proof, launcher_id()),
        Err(DriverError::InvalidVaultMessageEnvelope)
    ));

    Ok(())
}

#[test]
fn test_vault_message_envelope_spend_fails() -> Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody = TestCustody::new(&[Kind::R1, Kind::Passkey, Kind::R1], 2);

    let alice = sim.bls(1);
    let (mint_vault, vault) =
        Launcher::new(alice.coin.coin_id(), 1).mint_vault(&mut ctx, custody.custody_hash(), ())?;
    StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, mint_vault)?;
    sim.spend_coins(ctx.take(), &[alice.sk])?;

    let messages = messages();
    let envelope = vault_message_envelope(&mut ctx, &messages)?;
    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(envelope.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(vault.info.launcher_id),
    };
    let spend = custody.mips_spend(&mut ctx, envelope, signed, &[0, 1])?;

    // The proof of the same spend is valid.
    let proof = custody.proof(&mut ctx, vault.info.launcher_id, &spend)?;
    assert_eq!(custody.verify(&proof, vault.info.launcher_id)?, messages);

    // But the spend can't be valid on chain. Without recreating the vault, the singleton itself fails.
    vault.spend(&mut ctx, &spend)?;
    assert!(matches!(
        sim.spend_coins(ctx.take(), &[]),
        Err(SimulatorError::Signer(_))
    ));

    // And with the vault recreated, so that the singleton is valid, the zero coin id can't receive the messages.
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
    let envelope = ctx.delegated_spend(conditions)?;
    let signed = Signed {
        delegated_puzzle_hash: ctx.tree_hash(envelope.puzzle).into(),
        vault_puzzle_hash: custody.vault_puzzle_hash(vault.info.launcher_id),
    };
    let spend = custody.mips_spend(&mut ctx, envelope, signed, &[0, 1])?;
    vault.spend(&mut ctx, &spend)?;
    assert!(matches!(
        sim.spend_coins(ctx.take(), &[]),
        Err(SimulatorError::Validation(
            ErrorCode::MessageNotSentOrReceived
        ))
    ));

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
        parse_vault_transaction(Reveals::default(), &mut ctx, launcher_id(), delegated_spend),
        Err(DriverError::MissingSpend)
    ));

    // A delegated spend without any messages doesn't sign any messages.
    let delegated_spend = ctx.delegated_spend(Conditions::new().reserve_fee(1))?;
    let tx = parse_vault_transaction(Reveals::default(), &mut ctx, launcher_id(), delegated_spend)?;
    assert!(tx.signed_messages.is_empty());

    Ok(())
}
