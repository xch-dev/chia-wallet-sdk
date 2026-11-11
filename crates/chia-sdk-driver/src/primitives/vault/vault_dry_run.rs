use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{
    Mod,
    puzzles::{
        DelegatedPuzzleFeederSolution, K1Member, K1MemberPuzzleAssert,
        K1MemberPuzzleAssertSolution, K1MemberSolution, PasskeyMember, PasskeyMemberPuzzleAssert,
        PasskeyMemberPuzzleAssertSolution, PasskeyMemberSolution, R1Member, R1MemberPuzzleAssert,
        R1MemberPuzzleAssertSolution, R1MemberSolution,
    },
};
use chia_secp::{K1SecretKey, R1SecretKey};
use chia_sha2::Sha256;
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{
    DriverError, InnerPuzzleSpend, MipsSpend, MipsSpendKind, MofN, Puzzle, Spend, SpendContext,
    Vault, VaultInfo, calculate_vault_puzzle_message, mips_puzzle_hash,
};

/// The default authenticator data of a passkey member, used when the custody spend passed to
/// [`VaultDryRun::new`] doesn't include a passkey solution to copy.
const DEFAULT_AUTHENTICATOR_DATA: [u8; 37] = [0x49; 37];

/// The default origin of a passkey member's client data, used when the custody spend passed to
/// [`VaultDryRun::new`] doesn't include a passkey solution to copy.
const DEFAULT_ORIGIN: &str = "https://vault.example.com";

/// The cost to allow for each passkey member that signs, since a passkey member's cost depends on the value of its
/// challenge, which the stand-in can't reproduce. Over 3,000 random challenges, the cost of a [`PasskeyMember`] varied
/// by 16,818, so this is about twice that.
pub const PASSKEY_COST_MARGIN: u64 = 32_000;

/// A stand-in for a vault, for estimating the cost of a vault transaction before the vault's members sign it.
///
/// Members such as [`R1Member`] and [`PasskeyMember`] put their signature in their solution and check it in CLVM, so
/// an unsigned vault spend can't be run, and neither can the spends that assert it. The stand-in replaces each member
/// that signs with a member of the same type that has a freshly generated key, which it can sign with. Everything
/// else about the custody spend is kept: the members that aren't revealed, the restrictions and their spends, the
/// nonces, and the M of N thresholds. It also has the same launcher id, lineage proof, parent and amount as the vault.
///
/// Build the transaction against [`VaultDryRun::vault`] (including the custody hash that its p2 spends pass to the
/// singleton), then spend it with [`VaultDryRun::spend`]. The result costs the same as the real transaction, so it can
/// be passed to [`spend_bundle_cost`](crate::spend_bundle_cost) to work out the fee before anyone signs.
///
/// The cost is exact for [`K1Member`], [`R1Member`] and their puzzle assert variants, since their keys and
/// signatures are the same size whatever their value. BLS members don't sign in CLVM, so they're kept as they are.
///
/// Passkey members aren't exact, for two reasons. Their cost depends on the value of the challenge they sign, which
/// is different for the stand-in, so price the transaction with [`VaultDryRun::cost_margin`] added (see
/// [`FeeOptions::extra_cost`](crate::FeeOptions::extra_cost)). And their authenticator data and client data JSON vary
/// in length between authenticators, so the stand-in copies them from the passkey solution in the custody spend passed
/// to [`VaultDryRun::new`], which should be the same length as the real ones (for example, from a previous signature
/// by the same passkey).
///
/// Other members that check a signature in CLVM aren't supported, and the stand-in spend will fail to run.
#[derive(Debug, Clone)]
pub struct VaultDryRun {
    /// The stand-in vault, with the custody hash of the stand-in custody puzzle.
    pub vault: Vault,
    custody: MipsSpend,
    signers: Vec<DryRunSigner>,
}

#[derive(Debug, Clone)]
struct DryRunSigner {
    member_hash: TreeHash,
    puzzle: NodePtr,
    key: DryRunKey,
}

#[derive(Debug, Clone)]
enum DryRunKey {
    K1(K1SecretKey),
    K1PuzzleAssert(K1SecretKey),
    R1(R1SecretKey),
    R1PuzzleAssert(R1SecretKey),
    Passkey(R1SecretKey, PasskeyData),
    PasskeyPuzzleAssert(R1SecretKey, PasskeyData),
}

#[derive(Debug, Clone)]
struct PasskeyData {
    authenticator_data: Bytes,
    client_data_json: String,
    challenge_index: usize,
}

impl VaultDryRun {
    /// Creates a stand-in for `vault`, from the custody spend that its members will sign. The custody spend's
    /// [`delegated`](MipsSpend::delegated) spend is ignored, and its member solutions are only used to copy the
    /// authenticator data and client data of passkey members.
    pub fn new(
        ctx: &mut SpendContext,
        vault: &Vault,
        custody: &MipsSpend,
    ) -> Result<Self, DriverError> {
        let mut stand_in = MipsSpend::new(custody.delegated);
        stand_in.restrictions.clone_from(&custody.restrictions);

        let mut signers = Vec::new();
        let mut next_key = 0;

        let custody_hash = rekey(
            ctx,
            custody,
            &mut stand_in,
            &mut signers,
            &mut next_key,
            vault.info.custody_hash,
            true,
        )?;

        let puzzle_hash = SingletonArgs::curry_tree_hash(vault.info.launcher_id, custody_hash);

        let vault = Vault {
            coin: Coin::new(
                vault.coin.parent_coin_info,
                puzzle_hash.into(),
                vault.coin.amount,
            ),
            proof: vault.proof,
            info: VaultInfo::new(vault.info.launcher_id, custody_hash),
        };

        Ok(Self {
            vault,
            custody: stand_in,
            signers,
        })
    }

    /// The cost to add to the stand-in transaction's cost to cover the real one, which is
    /// [`PASSKEY_COST_MARGIN`] for each passkey member that signs, and zero otherwise.
    pub fn cost_margin(&self) -> u64 {
        let passkeys = self
            .signers
            .iter()
            .filter(|signer| {
                matches!(
                    signer.key,
                    DryRunKey::Passkey(..) | DryRunKey::PasskeyPuzzleAssert(..)
                )
            })
            .count() as u64;

        passkeys * PASSKEY_COST_MARGIN
    }

    /// Spends the stand-in vault with the delegated spend, signing with the stand-in keys.
    pub fn spend(&self, ctx: &mut SpendContext, delegated_spend: Spend) -> Result<(), DriverError> {
        let mut custody = self.custody.clone();
        custody.delegated = delegated_spend;

        // The members sign the delegated puzzle hash that the delegated puzzle feeder passes them, which includes any
        // delegated puzzle wrappers on the path. It doesn't depend on the signatures, so it's found by building the
        // custody spend with placeholder signatures first.
        for signer in &self.signers {
            let solution = signer.solution(ctx, &self.vault, Bytes32::default())?;
            set_member_spend(
                &mut custody,
                signer.member_hash,
                Spend::new(signer.puzzle, solution),
            )?;
        }

        let custody_spend = custody.spend(ctx, self.vault.info.custody_hash)?;
        let feeder_solution: DelegatedPuzzleFeederSolution<NodePtr, NodePtr, NodePtr> =
            ctx.extract(custody_spend.solution)?;
        let delegated_puzzle_hash = ctx.tree_hash(feeder_solution.delegated_puzzle).into();

        for signer in &self.signers {
            let solution = signer.solution(ctx, &self.vault, delegated_puzzle_hash)?;
            set_member_spend(
                &mut custody,
                signer.member_hash,
                Spend::new(signer.puzzle, solution),
            )?;
        }

        self.vault.spend(ctx, &custody)
    }
}

impl DryRunSigner {
    fn solution(
        &self,
        ctx: &mut SpendContext,
        vault: &Vault,
        delegated_puzzle_hash: Bytes32,
    ) -> Result<NodePtr, DriverError> {
        let coin_id = vault.coin.coin_id();
        let puzzle_hash = vault.coin.puzzle_hash;

        let coin_message = sha256(&[delegated_puzzle_hash.as_ref(), coin_id.as_ref()]);
        let puzzle_message = sha256(&[&calculate_vault_puzzle_message(
            delegated_puzzle_hash,
            puzzle_hash,
        )]);

        match &self.key {
            DryRunKey::K1(sk) => {
                ctx.alloc(&K1MemberSolution::new(coin_id, sign_k1(sk, &coin_message)?))
            }
            DryRunKey::K1PuzzleAssert(sk) => ctx.alloc(&K1MemberPuzzleAssertSolution::new(
                puzzle_hash,
                sign_k1(sk, &puzzle_message)?,
            )),
            DryRunKey::R1(sk) => {
                ctx.alloc(&R1MemberSolution::new(coin_id, sign_r1(sk, &coin_message)?))
            }
            DryRunKey::R1PuzzleAssert(sk) => ctx.alloc(&R1MemberPuzzleAssertSolution::new(
                puzzle_hash,
                sign_r1(sk, &puzzle_message)?,
            )),
            DryRunKey::Passkey(sk, data) => {
                let (client_data_json, signature) = data.sign(sk, &coin_message)?;
                ctx.alloc(&PasskeyMemberSolution {
                    authenticator_data: data.authenticator_data.clone(),
                    client_data_json,
                    challenge_index: data.challenge_index,
                    signature,
                    coin_id,
                })
            }
            DryRunKey::PasskeyPuzzleAssert(sk, data) => {
                let (client_data_json, signature) = data.sign(sk, &puzzle_message)?;
                ctx.alloc(&PasskeyMemberPuzzleAssertSolution {
                    authenticator_data: data.authenticator_data.clone(),
                    client_data_json,
                    challenge_index: data.challenge_index,
                    signature,
                    puzzle_hash,
                })
            }
        }
    }
}

impl PasskeyData {
    /// Copies the authenticator data and client data from a passkey solution, so that the stand-in is the same size.
    fn copy(authenticator_data: Bytes, client_data_json: &[u8], challenge_index: usize) -> Self {
        let default = Self::default_data();

        let Ok(client_data_json) = String::from_utf8(client_data_json.to_vec()) else {
            return default;
        };

        let data = Self {
            authenticator_data,
            client_data_json,
            challenge_index,
        };

        if data.challenge_range().is_some() {
            data
        } else {
            default
        }
    }

    fn default_data() -> Self {
        let challenge = base64_url(&[0; 32]);
        let client_data_json = format!(
            r#"{{"type":"webauthn.get","challenge":"{challenge}","origin":"{DEFAULT_ORIGIN}","crossOrigin":false}}"#
        );
        let challenge_index = client_data_json
            .find(r#""challenge":"#)
            .expect("the client data includes the challenge");

        Self {
            authenticator_data: Bytes::new(DEFAULT_AUTHENTICATOR_DATA.to_vec()),
            client_data_json,
            challenge_index,
        }
    }

    /// The range of the base64url challenge in the client data JSON, after `"challenge":"` at the challenge index.
    fn challenge_range(&self) -> Option<std::ops::Range<usize>> {
        const PREFIX: &str = r#""challenge":""#;
        let challenge_length = base64_url(&[0; 32]).len();

        let start = self.challenge_index + PREFIX.len();
        let end = start + challenge_length;

        (self.client_data_json.get(self.challenge_index..start) == Some(PREFIX)
            && self.client_data_json.is_char_boundary(end)
            && end <= self.client_data_json.len())
        .then_some(start..end)
    }

    fn sign(
        &self,
        sk: &R1SecretKey,
        message_hash: &[u8; 32],
    ) -> Result<(Bytes, chia_secp::R1Signature), DriverError> {
        let range = self
            .challenge_range()
            .ok_or(DriverError::DryRunSigningFailed)?;

        let mut client_data_json = self.client_data_json.clone();
        client_data_json.replace_range(range, &base64_url(message_hash));

        let client_data_hash = sha256(&[client_data_json.as_bytes()]);
        let signature = sign_r1(
            sk,
            &sha256(&[self.authenticator_data.as_ref(), &client_data_hash]),
        )?;

        Ok((client_data_json.into_bytes().into(), signature))
    }
}

/// Copies the node at `hash` (and the nodes below it that the custody spend reveals) into `stand_in`, with every member
/// that signs replaced by one with a new key, and returns its new hash.
fn rekey(
    ctx: &mut SpendContext,
    custody: &MipsSpend,
    stand_in: &mut MipsSpend,
    signers: &mut Vec<DryRunSigner>,
    next_key: &mut u32,
    hash: TreeHash,
    top_level: bool,
) -> Result<TreeHash, DriverError> {
    let node = custody
        .members
        .get(&hash)
        .ok_or(DriverError::MissingSubpathSpend)?
        .clone();

    let mut signer = None;

    let (kind, inner_puzzle_hash) = match node.kind {
        MipsSpendKind::Member(spend) => {
            let spend = if let Some((puzzle, key)) = rekey_member(ctx, spend, next_key)? {
                signer = Some((puzzle, key));
                Spend::new(puzzle, NodePtr::NIL)
            } else {
                spend
            };
            let inner_puzzle_hash = ctx.tree_hash(spend.puzzle);
            (MipsSpendKind::Member(spend), inner_puzzle_hash)
        }
        MipsSpendKind::MofN(m_of_n) => {
            let mut items = Vec::with_capacity(m_of_n.items.len());

            for item in m_of_n.items {
                // Items that aren't spent are only revealed by their hash, which costs the same whatever it is.
                items.push(if custody.members.contains_key(&item) {
                    rekey(ctx, custody, stand_in, signers, next_key, item, false)?
                } else {
                    item
                });
            }

            let m_of_n = MofN::new(m_of_n.required, items);
            let inner_puzzle_hash = m_of_n.inner_puzzle_hash();
            (MipsSpendKind::MofN(m_of_n), inner_puzzle_hash)
        }
    };

    let new_hash = mips_puzzle_hash(
        node.nonce,
        node.restrictions.clone(),
        inner_puzzle_hash,
        top_level,
    );

    if let Some((puzzle, key)) = signer {
        signers.push(DryRunSigner {
            member_hash: new_hash,
            puzzle,
            key,
        });
    }

    stand_in.members.insert(
        new_hash,
        InnerPuzzleSpend {
            nonce: node.nonce,
            restrictions: node.restrictions,
            kind,
        },
    );

    Ok(new_hash)
}

/// If the member signs in CLVM, returns a member of the same type with a new key, along with the key.
fn rekey_member(
    ctx: &mut SpendContext,
    spend: Spend,
    next_key: &mut u32,
) -> Result<Option<(NodePtr, DryRunKey)>, DriverError> {
    let mod_hash = Puzzle::parse(ctx, spend.puzzle).mod_hash();

    let result = if mod_hash == K1Member::mod_hash() {
        let sk = k1_key(next_key);
        let puzzle = ctx.curry(K1Member::new(sk.public_key()))?;
        (puzzle, DryRunKey::K1(sk))
    } else if mod_hash == K1MemberPuzzleAssert::mod_hash() {
        let sk = k1_key(next_key);
        let puzzle = ctx.curry(K1MemberPuzzleAssert::new(sk.public_key()))?;
        (puzzle, DryRunKey::K1PuzzleAssert(sk))
    } else if mod_hash == R1Member::mod_hash() {
        let sk = r1_key(next_key);
        let puzzle = ctx.curry(R1Member::new(sk.public_key()))?;
        (puzzle, DryRunKey::R1(sk))
    } else if mod_hash == R1MemberPuzzleAssert::mod_hash() {
        let sk = r1_key(next_key);
        let puzzle = ctx.curry(R1MemberPuzzleAssert::new(sk.public_key()))?;
        (puzzle, DryRunKey::R1PuzzleAssert(sk))
    } else if mod_hash == PasskeyMember::mod_hash() {
        let data = ctx
            .extract::<PasskeyMemberSolution>(spend.solution)
            .map_or_else(
                |_| PasskeyData::default_data(),
                |solution| {
                    PasskeyData::copy(
                        solution.authenticator_data,
                        &solution.client_data_json,
                        solution.challenge_index,
                    )
                },
            );
        let sk = r1_key(next_key);
        let puzzle = ctx.curry(PasskeyMember::new(sk.public_key()))?;
        (puzzle, DryRunKey::Passkey(sk, data))
    } else if mod_hash == PasskeyMemberPuzzleAssert::mod_hash() {
        let data = ctx
            .extract::<PasskeyMemberPuzzleAssertSolution>(spend.solution)
            .map_or_else(
                |_| PasskeyData::default_data(),
                |solution| {
                    PasskeyData::copy(
                        solution.authenticator_data,
                        &solution.client_data_json,
                        solution.challenge_index,
                    )
                },
            );
        let sk = r1_key(next_key);
        let puzzle = ctx.curry(PasskeyMemberPuzzleAssert::new(sk.public_key()))?;
        (puzzle, DryRunKey::PasskeyPuzzleAssert(sk, data))
    } else {
        return Ok(None);
    };

    Ok(Some(result))
}

fn set_member_spend(
    custody: &mut MipsSpend,
    member_hash: TreeHash,
    spend: Spend,
) -> Result<(), DriverError> {
    let member = custody
        .members
        .get_mut(&member_hash)
        .ok_or(DriverError::MissingSubpathSpend)?;
    member.kind = MipsSpendKind::Member(spend);
    Ok(())
}

/// Derives the next throwaway key. The keys don't need to be secret, since they only sign a transaction that's never
/// submitted, so they're derived from a counter.
fn key_bytes(next_key: &mut u32) -> [u8; 32] {
    let bytes = sha256(&[b"chia-wallet-sdk vault dry run", &next_key.to_be_bytes()]);
    *next_key += 1;
    bytes
}

fn k1_key(next_key: &mut u32) -> K1SecretKey {
    loop {
        if let Ok(sk) = K1SecretKey::from_bytes(&key_bytes(next_key)) {
            return sk;
        }
    }
}

fn r1_key(next_key: &mut u32) -> R1SecretKey {
    loop {
        if let Ok(sk) = R1SecretKey::from_bytes(&key_bytes(next_key)) {
            return sk;
        }
    }
}

fn sign_k1(
    sk: &K1SecretKey,
    message_hash: &[u8; 32],
) -> Result<chia_secp::K1Signature, DriverError> {
    sk.sign_prehashed(message_hash)
        .map_err(|_| DriverError::DryRunSigningFailed)
}

fn sign_r1(
    sk: &R1SecretKey,
    message_hash: &[u8; 32],
) -> Result<chia_secp::R1Signature, DriverError> {
    sk.sign_prehashed(message_hash)
        .map_err(|_| DriverError::DryRunSigningFailed)
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize()
}

/// Unpadded base64url, which is how `WebAuthn` encodes the challenge in the client data JSON.
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chia_bls::Signature;
    use chia_protocol::{CoinSpend, SpendBundle};
    use chia_puzzle_types::Memos;
    use chia_sdk_test::{K1Pair, R1Pair, Simulator};
    use chia_sdk_types::{
        Conditions, MAINNET_CONSTANTS, MessageFlags, MessageSide, conditions::SendMessage,
        puzzles::BlsMember,
    };
    use clvm_utils::ToTreeHash;
    use rstest::rstest;

    use crate::{
        Action, FeeOptions, FeeTransaction, Finished, Id, Launcher, P2Singleton, Relation,
        SpendKind, Spends, StandardLayer, minimum_fee, spend_bundle_conditions, spend_bundle_cost,
    };

    use super::*;

    /// The key of a member, for signing the real transaction.
    #[derive(Debug, Clone)]
    enum TestKey {
        K1(K1SecretKey),
        R1(R1SecretKey),
        Passkey(R1SecretKey),
        R1PuzzleAssert(R1SecretKey),
        Bls,
    }

    impl TestKey {
        fn member_puzzle(&self, ctx: &mut SpendContext) -> anyhow::Result<NodePtr> {
            Ok(match self {
                Self::K1(sk) => ctx.curry(K1Member::new(sk.public_key()))?,
                Self::R1(sk) => ctx.curry(R1Member::new(sk.public_key()))?,
                Self::Passkey(sk) => ctx.curry(PasskeyMember::new(sk.public_key()))?,
                Self::R1PuzzleAssert(sk) => {
                    ctx.curry(R1MemberPuzzleAssert::new(sk.public_key()))?
                }
                Self::Bls => ctx.curry(BlsMember::new(
                    chia_bls::SecretKey::from_seed(&[2; 32]).public_key(),
                ))?,
            })
        }

        /// The real solution, signed with the real key. Passkeys use the default client data, which the dry run copies
        /// from the placeholder solution, so they're the same length.
        fn solution(
            &self,
            ctx: &mut SpendContext,
            vault: &Vault,
            delegated_puzzle_hash: Bytes32,
        ) -> anyhow::Result<NodePtr> {
            let key = match self {
                Self::K1(sk) => DryRunKey::K1(sk.clone()),
                Self::R1(sk) => DryRunKey::R1(sk.clone()),
                Self::Passkey(sk) => DryRunKey::Passkey(sk.clone(), PasskeyData::default_data()),
                Self::R1PuzzleAssert(sk) => DryRunKey::R1PuzzleAssert(sk.clone()),
                Self::Bls => return Ok(NodePtr::NIL),
            };
            let signer = DryRunSigner {
                member_hash: TreeHash::new([0; 32]),
                puzzle: NodePtr::NIL,
                key,
            };
            Ok(signer.solution(ctx, vault, delegated_puzzle_hash)?)
        }
    }

    /// A vault whose custody is `required` of the keys (or the single key, if there's one).
    struct TestVault {
        vault: Vault,
        keys: Vec<TestKey>,
        required: usize,
        p2_puzzle_hash: Bytes32,
        coins: Vec<Coin>,
    }

    impl TestVault {
        fn member_hashes(&self, ctx: &mut SpendContext) -> anyhow::Result<Vec<TreeHash>> {
            let mut hashes = Vec::new();
            for key in &self.keys {
                let puzzle = key.member_puzzle(ctx)?;
                let top_level = self.keys.len() == 1;
                hashes.push(mips_puzzle_hash(
                    0,
                    vec![],
                    ctx.tree_hash(puzzle),
                    top_level,
                ));
            }
            Ok(hashes)
        }

        fn custody_hash(&self, ctx: &mut SpendContext) -> anyhow::Result<TreeHash> {
            let hashes = self.member_hashes(ctx)?;
            if self.keys.len() == 1 {
                return Ok(hashes[0]);
            }
            let m_of_n = MofN::new(self.required, hashes);
            Ok(mips_puzzle_hash(
                0,
                vec![],
                m_of_n.inner_puzzle_hash(),
                true,
            ))
        }

        fn mint(
            sim: &mut Simulator,
            ctx: &mut SpendContext,
            keys: Vec<TestKey>,
            required: usize,
            amounts: &[u64],
        ) -> anyhow::Result<Self> {
            let mut test_vault = Self {
                vault: Vault {
                    coin: Coin::new(Bytes32::default(), Bytes32::default(), 1),
                    proof: chia_puzzle_types::Proof::Eve(chia_puzzle_types::EveProof {
                        parent_parent_coin_info: Bytes32::default(),
                        parent_amount: 1,
                    }),
                    info: VaultInfo::new(Bytes32::default(), TreeHash::new([0; 32])),
                },
                keys,
                required,
                p2_puzzle_hash: Bytes32::default(),
                coins: Vec::new(),
            };

            let custody_hash = test_vault.custody_hash(ctx)?;
            let total: u64 = amounts.iter().sum();
            let alice = sim.bls(total + 1);

            let (mut conditions, vault) =
                Launcher::new(alice.coin.coin_id(), 1).mint_vault(ctx, custody_hash, ())?;

            let p2_puzzle_hash: Bytes32 = P2Singleton::new(vault.info.launcher_id, 0)
                .tree_hash()
                .into();

            for &amount in amounts {
                conditions = conditions.create_coin(p2_puzzle_hash, amount, Memos::None);
            }

            StandardLayer::new(alice.pk).spend(ctx, alice.coin, conditions)?;
            sim.spend_coins(ctx.take(), &[alice.sk])?;

            test_vault.coins = amounts
                .iter()
                .map(|&amount| Coin::new(alice.coin.coin_id(), p2_puzzle_hash, amount))
                .collect();
            test_vault.vault = vault;
            test_vault.p2_puzzle_hash = p2_puzzle_hash;

            Ok(test_vault)
        }

        /// The custody spend, with the members that sign (the first `required` of them) and placeholder solutions.
        fn custody_template(&self, ctx: &mut SpendContext) -> anyhow::Result<MipsSpend> {
            let mut custody = MipsSpend::new(Spend::new(NodePtr::NIL, NodePtr::NIL));
            let hashes = self.member_hashes(ctx)?;

            for (key, hash) in self.keys.iter().zip(&hashes).take(self.required) {
                let puzzle = key.member_puzzle(ctx)?;
                let solution = match key {
                    TestKey::Passkey(_) => {
                        let data = PasskeyData::default_data();
                        ctx.alloc(&PasskeyMemberSolution {
                            authenticator_data: data.authenticator_data,
                            client_data_json: data.client_data_json.into_bytes().into(),
                            challenge_index: data.challenge_index,
                            signature: chia_secp::R1Signature::from_bytes(&[1; 64])?,
                            coin_id: Bytes32::default(),
                        })?
                    }
                    _ => NodePtr::NIL,
                };
                custody.members.insert(
                    *hash,
                    InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, solution)),
                );
            }

            if self.keys.len() > 1 {
                let custody_hash = self.custody_hash(ctx)?;
                custody.members.insert(
                    custody_hash,
                    InnerPuzzleSpend::m_of_n(0, vec![], self.required, hashes),
                );
            }

            Ok(custody)
        }

        /// Spends the real vault, signing with the real keys.
        fn spend_vault(&self, ctx: &mut SpendContext, delegated: Spend) -> anyhow::Result<()> {
            let mut custody = self.custody_template(ctx)?;
            custody.delegated = delegated;
            let delegated_puzzle_hash = ctx.tree_hash(delegated.puzzle).into();
            let hashes = self.member_hashes(ctx)?;

            for (key, hash) in self.keys.iter().zip(&hashes).take(self.required) {
                let puzzle = key.member_puzzle(ctx)?;
                let solution = key.solution(ctx, &self.vault, delegated_puzzle_hash)?;
                set_member_spend(&mut custody, *hash, Spend::new(puzzle, solution))?;
            }

            self.vault.spend(ctx, &custody)?;
            Ok(())
        }
    }

    /// Spends the p2 coins of `vault` and creates the vault's delegated spend, which is passed to `spend_vault`.
    fn spend_p2(
        ctx: &mut SpendContext,
        spends: Spends<Finished>,
        vault: &Vault,
        spend_vault: impl FnOnce(&mut SpendContext, Spend) -> Result<(), DriverError>,
    ) -> Result<(), DriverError> {
        let mut vault_conditions = Conditions::new();
        let mut coin_spends = HashMap::new();

        for (asset, kind) in spends.unspent() {
            let SpendKind::Conditions(spend) = kind else {
                unreachable!("there are no settlement spends");
            };

            let delegated_spend = ctx.delegated_spend(spend.finish())?;
            let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
                | MessageFlags::COIN.encode(MessageSide::Receiver);
            let coin_id = ctx.alloc(&asset.coin().coin_id())?;

            vault_conditions.push(SendMessage::new(
                mode,
                ctx.tree_hash(delegated_spend.puzzle).to_vec().into(),
                vec![coin_id],
            ));

            let p2_spend = P2Singleton::new(vault.info.launcher_id, 0).spend(
                ctx,
                vault.info.custody_hash.into(),
                vault.coin.amount,
                delegated_spend,
            )?;

            coin_spends.insert(asset.coin().coin_id(), p2_spend);
        }

        spends.spend(ctx, coin_spends)?;

        let delegated_spend = ctx.delegated_spend(vault_conditions.create_coin(
            vault.info.custody_hash.into(),
            vault.coin.amount,
            Memos::None,
        ))?;

        spend_vault(ctx, delegated_spend)
    }

    fn actions() -> [Action; 1] {
        [Action::send(
            Id::Xch,
            Bytes32::new([7; 32]),
            1_000,
            Memos::None,
        )]
    }

    /// Estimates the fee with the stand-in vault.
    fn estimate(
        ctx: &mut SpendContext,
        test_vault: &TestVault,
        dry_run: &VaultDryRun,
    ) -> anyhow::Result<FeeTransaction<()>> {
        let mut options = FeeOptions::new(&MAINNET_CONSTANTS);
        options.extra_cost = dry_run.cost_margin();

        Ok(
            Spends::new(test_vault.p2_puzzle_hash).apply_with_minimum_fee(
                ctx,
                &actions(),
                &test_vault.coins,
                Relation::None,
                options,
                |ctx, spends| {
                    spend_p2(ctx, spends, &dry_run.vault, |ctx, delegated| {
                        dry_run.spend(ctx, delegated)
                    })
                },
            )?,
        )
    }

    /// Builds the real transaction with the estimated fee, signed with the real keys.
    fn build_real(
        ctx: &mut SpendContext,
        test_vault: &TestVault,
        fee: u64,
    ) -> anyhow::Result<Vec<CoinSpend>> {
        Spends::new(test_vault.p2_puzzle_hash).apply_with_fee(
            ctx,
            &actions(),
            &test_vault.coins,
            fee,
            Relation::None,
            |ctx, spends| {
                spend_p2(ctx, spends, &test_vault.vault, |ctx, delegated| {
                    test_vault
                        .spend_vault(ctx, delegated)
                        .map_err(|error| DriverError::Custom(error.to_string()))
                })
            },
        )?;
        Ok(ctx.take())
    }

    fn keys(kind: &str, count: usize) -> Vec<TestKey> {
        (0..count)
            .map(|i| match kind {
                "k1" => TestKey::K1(K1Pair::range_vec(count)[i].sk.clone()),
                "r1" => TestKey::R1(R1Pair::range_vec(count)[i].sk.clone()),
                "passkey" => TestKey::Passkey(R1Pair::range_vec(count)[i].sk.clone()),
                "r1_puzzle_assert" => {
                    TestKey::R1PuzzleAssert(R1Pair::range_vec(count)[i].sk.clone())
                }
                "bls" => TestKey::Bls,
                _ => unreachable!(),
            })
            .collect()
    }

    /// The dry run costs exactly the same as the real transaction, so it finds the same fee before anyone signs. The
    /// fee pays for the coin it comes from, and the mempool sees a fee that clears its minimum.
    #[rstest]
    #[case::k1("k1", 1, 1)]
    #[case::r1("r1", 1, 1)]
    #[case::passkey("passkey", 1, 1)]
    #[case::r1_puzzle_assert("r1_puzzle_assert", 1, 1)]
    #[case::bls("bls", 1, 1)]
    #[case::r1_2_of_3("r1", 2, 3)]
    #[case::passkey_1_of_2("passkey", 1, 2)]
    fn test_dry_run_matches_real_cost(
        #[case] kind: &str,
        #[case] required: usize,
        #[case] count: usize,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        // The first coin covers the payment but not the fee, so the fee has to come from the second coin.
        let test_vault = TestVault::mint(
            &mut sim,
            &mut ctx,
            keys(kind, count),
            required,
            &[1_000, 1_000_000_000_000],
        )?;

        let template = test_vault.custody_template(&mut ctx)?;
        let dry_run = VaultDryRun::new(&mut ctx, &test_vault.vault, &template)?;

        // BLS members don't sign in CLVM, so a BLS vault is its own stand-in.
        if kind == "bls" {
            assert_eq!(dry_run.vault, test_vault.vault);
        } else {
            assert_ne!(
                dry_run.vault.info.custody_hash,
                test_vault.vault.info.custody_hash
            );
        }

        let estimate = estimate(&mut ctx, &test_vault, &dry_run)?;
        let real = build_real(&mut ctx, &test_vault, estimate.fee)?;
        let real_cost = spend_bundle_cost(&real, &MAINNET_CONSTANTS, None)?;

        if kind == "passkey" {
            assert!(estimate.cost.abs_diff(real_cost) <= dry_run.cost_margin());
        } else {
            assert_eq!(estimate.cost, real_cost);
        }
        assert!(estimate.fee >= minimum_fee(real_cost));

        let fee_coin = test_vault.coins[1].coin_id();
        assert!(real.iter().any(|cs| cs.coin.coin_id() == fee_coin));

        for height in [
            MAINNET_CONSTANTS.hard_fork2_height - 1,
            MAINNET_CONSTANTS.hard_fork2_height,
        ] {
            let conds = spend_bundle_conditions(&real, &MAINNET_CONSTANTS, height)?;
            let mempool_fee = u64::try_from(conds.removal_amount - conds.addition_amount)?;
            assert_eq!(mempool_fee, estimate.fee);
            assert!(mempool_fee >= conds.cost * 5);
        }

        // BLS members sign with AGG_SIG_ME, which the simulator checks, so the BLS case isn't submitted.
        if kind != "bls" {
            sim.new_transaction(SpendBundle::new(real, Signature::default()))?;
        }

        Ok(())
    }
}
