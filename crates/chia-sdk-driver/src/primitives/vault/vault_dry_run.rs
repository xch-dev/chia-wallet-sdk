use std::collections::{HashMap, HashSet};

use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{
    Mod,
    puzzles::{
        K1Member, K1MemberPuzzleAssert, K1MemberPuzzleAssertSolution, K1MemberSolution,
        PasskeyMember, PasskeyMemberPuzzleAssert, PasskeyMemberPuzzleAssertSolution,
        PasskeyMemberSolution, R1Member, R1MemberPuzzleAssert, R1MemberPuzzleAssertSolution,
        R1MemberSolution, SingletonMember, SingletonMemberSolution, SingletonMemberWithMode,
        SingletonMemberWithModeSolution,
    },
};
use chia_secp::{K1SecretKey, R1SecretKey};
use chia_sha2::Sha256;
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{
    CurriedPuzzle, DriverError, InnerPuzzleSpend, MipsSpend, MipsSpendKind, MofN, Puzzle, Spend,
    SpendContext, Vault, VaultInfo, calculate_vault_puzzle_message, mips_puzzle_hash,
    wrap_delegated_puzzle_hash,
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
/// A vault controlled by other vaults (through a [`SingletonMember`] or [`SingletonMemberWithMode`]) is priced with
/// [`VaultDryRun::with_controllers`], which points its singleton members at the controllers' stand-ins.
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

/// Derives the throwaway keys of a stand-in. The keys don't need to be secret, since they only sign a transaction that's
/// never submitted. They're derived from the vault's launcher id, so that the stand-ins of different vaults in the
/// same transaction have different keys, as the real vaults do. Otherwise, after the 3.0 hard fork, the generator would
/// charge for identical member puzzles only once, and the stand-in would cost less than the real transaction.
struct KeyGenerator {
    launcher_id: Bytes32,
    next: u32,
}

/// What a member of the custody spend becomes in the stand-in.
enum MemberRewrite {
    /// The member signs in CLVM, so it's replaced with a member of the same type with a new key.
    Rekeyed(NodePtr, DryRunKey),
    /// The member is a singleton member of a controller, so its solution points at the controller's stand-in.
    Controlled(Spend),
    /// The member is kept as it is.
    Unchanged,
}

impl VaultDryRun {
    /// Creates a stand-in for `vault`, from the custody spend that its members will sign. The custody spend's
    /// [`delegated`](MipsSpend::delegated) spend is ignored, and its member solutions are only used to copy the
    /// authenticator data and client data of passkey members.
    ///
    /// Only the members that the custody spend reveals are replaced. In an M of N, that's the first `required` members
    /// that are in the custody spend, as [`MipsSpend::spend`] selects them.
    pub fn new(
        ctx: &mut SpendContext,
        vault: &Vault,
        custody: &MipsSpend,
    ) -> Result<Self, DriverError> {
        Self::with_controllers(ctx, vault, custody, &[])
    }

    /// Creates a stand-in for `vault`, which is controlled by the vaults that `controllers` stand in for. Each
    /// [`SingletonMember`] or [`SingletonMemberWithMode`] of a controller's launcher id is given the controller's
    /// stand-in inner puzzle hash and amount, so that it accepts the message from the controller's stand-in. Create the
    /// controllers' stand-ins first, and build their authorizations against this stand-in's coin.
    ///
    /// Returns [`DriverError::DuplicateDryRunController`] if two controllers have the same launcher id, and
    /// [`DriverError::DryRunControllerIsVault`] if a controller has the vault's own launcher id.
    pub fn with_controllers(
        ctx: &mut SpendContext,
        vault: &Vault,
        custody: &MipsSpend,
        controllers: &[VaultDryRun],
    ) -> Result<Self, DriverError> {
        let mut controller_ids = HashSet::with_capacity(controllers.len());

        for controller in controllers {
            let launcher_id = controller.vault.info.launcher_id;

            if launcher_id == vault.info.launcher_id {
                return Err(DriverError::DryRunControllerIsVault);
            }

            if !controller_ids.insert(launcher_id) {
                return Err(DriverError::DuplicateDryRunController(launcher_id));
            }
        }

        let mut stand_in = StandIn {
            custody: MipsSpend::new(custody.delegated),
            signers: Vec::new(),
            keys: KeyGenerator {
                launcher_id: vault.info.launcher_id,
                next: 0,
            },
            rekeyed: HashMap::new(),
        };
        stand_in
            .custody
            .restrictions
            .clone_from(&custody.restrictions);

        let custody_hash = rekey(
            ctx,
            &RekeyContext {
                custody,
                controllers,
            },
            &mut stand_in,
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
            custody: stand_in.custody,
            signers: stand_in.signers,
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

        let delegated_puzzle_hash = self.signed_delegated_puzzle_hash(ctx, &custody)?;

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

    /// The delegated puzzle hash that the members sign, which is the hash of the delegated puzzle wrapped in each of the
    /// delegated puzzle wrappers on the spent path, as [`InnerPuzzleSpend::spend`] wraps it.
    fn signed_delegated_puzzle_hash(
        &self,
        ctx: &SpendContext,
        custody: &MipsSpend,
    ) -> Result<Bytes32, DriverError> {
        let mut wrappers = Vec::new();

        custody
            .members
            .get(&self.vault.info.custody_hash)
            .ok_or(DriverError::MissingSubpathSpend)?
            .collect_delegated_puzzle_wrappers(custody, &mut wrappers)?;

        Ok(wrap_delegated_puzzle_hash(ctx.tree_hash(custody.delegated.puzzle), &wrappers).into())
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
        let range = self.challenge_range().ok_or_else(|| {
            DriverError::DryRunSigningFailed(
                "the passkey client data doesn't contain a challenge".to_string(),
            )
        })?;

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

struct RekeyContext<'a> {
    custody: &'a MipsSpend,
    controllers: &'a [VaultDryRun],
}

/// What a stand-in is built up in: its custody spend, its signers, the keys it's given, and the new hash of each node
/// that's already been copied.
struct StandIn {
    custody: MipsSpend,
    signers: Vec<DryRunSigner>,
    keys: KeyGenerator,
    rekeyed: HashMap<TreeHash, TreeHash>,
}

/// Copies the node at `hash` (and the nodes below it that the custody spend reveals) into `stand_in`, with every member
/// that signs replaced by one with a new key, and returns its new hash.
///
/// A node that's revealed more than once, such as a member selected in two M of N branches, is only copied once, so that
/// the stand-in reveals the same puzzle each time, as the real spend does.
fn rekey(
    ctx: &mut SpendContext,
    source: &RekeyContext<'_>,
    stand_in: &mut StandIn,
    hash: TreeHash,
    top_level: bool,
) -> Result<TreeHash, DriverError> {
    if let Some(&new_hash) = stand_in.rekeyed.get(&hash) {
        return Ok(new_hash);
    }

    let node = source
        .custody
        .members
        .get(&hash)
        .ok_or(DriverError::MissingSubpathSpend)?
        .clone();

    let mut signer = None;

    let (kind, inner_puzzle_hash) = match node.kind {
        MipsSpendKind::Member(spend) => {
            let spend = match rewrite_member(ctx, spend, &mut stand_in.keys, source.controllers)? {
                MemberRewrite::Rekeyed(puzzle, key) => {
                    signer = Some((puzzle, key));
                    Spend::new(puzzle, NodePtr::NIL)
                }
                MemberRewrite::Controlled(spend) => spend,
                MemberRewrite::Unchanged => spend,
            };
            let inner_puzzle_hash = ctx.tree_hash(spend.puzzle);
            (MipsSpendKind::Member(spend), inner_puzzle_hash)
        }
        MipsSpendKind::MofN(m_of_n) => {
            // Only the selected items are revealed. The others are only revealed by their hash, which costs the same
            // whatever it is, and they're left out of the stand-in so that it selects the same items.
            let selected = m_of_n.selected_items(source.custody)?;
            let mut items = m_of_n.items.clone();

            for index in selected {
                items[index] = rekey(ctx, source, stand_in, m_of_n.items[index], false)?;
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
        stand_in.signers.push(DryRunSigner {
            member_hash: new_hash,
            puzzle,
            key,
        });
    }

    stand_in.rekeyed.insert(hash, new_hash);
    stand_in.custody.members.insert(
        new_hash,
        InnerPuzzleSpend {
            nonce: node.nonce,
            restrictions: node.restrictions,
            kind,
        },
    );

    Ok(new_hash)
}

/// Works out what a member of the custody spend becomes in the stand-in.
fn rewrite_member(
    ctx: &mut SpendContext,
    spend: Spend,
    keys: &mut KeyGenerator,
    controllers: &[VaultDryRun],
) -> Result<MemberRewrite, DriverError> {
    let mod_hash = Puzzle::parse(ctx, spend.puzzle).mod_hash();

    if mod_hash == SingletonMember::mod_hash() || mod_hash == SingletonMemberWithMode::mod_hash() {
        return controlled_member(ctx, spend, mod_hash, controllers);
    }

    let result = if mod_hash == K1Member::mod_hash() {
        let sk = keys.k1();
        let puzzle = ctx.curry(K1Member::new(sk.public_key()))?;
        (puzzle, DryRunKey::K1(sk))
    } else if mod_hash == K1MemberPuzzleAssert::mod_hash() {
        let sk = keys.k1();
        let puzzle = ctx.curry(K1MemberPuzzleAssert::new(sk.public_key()))?;
        (puzzle, DryRunKey::K1PuzzleAssert(sk))
    } else if mod_hash == R1Member::mod_hash() {
        let sk = keys.r1();
        let puzzle = ctx.curry(R1Member::new(sk.public_key()))?;
        (puzzle, DryRunKey::R1(sk))
    } else if mod_hash == R1MemberPuzzleAssert::mod_hash() {
        let sk = keys.r1();
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
        let sk = keys.r1();
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
        let sk = keys.r1();
        let puzzle = ctx.curry(PasskeyMemberPuzzleAssert::new(sk.public_key()))?;
        (puzzle, DryRunKey::PasskeyPuzzleAssert(sk, data))
    } else {
        return Ok(MemberRewrite::Unchanged);
    };

    Ok(MemberRewrite::Rekeyed(result.0, result.1))
}

/// If the singleton member is a controller's, gives it the controller's stand-in inner puzzle hash and amount. The
/// puzzle is unchanged, since it only commits to the controller's launcher id.
fn controlled_member(
    ctx: &mut SpendContext,
    spend: Spend,
    mod_hash: TreeHash,
    controllers: &[VaultDryRun],
) -> Result<MemberRewrite, DriverError> {
    let Some(curried) = CurriedPuzzle::parse(ctx, spend.puzzle) else {
        return Ok(MemberRewrite::Unchanged);
    };

    let launcher_id = if mod_hash == SingletonMember::mod_hash() {
        ctx.extract::<SingletonMember>(curried.args)?
            .singleton_struct
            .launcher_id
    } else {
        ctx.extract::<SingletonMemberWithMode>(curried.args)?
            .singleton_struct
            .launcher_id
    };

    let Some(controller) = controllers
        .iter()
        .find(|controller| controller.vault.info.launcher_id == launcher_id)
    else {
        return Ok(MemberRewrite::Unchanged);
    };

    let inner_puzzle_hash = controller.vault.info.custody_hash.into();
    let amount = controller.vault.coin.amount;

    let solution = if mod_hash == SingletonMember::mod_hash() {
        ctx.alloc(&SingletonMemberSolution::new(inner_puzzle_hash, amount))?
    } else {
        ctx.alloc(&SingletonMemberWithModeSolution::new(
            inner_puzzle_hash,
            amount,
        ))?
    };

    Ok(MemberRewrite::Controlled(Spend::new(
        spend.puzzle,
        solution,
    )))
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

impl KeyGenerator {
    fn next_bytes(&mut self) -> [u8; 32] {
        let bytes = sha256(&[
            b"chia-wallet-sdk vault dry run",
            self.launcher_id.as_ref(),
            &self.next.to_be_bytes(),
        ]);
        self.next += 1;
        bytes
    }

    fn k1(&mut self) -> K1SecretKey {
        loop {
            if let Ok(sk) = K1SecretKey::from_bytes(&self.next_bytes()) {
                return sk;
            }
        }
    }

    fn r1(&mut self) -> R1SecretKey {
        loop {
            if let Ok(sk) = R1SecretKey::from_bytes(&self.next_bytes()) {
                return sk;
            }
        }
    }
}

fn sign_k1(
    sk: &K1SecretKey,
    message_hash: &[u8; 32],
) -> Result<chia_secp::K1Signature, DriverError> {
    sk.sign_prehashed(message_hash)
        .map_err(|error| DriverError::DryRunSigningFailed(error.to_string()))
}

fn sign_r1(
    sk: &R1SecretKey,
    message_hash: &[u8; 32],
) -> Result<chia_secp::R1Signature, DriverError> {
    sk.sign_prehashed(message_hash)
        .map_err(|error| DriverError::DryRunSigningFailed(error.to_string()))
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
    use chia_puzzles::PREVENT_MULTIPLE_CREATE_COINS_HASH;
    use chia_sdk_test::{K1Pair, R1Pair, Simulator};
    use chia_sdk_types::{
        Conditions, MAINNET_CONSTANTS, MessageFlags, MessageSide,
        conditions::SendMessage,
        puzzles::{AddDelegatedPuzzleWrapper, BlsMember, PreventMultipleCreateCoinsMod},
    };
    use clvm_utils::ToTreeHash;
    use rstest::rstest;

    use crate::{
        Action, FeeOptions, FeeTransaction, Finished, Id, Launcher, P2Singleton, Relation,
        Restriction, RestrictionKind, SpendKind, Spends, StandardLayer, minimum_fee,
        spend_bundle_conditions, spend_bundle_cost,
        test_wallet::{controlled_custody_puzzle_hash, controller_authorization},
    };

    use super::*;

    /// A mainnet height just before the 3.0 hard fork.
    fn pre_fork() -> u32 {
        MAINNET_CONSTANTS.hard_fork2_height - 1
    }

    /// The mainnet height of the 3.0 hard fork.
    fn post_fork() -> u32 {
        MAINNET_CONSTANTS.hard_fork2_height
    }

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

        /// A placeholder solution for the custody template. Passkeys get the default client data, which the stand-in
        /// copies, and which the real solution also uses, so they're the same length.
        fn placeholder(&self, ctx: &mut SpendContext) -> anyhow::Result<NodePtr> {
            Ok(match self {
                Self::Passkey(_) => {
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
            })
        }

        /// The real solution, signed with the real key.
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

    /// How a test vault's custody is made up.
    #[derive(Debug, Clone)]
    enum Custody {
        /// `required` of the keys, or the single key if there's one. The custody template includes every key when
        /// `template_includes_all` is set, even though only the first `required` are selected. With `wrapper`, the
        /// custody puzzle has a delegated puzzle wrapper that prevents creating more than one coin.
        Keys {
            keys: Vec<TestKey>,
            required: usize,
            template_includes_all: bool,
            wrapper: bool,
        },
        /// A singleton member of the controller with this launcher id.
        Controlled(Bytes32),
    }

    impl Custody {
        fn keys(keys: Vec<TestKey>, required: usize) -> Self {
            Self::Keys {
                keys,
                required,
                template_includes_all: false,
                wrapper: false,
            }
        }
    }

    struct TestVault {
        vault: Vault,
        custody: Custody,
        p2_puzzle_hash: Bytes32,
        coins: Vec<Coin>,
    }

    fn wrapper_restrictions(wrapper: bool) -> Vec<Restriction> {
        if wrapper {
            vec![Restriction {
                kind: RestrictionKind::DelegatedPuzzleWrapper,
                puzzle_hash: PREVENT_MULTIPLE_CREATE_COINS_HASH.into(),
            }]
        } else {
            vec![]
        }
    }

    impl Custody {
        /// The MIPS puzzle hash of each key's member, and the custody hash.
        fn hashes(&self, ctx: &mut SpendContext) -> anyhow::Result<(Vec<TreeHash>, TreeHash)> {
            match self {
                Self::Keys {
                    keys,
                    required,
                    wrapper,
                    ..
                } => {
                    let single = keys.len() == 1;
                    let mut hashes = Vec::new();
                    for key in keys {
                        let puzzle = key.member_puzzle(ctx)?;
                        let restrictions = if single {
                            wrapper_restrictions(*wrapper)
                        } else {
                            vec![]
                        };
                        hashes.push(mips_puzzle_hash(
                            0,
                            restrictions,
                            ctx.tree_hash(puzzle),
                            single,
                        ));
                    }
                    if single {
                        return Ok((hashes.clone(), hashes[0]));
                    }
                    let m_of_n = MofN::new(*required, hashes.clone());
                    let custody_hash = mips_puzzle_hash(
                        0,
                        wrapper_restrictions(*wrapper),
                        m_of_n.inner_puzzle_hash(),
                        true,
                    );
                    Ok((hashes, custody_hash))
                }
                Self::Controlled(launcher_id) => {
                    Ok((vec![], controlled_custody_puzzle_hash(*launcher_id, false)))
                }
            }
        }

        /// The custody spend, with placeholder solutions, for the vault to stand in for. For a controlled vault, this
        /// is the real singleton member spend, pointing at the real controller.
        fn template(
            &self,
            ctx: &mut SpendContext,
            controller: Option<&Vault>,
        ) -> anyhow::Result<MipsSpend> {
            let mut custody = MipsSpend::new(Spend::new(NodePtr::NIL, NodePtr::NIL));
            let (hashes, custody_hash) = self.hashes(ctx)?;

            match self {
                Self::Keys {
                    keys,
                    required,
                    template_includes_all,
                    wrapper,
                } => {
                    let count = if *template_includes_all {
                        keys.len()
                    } else {
                        *required
                    };
                    let single = keys.len() == 1;

                    for (key, hash) in keys.iter().zip(&hashes).take(count) {
                        let puzzle = key.member_puzzle(ctx)?;
                        let solution = key.placeholder(ctx)?;
                        let restrictions = if single {
                            wrapper_restrictions(*wrapper)
                        } else {
                            vec![]
                        };
                        custody.members.insert(
                            *hash,
                            InnerPuzzleSpend::new(0, restrictions, Spend::new(puzzle, solution)),
                        );
                    }

                    if !single {
                        custody.members.insert(
                            custody_hash,
                            InnerPuzzleSpend::m_of_n(
                                0,
                                wrapper_restrictions(*wrapper),
                                *required,
                                hashes,
                            ),
                        );
                    }

                    if *wrapper {
                        let puzzle = ctx.alloc_mod::<PreventMultipleCreateCoinsMod>()?;
                        custody.restrictions.insert(
                            PREVENT_MULTIPLE_CREATE_COINS_HASH.into(),
                            Spend::new(puzzle, NodePtr::NIL),
                        );
                    }
                }
                Self::Controlled(_) => {
                    let controller = controller.expect("a controlled vault needs its controller");
                    let spend =
                        crate::test_wallet::controlled_member_spend(ctx, controller, false)?;
                    custody
                        .members
                        .insert(custody_hash, InnerPuzzleSpend::new(0, vec![], spend));
                }
            }

            Ok(custody)
        }

        /// Spends the real vault, signing with the real keys.
        fn spend_vault(
            &self,
            ctx: &mut SpendContext,
            vault: &Vault,
            controller: Option<&Vault>,
            delegated: Spend,
        ) -> anyhow::Result<()> {
            let mut custody = self.template(ctx, controller)?;
            custody.delegated = delegated;

            if let Self::Keys {
                keys,
                required,
                wrapper,
                ..
            } = self
            {
                let mut delegated_puzzle_hash = ctx.tree_hash(delegated.puzzle);
                if *wrapper {
                    delegated_puzzle_hash = AddDelegatedPuzzleWrapper::new(
                        TreeHash::from(PREVENT_MULTIPLE_CREATE_COINS_HASH),
                        delegated_puzzle_hash,
                    )
                    .curry_tree_hash();
                }

                let (hashes, _) = self.hashes(ctx)?;

                for (key, hash) in keys.iter().zip(&hashes).take(*required) {
                    let puzzle = key.member_puzzle(ctx)?;
                    let solution = key.solution(ctx, vault, delegated_puzzle_hash.into())?;
                    set_member_spend(&mut custody, *hash, Spend::new(puzzle, solution))?;
                }
            }

            vault.spend(ctx, &custody)?;
            Ok(())
        }
    }

    impl TestVault {
        fn mint(
            sim: &mut Simulator,
            ctx: &mut SpendContext,
            custody: Custody,
            amounts: &[u64],
        ) -> anyhow::Result<Self> {
            let (_, custody_hash) = custody.hashes(ctx)?;
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

            let coins = amounts
                .iter()
                .map(|&amount| Coin::new(alice.coin.coin_id(), p2_puzzle_hash, amount))
                .collect();

            Ok(Self {
                vault,
                custody,
                p2_puzzle_hash,
                coins,
            })
        }

        fn dry_run(&self, ctx: &mut SpendContext) -> anyhow::Result<VaultDryRun> {
            let template = self.custody.template(ctx, None)?;
            Ok(VaultDryRun::new(ctx, &self.vault, &template)?)
        }
    }

    /// Spends the p2 coins of the vault and returns the vault's delegated spend, which sends each p2 coin its delegated
    /// puzzle hash and recreates the vault.
    fn spend_p2(
        ctx: &mut SpendContext,
        spends: Spends<Finished>,
        vault: &Vault,
        extra_conditions: Conditions,
    ) -> Result<Spend, DriverError> {
        let mut vault_conditions = extra_conditions;
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

        ctx.delegated_spend(vault_conditions.create_coin(
            vault.info.custody_hash.into(),
            vault.coin.amount,
            Memos::None,
        ))
    }

    fn actions() -> [Action; 1] {
        [Action::send(
            Id::Xch,
            Bytes32::new([7; 32]),
            1_000,
            Memos::None,
        )]
    }

    fn options(height: u32, extra_cost: u64) -> FeeOptions<'static> {
        let mut options = FeeOptions::new(&MAINNET_CONSTANTS, height);
        options.extra_cost = extra_cost;
        options
    }

    /// Checks the real transaction against the estimate: the cost is the same (or within the margin), the fee covers
    /// it, the mempool sees the same fee, and the simulator accepts it.
    fn check_real(
        sim: &mut Simulator,
        estimate: &FeeTransaction<()>,
        real: Vec<CoinSpend>,
        height: u32,
        margin: u64,
        submit: bool,
    ) -> anyhow::Result<()> {
        let real_cost = spend_bundle_cost(&real, &MAINNET_CONSTANTS, Some(height))?;

        if margin == 0 {
            assert_eq!(estimate.cost, real_cost);
        } else {
            assert!(estimate.cost.abs_diff(real_cost) <= margin);
        }
        assert!(estimate.fee >= minimum_fee(real_cost));

        let conds = spend_bundle_conditions(&real, &MAINNET_CONSTANTS, height)?;
        let mempool_fee = u64::try_from(conds.removal_amount - conds.addition_amount)?;
        assert_eq!(mempool_fee, estimate.fee);
        assert!(mempool_fee >= conds.cost * 5);

        if submit {
            sim.new_transaction(SpendBundle::new(real, Signature::default()))?;
        }

        Ok(())
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

    /// Prices a send from the vault with its stand-in, then builds the real transaction with the same fee and checks
    /// it. Returns the margin.
    fn price_and_check(
        sim: &mut Simulator,
        ctx: &mut SpendContext,
        test_vault: &TestVault,
        height: u32,
        submit: bool,
    ) -> anyhow::Result<u64> {
        let dry_run = test_vault.dry_run(ctx)?;
        let margin = dry_run.cost_margin();
        let spends = Spends::new(test_vault.p2_puzzle_hash);

        let estimate = spends.apply_with_minimum_fee(
            ctx,
            &actions(),
            &test_vault.coins,
            Relation::None,
            options(height, margin),
            |ctx, spends| {
                let delegated = spend_p2(ctx, spends, &dry_run.vault, Conditions::new())?;
                dry_run.spend(ctx, delegated)
            },
        )?;

        spends.apply_with_fee(
            ctx,
            &actions(),
            &test_vault.coins,
            estimate.fee,
            Relation::None,
            |ctx, spends| {
                let delegated = spend_p2(ctx, spends, &test_vault.vault, Conditions::new())?;
                test_vault
                    .custody
                    .spend_vault(ctx, &test_vault.vault, None, delegated)
                    .map_err(|error| DriverError::Custom(error.to_string()))
            },
        )?;
        let real = ctx.take();

        // The first coin covers the payment but not the fee, so the fee has to come from the second coin.
        let fee_coin = test_vault.coins[1].coin_id();
        assert!(real.iter().any(|cs| cs.coin.coin_id() == fee_coin));

        check_real(sim, &estimate, real, height, margin, submit)?;

        Ok(margin)
    }

    const AMOUNTS: [u64; 2] = [1_000, 1_000_000_000_000];

    /// The stand-in costs exactly the same as the real transaction on both sides of the fork, so it finds the fee before
    /// anyone signs.
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
        #[values(pre_fork(), post_fork())] height: u32,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let custody = Custody::keys(keys(kind, count), required);
        let test_vault = TestVault::mint(&mut sim, &mut ctx, custody, &AMOUNTS)?;

        let dry_run = test_vault.dry_run(&mut ctx)?;
        if kind == "bls" {
            // BLS members don't sign in CLVM, so a BLS vault is its own stand-in.
            assert_eq!(dry_run.vault, test_vault.vault);
        } else {
            assert_ne!(
                dry_run.vault.info.custody_hash,
                test_vault.vault.info.custody_hash
            );
        }

        // BLS members sign with AGG_SIG_ME, which the simulator checks, so the BLS case isn't submitted.
        let margin = price_and_check(&mut sim, &mut ctx, &test_vault, height, kind != "bls")?;
        let passkeys = if kind == "passkey" { required } else { 0 };
        assert_eq!(margin, PASSKEY_COST_MARGIN * passkeys as u64);

        Ok(())
    }

    /// Members that the custody spend includes but that an M of N doesn't select aren't revealed, so they aren't
    /// replaced, and they don't add to the margin.
    #[rstest]
    fn test_unselected_members_are_not_replaced(
        #[values(pre_fork(), post_fork())] height: u32,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let custody = Custody::Keys {
            keys: keys("passkey", 3),
            required: 1,
            template_includes_all: true,
            wrapper: false,
        };
        let test_vault = TestVault::mint(&mut sim, &mut ctx, custody, &AMOUNTS)?;

        let margin = price_and_check(&mut sim, &mut ctx, &test_vault, height, true)?;
        assert_eq!(margin, PASSKEY_COST_MARGIN);

        Ok(())
    }

    /// The members sign the delegated puzzle hash wrapped in the delegated puzzle wrappers on the path.
    #[rstest]
    fn test_delegated_puzzle_wrapper(
        #[values(1, 3)] count: usize,
        #[values(pre_fork(), post_fork())] height: u32,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let custody = Custody::Keys {
            keys: keys("r1", count),
            required: count.min(2),
            template_includes_all: false,
            wrapper: true,
        };
        let test_vault = TestVault::mint(&mut sim, &mut ctx, custody, &AMOUNTS)?;

        price_and_check(&mut sim, &mut ctx, &test_vault, height, true)?;

        Ok(())
    }

    /// Two vaults in one transaction get different stand-in keys, as the real vaults have, so after the fork the
    /// generator doesn't charge for one shared member puzzle where the real transaction pays for two.
    #[rstest]
    fn test_two_vaults_in_one_transaction(
        #[values(pre_fork(), post_fork())] height: u32,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let r1 = R1Pair::range_vec(2);
        let alice = TestVault::mint(
            &mut sim,
            &mut ctx,
            Custody::keys(vec![TestKey::R1(r1[0].sk.clone())], 1),
            &AMOUNTS,
        )?;
        let bob = TestVault::mint(
            &mut sim,
            &mut ctx,
            Custody::keys(vec![TestKey::R1(r1[1].sk.clone())], 1),
            &AMOUNTS,
        )?;

        let alice_dry_run = alice.dry_run(&mut ctx)?;
        let bob_dry_run = bob.dry_run(&mut ctx)?;

        // Each vault sends from its own coins in one transaction.
        let build = |ctx: &mut SpendContext,
                     fee: u64,
                     alice_vault: &Vault,
                     bob_vault: &Vault,
                     spend_alice: &dyn Fn(&mut SpendContext, Spend) -> Result<(), DriverError>,
                     spend_bob: &dyn Fn(&mut SpendContext, Spend) -> Result<(), DriverError>|
         -> Result<(), DriverError> {
            Spends::new(alice.p2_puzzle_hash).apply_with_fee(
                ctx,
                &actions(),
                &alice.coins,
                fee,
                Relation::None,
                |ctx, spends| {
                    let delegated = spend_p2(ctx, spends, alice_vault, Conditions::new())?;
                    spend_alice(ctx, delegated)
                },
            )?;
            Spends::new(bob.p2_puzzle_hash).apply_with_fee(
                ctx,
                &actions(),
                &bob.coins,
                0,
                Relation::None,
                |ctx, spends| {
                    let delegated = spend_p2(ctx, spends, bob_vault, Conditions::new())?;
                    spend_bob(ctx, delegated)
                },
            )
        };

        let estimate = spend_with_fee_for_test(&mut ctx, height, |ctx, fee| {
            build(
                ctx,
                fee,
                &alice_dry_run.vault,
                &bob_dry_run.vault,
                &|ctx, delegated| alice_dry_run.spend(ctx, delegated),
                &|ctx, delegated| bob_dry_run.spend(ctx, delegated),
            )
        })?;

        build(
            &mut ctx,
            estimate.fee,
            &alice.vault,
            &bob.vault,
            &|ctx, delegated| {
                alice
                    .custody
                    .spend_vault(ctx, &alice.vault, None, delegated)
                    .map_err(|error| DriverError::Custom(error.to_string()))
            },
            &|ctx, delegated| {
                bob.custody
                    .spend_vault(ctx, &bob.vault, None, delegated)
                    .map_err(|error| DriverError::Custom(error.to_string()))
            },
        )?;
        let real = ctx.take();

        check_real(&mut sim, &estimate, real, height, 0, true)?;

        Ok(())
    }

    fn spend_with_fee_for_test(
        ctx: &mut SpendContext,
        height: u32,
        build: impl FnMut(&mut SpendContext, u64) -> Result<(), DriverError>,
    ) -> Result<FeeTransaction<()>, DriverError> {
        crate::spend_with_minimum_fee(ctx, options(height, 0), build)
    }

    /// A vault controlled by another vault: the controlled vault's singleton member points at the controller's
    /// stand-in, and the controller's stand-in authorizes the controlled vault's spend.
    #[rstest]
    fn test_vault_controlled_by_another_vault(
        #[values(pre_fork(), post_fork())] height: u32,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let controller = TestVault::mint(
            &mut sim,
            &mut ctx,
            Custody::keys(vec![TestKey::R1(R1Pair::default().sk)], 1),
            &[],
        )?;
        let controlled = TestVault::mint(
            &mut sim,
            &mut ctx,
            Custody::Controlled(controller.vault.info.launcher_id),
            &AMOUNTS,
        )?;

        let controller_dry_run = controller.dry_run(&mut ctx)?;
        let controlled_template = controlled
            .custody
            .template(&mut ctx, Some(&controller.vault))?;
        let controlled_dry_run = VaultDryRun::with_controllers(
            &mut ctx,
            &controlled.vault,
            &controlled_template,
            std::slice::from_ref(&controller_dry_run),
        )?;

        // The controlled vault has no members that sign, so only its singleton member's solution changes.
        assert_eq!(controlled_dry_run.vault, controlled.vault);

        let spends = Spends::new(controlled.p2_puzzle_hash);

        let estimate = spends.apply_with_minimum_fee(
            &mut ctx,
            &actions(),
            &controlled.coins,
            Relation::None,
            options(height, controller_dry_run.cost_margin()),
            |ctx, spends| {
                let delegated =
                    spend_p2(ctx, spends, &controlled_dry_run.vault, Conditions::new())?;
                let delegated_puzzle_hash = ctx.tree_hash(delegated.puzzle).into();
                controlled_dry_run.spend(ctx, delegated)?;

                let authorization = controller_authorization(
                    ctx,
                    &controller_dry_run.vault,
                    controlled_dry_run.vault.coin,
                    delegated_puzzle_hash,
                    false,
                )
                .map_err(|error| DriverError::Custom(error.to_string()))?;
                controller_dry_run.spend(ctx, authorization)
            },
        )?;

        spends.apply_with_fee(
            &mut ctx,
            &actions(),
            &controlled.coins,
            estimate.fee,
            Relation::None,
            |ctx, spends| {
                let delegated = spend_p2(ctx, spends, &controlled.vault, Conditions::new())?;
                let delegated_puzzle_hash = ctx.tree_hash(delegated.puzzle).into();
                controlled
                    .custody
                    .spend_vault(ctx, &controlled.vault, Some(&controller.vault), delegated)
                    .map_err(|error| DriverError::Custom(error.to_string()))?;

                let authorization = controller_authorization(
                    ctx,
                    &controller.vault,
                    controlled.vault.coin,
                    delegated_puzzle_hash,
                    false,
                )
                .map_err(|error| DriverError::Custom(error.to_string()))?;
                controller
                    .custody
                    .spend_vault(ctx, &controller.vault, None, authorization)
                    .map_err(|error| DriverError::Custom(error.to_string()))
            },
        )?;
        let real = ctx.take();

        check_real(&mut sim, &estimate, real, height, 0, true)?;

        Ok(())
    }

    /// A controller can only be given once, and a vault can't be its own controller.
    #[test]
    fn test_controllers_are_validated() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let controller = TestVault::mint(
            &mut sim,
            &mut ctx,
            Custody::keys(vec![TestKey::R1(R1Pair::default().sk)], 1),
            &[],
        )?;
        let signing_vault = TestVault::mint(
            &mut sim,
            &mut ctx,
            Custody::Controlled(controller.vault.info.launcher_id),
            &[],
        )?;

        let controller_dry_run = controller.dry_run(&mut ctx)?;
        let signing_template = signing_vault
            .custody
            .template(&mut ctx, Some(&controller.vault))?;

        let result = VaultDryRun::with_controllers(
            &mut ctx,
            &signing_vault.vault,
            &signing_template,
            &[controller_dry_run.clone(), controller_dry_run.clone()],
        );
        assert!(matches!(
            result,
            Err(DriverError::DuplicateDryRunController(launcher_id))
                if launcher_id == controller.vault.info.launcher_id
        ));

        let own_template = controller.custody.template(&mut ctx, None)?;
        let result = VaultDryRun::with_controllers(
            &mut ctx,
            &controller.vault,
            &own_template,
            std::slice::from_ref(&controller_dry_run),
        );
        assert!(matches!(result, Err(DriverError::DryRunControllerIsVault)));

        Ok(())
    }

    /// A member selected in two M of N branches is revealed twice by the real spend. The stand-in replaces it once, so
    /// it reveals the same puzzle twice too, and costs the same as the real spend after the fork's deduplication.
    #[rstest]
    fn test_member_in_two_branches_is_replaced_once(
        #[values(pre_fork(), post_fork())] height: u32,
    ) -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        let key = TestKey::K1(K1Pair::default().sk);

        let member = key.member_puzzle(&mut ctx)?;
        let member_hash = mips_puzzle_hash(0, vec![], ctx.tree_hash(member), false);
        let branch_hashes: Vec<TreeHash> = (0..2)
            .map(|nonce| {
                mips_puzzle_hash(
                    nonce,
                    vec![],
                    MofN::new(1, vec![member_hash]).inner_puzzle_hash(),
                    false,
                )
            })
            .collect();
        let custody_hash = mips_puzzle_hash(
            0,
            vec![],
            MofN::new(2, branch_hashes.clone()).inner_puzzle_hash(),
            true,
        );

        let mut custody = MipsSpend::new(Spend::new(NodePtr::NIL, NodePtr::NIL));
        custody.members.insert(
            member_hash,
            InnerPuzzleSpend::new(0, vec![], Spend::new(member, NodePtr::NIL)),
        );
        for (nonce, &hash) in branch_hashes.iter().enumerate() {
            custody.members.insert(
                hash,
                InnerPuzzleSpend::m_of_n(nonce, vec![], 1, vec![member_hash]),
            );
        }
        custody.members.insert(
            custody_hash,
            InnerPuzzleSpend::m_of_n(0, vec![], 2, branch_hashes),
        );

        let (_, vault) =
            Launcher::new(Bytes32::new([1; 32]), 1).mint_vault(&mut ctx, custody_hash, ())?;
        ctx.take();

        let dry_run = VaultDryRun::new(&mut ctx, &vault, &custody)?;
        assert_eq!(dry_run.signers.len(), 1);
        assert_eq!(dry_run.custody.members.len(), custody.members.len());

        // The stand-in spend.
        let delegated =
            Conditions::new().create_coin(dry_run.vault.info.custody_hash.into(), 1, Memos::None);
        let delegated = Spend::new(
            ctx.alloc(&clvm_traits::clvm_quote!(delegated))?,
            NodePtr::NIL,
        );
        dry_run.spend(&mut ctx, delegated)?;
        let stand_in_cost = spend_bundle_cost(&ctx.take(), &MAINNET_CONSTANTS, Some(height))?;

        // The real spend, signed with the real key.
        let delegated = Conditions::new().create_coin(custody_hash.into(), 1, Memos::None);
        let delegated = Spend::new(
            ctx.alloc(&clvm_traits::clvm_quote!(delegated))?,
            NodePtr::NIL,
        );
        let delegated_puzzle_hash = ctx.tree_hash(delegated.puzzle).into();
        let solution = key.solution(&mut ctx, &vault, delegated_puzzle_hash)?;
        let mut real = custody.clone();
        real.delegated = delegated;
        set_member_spend(&mut real, member_hash, Spend::new(member, solution))?;
        vault.spend(&mut ctx, &real)?;
        let real_cost = spend_bundle_cost(&ctx.take(), &MAINNET_CONSTANTS, Some(height))?;

        assert_eq!(stand_in_cost, real_cost);

        Ok(())
    }
}
