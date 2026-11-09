use std::collections::{HashMap, HashSet};

use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::{
    cat::CatSolution,
    singleton::{SingletonArgs, SingletonStruct},
};
use chia_sdk_types::{Condition, Mod, puzzles::SingletonMember};
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::NodePtr;
use indexmap::{IndexMap, IndexSet};

use crate::{
    AssertedNotarizedPayment, AssertedPayment, ClawbackInfo, ClawbackPath, ClawbackV2,
    ClearSigningAsset, ControlledVaultReceiver, ControlledVaultRecovery, CustodyInfo, DriverError,
    DropCoin, Facts, Issuance, IssuanceKind, Layer, LinkedOffer, P2ConditionsOrSingletonInfo,
    P2SingletonInfo, ParsedAsset, ParsedChild, ParsedSpend, RevealedCoinSpend, Reveals, Spend,
    SpendContext, TransferType, VAULT_CONTROLLER_MESSAGE_COIN_ID, VaultLayers, VaultMessage,
    VaultOutput, build_linked_offer, get_extra_delta_message, mips_puzzle_hash,
    parse_asserted_requested_payments, parse_children, parse_delegated_spend, parse_run_cat_tail,
    parse_spend, parse_vault_controller_spend, parse_vault_delegated_spend,
    split_asserted_payments, vault_recovery_delegated_puzzle_hash,
};

/// The maximum number of vaults in an [`authorization_path`](VaultTransaction::authorization_path), which bounds the
/// work done to parse a transaction.
pub const MAX_CONTROLLED_VAULT_DEPTH: usize = 8;

/// The amount of every vault coin, which clear signing requires each vault to be recreated with.
pub const VAULT_AMOUNT: u64 = 1;

/// A summary of what a vault spend will do, as parsed by [`parse_vault_transaction`].
///
/// The purpose of this is to provide sufficient information to verify what is happening to a vault and its assets
/// as a result of a transaction at a glance. Information that is not verifiable is not included, and should not be
/// displayed from other sources. Transactions which are not fully verifiable can still be parsed, but the summary is
/// conservative: anything that the transaction could do without the vault's signature (such as spends that aren't
/// linked to the vault, or unasserted settlement payments) is left out rather than reported.
///
/// Every fact here holds *if the transaction is included in a block*. Clear signing can't tell you whether the
/// transaction is valid, only what it does if it is.
#[derive(Debug, Clone)]
pub struct VaultTransaction {
    /// The launcher id of the vault whose transaction it is. That's the vault being signed for, unless it authorizes the
    /// spend of a controlled vault, in which case it's the vault at the end of the
    /// [`authorization_path`](Self::authorization_path).
    pub launcher_id: Bytes32,
    /// The current custody hash of the vault whose transaction it is, if it's known. It's known for a controlled vault,
    /// and for the vault being signed for if it's revealed with
    /// [`Reveals::reveal_signing_vault`](crate::Reveals::reveal_signing_vault). If it's different from the custody hash
    /// of the [`vault_child`](Self::vault_child), the custody configuration is changing.
    pub custody_hash: Option<Bytes32>,
    /// If a new vault coin is created (i.e. the vault isn't melted), this will be set.
    /// It's the new inner puzzle hash and amount of the vault singleton. If the puzzle hash is different, the custody
    /// configuration has changed. The amount is always [`VAULT_AMOUNT`], which clear signing requires. The hash can be
    /// validated against a [`MipsMemo`](crate::MipsMemo) so that you know what specifically is happening.
    pub vault_child: Option<VaultOutput>,
    /// Coins with an even amount which are created by the vault singleton spend itself, for example to mint NFTs.
    /// The vault's own amount is [`VAULT_AMOUNT`], so their value comes from other coins in the transaction, and is
    /// counted as an XCH output in the [`asset_flows`](Self::asset_flows).
    pub drop_coins: Vec<DropCoin>,
    /// The spends whose output is guaranteed by the vault (see [`VerifiedSpend`]), and their children. This includes
    /// clawback push-throughs that the vault asserts, which don't need its authorization, and whose coins aren't
    /// necessarily owned by the vault.
    pub spends: Vec<VerifiedSpend>,
    /// The CAT TAILs run by verified spends, which issue or melt CATs. Each issuance records the coin id of the
    /// spend that output the `RUN_CAT_TAIL` condition, so callers can match it back to the corresponding
    /// [`VerifiedSpend`] by coin id if they want to. Its [`extra_delta`](Issuance::extra_delta) comes from the
    /// spend's solution, so it can only be relied on if it's [`verified`](Issuance::verified).
    pub issuances: Vec<Issuance>,
    /// Settlement payments which were both revealed and asserted (with `ASSERT_PUZZLE_ANNOUNCEMENT`) by the vault
    /// or a verified spend whose conditions are trusted, so the transaction is only valid if some settlement coin
    /// makes them. This doesn't say who funds them, which could be the vault's own coins or another party (such as
    /// the maker of an offer). NFT payments are matched for each owner that a settlement spend of the NFT is revealed
    /// with. Asserted payments of option contracts can't be reported, so they're an error.
    pub asserted_payments: Vec<AssertedNotarizedPayment>,
    /// Individual asserted payment outputs whose puzzle hash matches one of the vault's known p2 puzzle hashes.
    pub received_payments: Vec<AssertedPayment>,
    /// Individual asserted payment outputs whose puzzle hash doesn't match one of the vault's known p2 puzzle hashes.
    /// These are not necessarily paid by the vault, but the transaction requires them to happen.
    pub external_payments: Vec<AssertedPayment>,
    /// Per-asset value flow for verified spends, drop coins, issuances, and asserted payments. Assets with no
    /// value flow at all are omitted.
    pub asset_flows: Vec<AssetFlow>,
    /// If this transaction creates one or more offer pre-split coins, this rolls them up into a
    /// description of the future offer. Per-leg details (the individual pre-split amounts) live
    /// in the children's transfer type field.
    ///
    /// [`None`] means the transaction does not link any offer pre-split coins.
    pub linked_offer: Option<LinkedOffer>,
    /// The total of the `RESERVE_FEE` conditions of the vault spend and the verified spends whose conditions are
    /// trusted. The transaction is only valid if at least this much is left over as a fee, but it may be paid by
    /// other coins in the bundle.
    pub reserved_fee: u64,
    /// The known p2 puzzle hashes of the vault, based on revealed nonces (the first address is included by default).
    /// These are the [`P2Singleton`](crate::P2Singleton) puzzle hashes of the vault, in nonce order, and are used
    /// to tell received payments apart from external ones. Other addresses the vault can spend (such as clawbacks
    /// or [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) coins) aren't included.
    pub p2_puzzle_hashes: Vec<Bytes32>,
    /// The delegated puzzle hash that is being signed for, which is that of the vault being signed for, even if this
    /// is the transaction of a vault in its [`authorization_path`](Self::authorization_path).
    ///
    /// Use it with [`calculate_vault_coin_message`](crate::calculate_vault_coin_message) or
    /// [`calculate_vault_puzzle_message`](crate::calculate_vault_puzzle_message) to compute the message to sign,
    /// with the coin id or puzzle hash of the [`signing_vault`](Self::signing_vault). A recovery key signs it wrapped
    /// with [`vault_recovery_delegated_puzzle_hash`](crate::vault_recovery_delegated_puzzle_hash) instead.
    pub delegated_puzzle_hash: Bytes32,
    /// If the delegated spend is a [`vault_message_envelope`](crate::vault_message_envelope), these are the
    /// arbitrary messages being signed, in order. In that case, the delegated spend contains nothing else other than
    /// recreating the vault, which is [`vault_child`](Self::vault_child). It should be recreated with the vault's
    /// current custody hash and amount, which [`verify_vault_signed_message`](crate::verify_vault_signed_message)
    /// requires. That's checked if the [`custody_hash`](Self::custody_hash) is known.
    pub signed_messages: Vec<Bytes>,
    /// The vaults that authorize this vault's spend, starting with the vault being signed for. It's empty if this is
    /// the vault being signed for. Otherwise, the vault being signed for authorizes the spend of a vault that it
    /// controls, which can in turn authorize the spend of a vault that it controls, down to this vault.
    ///
    /// A vault that authorizes the spend of another vault can only recreate itself and send it the message (and for
    /// message signing, the same message to the controller message coin), which is how Cloud Wallet builds the spends of
    /// vault controllers. Everything else that the transaction does is done by this vault, so it's the only one whose
    /// spends are parsed. A controlled vault in the path that authorizes another vault must also be recreated with the
    /// same custody hash, and so must the vault being signed for if its custody hash is revealed with
    /// [`Reveals::reveal_signing_vault`](crate::Reveals::reveal_signing_vault). Otherwise its
    /// [`controller_child`](VaultAuthorization::controller_child) should be shown like a [`vault_child`](Self::vault_child).
    ///
    /// A controlled vault's puzzle is checked to be a vault, and its custody hash to be the one that its coin commits
    /// to, but not that its custody is controlled by the vault that authorizes it. Like the rest of clear signing, this
    /// assumes that the vaults in the transaction are the user's.
    pub authorization_path: Vec<VaultAuthorization>,
    /// The vault being signed for, if its custody hash was revealed with
    /// [`Reveals::reveal_signing_vault`](crate::Reveals::reveal_signing_vault). The message that's signed should be
    /// calculated from this, so that the signature can only be used if the custody hash (and coin) are correct.
    pub signing_vault: Option<SigningVault>,
}

/// The vault being signed for, with the custody hash and coin that were revealed for it.
///
/// Clear signing can't look these up, so they're only checked to be consistent with the vault's launcher id. They're
/// guaranteed by the signature as long as the message that's signed is calculated from [`puzzle_hash`](Self::puzzle_hash)
/// or [`coin`](Self::coin), since the vault can't be spent with a signature for a different puzzle hash or coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigningVault {
    /// The vault's current custody hash, as revealed. The vault is checked to be recreated with it if it authorizes
    /// the spend of a controlled vault or signs messages.
    pub custody_hash: Bytes32,
    /// The singleton puzzle hash for the vault's launcher id and custody hash, which a key member that signs with
    /// [`calculate_vault_puzzle_message`](crate::calculate_vault_puzzle_message) signs.
    pub puzzle_hash: Bytes32,
    /// The vault's coin, if it was revealed, whose id a key member that signs with
    /// [`calculate_vault_coin_message`](crate::calculate_vault_coin_message) signs. It's checked to have the puzzle
    /// hash and an amount of [`VAULT_AMOUNT`].
    pub coin: Option<Coin>,
}

impl VaultTransaction {
    /// Whether the vault signs arbitrary messages with a [`vault_message_envelope`](crate::vault_message_envelope)
    /// rather than doing anything else.
    ///
    /// Each vault in the [`authorization_path`](Self::authorization_path) must then use a
    /// [`vault_controller_envelope`](crate::vault_controller_envelope), which sends a message to
    /// [`VAULT_CONTROLLER_MESSAGE_COIN_ID`](crate::VAULT_CONTROLLER_MESSAGE_COIN_ID), so that its spend can never be
    /// valid on chain either.
    pub fn signs_messages(&self) -> bool {
        !self.signed_messages.is_empty()
    }
}

/// A vault's authorization of the spend of a vault that it controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultAuthorization {
    /// How the vault that authorizes the spend recreates itself.
    pub controller_child: VaultOutput,
    /// The controlled vault's coin. If the message is sent to its puzzle hash (for a fast-forwardable singleton member),
    /// only the puzzle hash is committed to, so the spend can happen on another coin of the vault with a different
    /// parent. Its amount is always [`VAULT_AMOUNT`] either way.
    pub coin: Coin,
    /// The controlled vault's launcher id.
    pub launcher_id: Bytes32,
    /// The controlled vault's current custody hash.
    pub custody_hash: Bytes32,
    /// Set if the spend is authorized through the controlled vault's recovery path, which starts a recovery.
    pub recovery: Option<ControlledVaultRecovery>,
}

/// How much of an asset flows into and out of the parts of a transaction that clear signing could verify.
///
/// The amounts are [`u128`] so that they can't overflow, even for a malicious transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFlow {
    /// The asset that this flow is for. NFTs are keyed by launcher id, and XCH includes bulletin coins.
    pub asset: ClearSigningAsset,
    /// The total amount of verified spends whose coins existed before the transaction (rather than being created
    /// by another verified spend).
    pub input_amount: u128,
    /// The total amount of the children of verified spends that aren't spent again within the verified spends,
    /// plus drop coins for XCH. Children sent to the settlement payments puzzle are left out, since they're paid
    /// out by asserted payments instead.
    pub output_amount: u128,
    /// The amount created by issuances: CATs issued with a TAIL, or for XCH, the value of CATs melted back into XCH.
    pub issued_amount: u128,
    /// The amount destroyed by issuances: CATs melted with a TAIL, or for XCH, the value that was turned into CATs
    /// or NFTs (for example, by issuing a CAT or minting an NFT from an XCH coin).
    pub melted_amount: u128,
    /// The total amount of asserted payments to the vault's [known p2 puzzle hashes](VaultTransaction::p2_puzzle_hashes).
    pub received_amount: u128,
    /// The total amount of asserted payments to other puzzle hashes. These must happen for the transaction to be
    /// valid, but may be paid by another party.
    pub paid_amount: u128,
    /// The amount that is spent but not accounted for by outputs, melts, payments, or (for XCH) the reserved fee,
    /// floored at zero. Value that isn't accounted for is implicitly left as a fee, so a nonzero amount should be
    /// treated as a warning. This is conservative, since `paid_amount` and the reserved fee may be covered by other
    /// parties, but they're still subtracted here.
    pub unaccounted_amount: u128,
}

/// A coin spend whose output is guaranteed by the vault being signed for.
///
/// A spend is verified if the vault sends it a message that commits to its exact conditions (or, for a spend whose
/// conditions are fixed by its puzzle, to its CAT issuance), or if it's asserted with `ASSERT_CONCURRENT_SPEND` and
/// its conditions are fixed by its puzzle (a child of a verified spend, or a clawback push-through). Either way, if
/// the transaction is valid, this coin is spent and outputs exactly the conditions in
/// [`custody`](VerifiedSpend::custody). See [`parse_vault_transaction`] for the details.
#[derive(Debug, Clone)]
pub struct VerifiedSpend {
    /// The coin being spent, and the asset it holds.
    pub asset: ParsedAsset,
    /// The clawback the coin is locked in, and the path it's spent with, if the clawback was revealed.
    pub clawback: Option<ClawbackInfo>,
    /// The p2 puzzle that controls the coin, and the conditions it outputs.
    pub custody: CustodyInfo,
    /// The coins created by the conditions, in order.
    pub children: Vec<ParsedChild>,
    /// Whether the coin is a revocable CAT being spent with its hidden puzzle (revoked by the issuer), rather than
    /// its p2 puzzle.
    pub revoked: bool,
}

/// Parses a vault spend into a [`VaultTransaction`] that summarizes what the transaction will do, so that it can be
/// reviewed by a person before the delegated puzzle is signed. This is what clear signing refers to: rather than
/// blindly signing a puzzle hash, the signer can see which coins are spent, what they create, which payments must be
/// made, and how much value is unaccounted for.
///
/// The transaction is parsed from the point of view of the vault being signed for. If the delegated spend authorizes
/// the spend of a revealed controlled vault, the vaults are followed down to the vault at the end, which is the one
/// whose transaction is returned. The vaults above it only authorize it, so they're described by
/// [`VaultTransaction::authorization_path`] instead.
///
/// # Inputs
///
/// - `delegated_spend` is the delegated puzzle and solution of the vault spend, which is what the vault's custody
///   signs. It must be a quoted list of conditions (as created by [`SpendContext::delegated_spend`]), so that the
///   conditions are fixed by the signed puzzle hash. Otherwise, this returns
///   [`DriverError::InvalidDelegatedSpendFormat`].
/// - `launcher_id` is the launcher id of the vault being signed for.
/// - `reveals` holds the coin spends of the transaction, and anything that can't be derived from them: the clawbacks,
///   [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) puzzles, and bags of the coins involved, the vault
///   nonces of the vault whose transaction it is, the spends of controlled vaults, and the current custody hash (and
///   coin) of the vault being signed for. Everything in it comes from an untrusted source, and is only used if it's
///   consistent with the coins and the signed conditions. See [`Reveals`].
///
/// # What is verified
///
/// *The vault's own spend.* The conditions of the delegated spend are fixed by the signature, so they're treated as
/// fact. An odd `CREATE_COIN` is the new vault singleton, which must have an amount of [`VAULT_AMOUNT`], and any other
/// `CREATE_COIN` is a [`DropCoin`]. Every `SEND_MESSAGE` must be sent from the vault's puzzle to a coin id, or to a
/// puzzle hash (which is only allowed for a controlled vault). Its `ASSERT_SECONDS_ABSOLUTE`,
/// `ASSERT_BEFORE_SECONDS_ABSOLUTE`, `RESERVE_FEE`, `ASSERT_PUZZLE_ANNOUNCEMENT`, and `ASSERT_CONCURRENT_SPEND`
/// conditions are recorded.
///
/// *Spends that the vault sends a message to.* The coin's spend must be revealed, and its conditions must be fixed by
/// the message or its puzzle. Either its p2 puzzle is a [`P2Singleton`](crate::P2Singleton) or revealed
/// [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) of this vault (directly, or inside a revealed
/// clawback), and the message is the tree hash of the quoted conditions it outputs, or its p2 puzzle is itself a
/// quoted list of conditions. So the spend can't output anything else without failing the whole transaction. A
/// [`P2ConditionsOrSingleton`](crate::P2ConditionsOrSingleton) spend must also assert its own coin id. A CAT spend
/// that runs an everything with singleton TAIL of this vault (whose singleton struct is the vault's) may also receive a
/// message that commits to its extra delta (which is the only message that a quoted list of conditions can receive),
/// and that marks the [`Issuance`] as [`verified`](Issuance::verified). For a controlled vault, that's the controlled
/// vault's TAIL, since it sends the message. Each message must match exactly one of these commitments, and a spend that
/// needs a message must receive one, or an error is returned. A coin spent with a clawback's receiver path is only
/// accepted if the transaction can't be valid before the clawback expires (with `ASSERT_SECONDS_ABSOLUTE` from the
/// vault, or from a spend that the vault sends a message to that isn't in a clawback), since the sender path could
/// otherwise consume the message.
///
/// *Spends that are asserted.* The spend of a child of a verified spend is also verified if the vault or a verified
/// spend whose conditions are trusted asserts it with `ASSERT_CONCURRENT_SPEND`, and its p2 puzzle is a quoted list
/// of conditions (possibly inside a revealed clawback, whose receiver path is only accepted once the clawback has
/// expired). Any revealed clawback spent with its push-through path is verified if its spend is asserted, its amount
/// matches the clawback, and the clawback has expired, with the conditions that the clawback commits to rather than
/// the revealed solution. A spend that's asserted but can't be parsed returns the error from parsing it.
///
/// *Clawbacks.* If a verified spend claws a coin back with the sender path of a clawback, the transaction must also
/// be guaranteed to expire (with `ASSERT_BEFORE_SECONDS_ABSOLUTE` from the vault, or from a verified spend that isn't
/// in a clawback) no later than the clawback does, since another path could otherwise be spent instead. The
/// conditions of a clawback spend (its assertions and reserved fees) are only trusted once the time bounds rule out
/// the paths it could be replaced with: the sender path once the transaction is guaranteed to expire in time, the
/// receiver path once the clawback has expired and only if the vault sends it a message, and the push-through path
/// never.
///
/// *Settlement payments and offers.* Settlement payments are only reported if their puzzle announcement is asserted
/// by the vault or a verified spend whose conditions are trusted. Coins set aside for a future offer are rolled up
/// into a [`LinkedOffer`], and must belong to this vault. An asserted payment of an option contract is an error, since
/// it can't be reported.
///
/// *Message signing.* If the delegated spend is a [`vault_message_envelope`](crate::vault_message_envelope), it can't
/// do anything other than recreate the vault, and the messages are returned in
/// [`VaultTransaction::signed_messages`]. If the vault's custody hash is known, it must be recreated with it. If it's
/// a controlled vault, every vault in the authorization path must use a
/// [`vault_controller_envelope`](crate::vault_controller_envelope), so that its spend can never be valid on chain.
///
/// *Controlled vaults.* A vault that sends a message to a revealed controlled vault (or to
/// [`VAULT_CONTROLLER_MESSAGE_COIN_ID`]) can only recreate itself with an amount of [`VAULT_AMOUNT`] and send that
/// one message (plus the same message to [`VAULT_CONTROLLER_MESSAGE_COIN_ID`] for a controller envelope). It must be
/// recreated with its custody hash if that's known, which it is for a controlled vault, and for the vault being
/// signed for if it's revealed. The controlled vault's coin spend must be revealed, its puzzle must be a vault, and
/// the message must be its delegated puzzle hash (wrapped with
/// [`vault_recovery_delegated_puzzle_hash`](crate::vault_recovery_delegated_puzzle_hash) if it's authorized through
/// its recovery path). A message to a puzzle hash must match exactly one revealed controlled vault. The path can be at
/// most [`MAX_CONTROLLED_VAULT_DEPTH`] vaults long, and can't visit a vault twice.
///
/// *The vault being signed for.* If its custody hash is revealed, the [`SigningVault`] puzzle hash is calculated from
/// it and the launcher id, and a revealed coin must have that puzzle hash and an amount of [`VAULT_AMOUNT`].
///
/// # What isn't verified
///
/// - Whether the transaction is valid, for example whether its signatures are correct, the coins exist, or the
///   fee is left over. Every fact only holds if the transaction is included in a block.
/// - Spends that aren't linked to the vault. They may be in the same spend bundle, but nothing they do is reported
///   unless it's asserted by the vault or a verified spend.
/// - Who funds asserted payments, which may be the vault's own coins, or another party.
/// - The meaning of [`VaultTransaction::vault_child`], which is a new custody hash. Compare it to the current custody
///   hash, or reveal it with a [`MipsMemo`](crate::MipsMemo), to know what it means.
/// - That `launcher_id` and the controlled vaults are the user's, and that a controlled vault's custody actually
///   includes the vault that authorizes it.
/// - The revealed custody hash and coin of the vault being signed for, other than being consistent with its launcher
///   id. They're only guaranteed if the message that's signed is calculated from [`VaultTransaction::signing_vault`].
///   If they aren't revealed, the vault being signed for isn't checked to be recreated as it was when it authorizes a
///   controlled vault or signs messages.
/// - The extra delta of an [`Issuance`] that isn't [`verified`](Issuance::verified), which is left out of the
///   [`AssetFlow`]s.
/// - What happens to the payments of a Secure the Bag once it's unrolled (a revocable CAT's issuer can still revoke
///   the bag), and the human readable memos of a child, which are arbitrary text.
///
/// # Errors
///
/// Returns an error rather than a partial summary whenever something linked to the vault can't be verified. Among
/// others:
///
/// - [`DriverError::MissingSpend`] for a message to a coin whose spend isn't revealed or belongs to another vault, or
///   a message to a puzzle hash that isn't a revealed controlled vault.
/// - [`DriverError::InvalidLinkedCustody`] for a message to a coin whose p2 puzzle isn't recognized.
/// - [`DriverError::WrongConditions`], [`DriverError::UnmatchedVaultMessage`],
///   [`DriverError::DuplicateVaultMessage`], or [`DriverError::MissingVaultMessage`] if the messages don't match the
///   spends, including an extra delta message to a CAT whose TAIL is authorized by another singleton.
/// - [`DriverError::UnguaranteedClawBack`] or [`DriverError::UnguaranteedClawbackExpiry`] if a clawback spend could
///   be replaced with another path.
/// - [`DriverError::RevocableChild`] for a CAT child whose revocation layer differs from its parent's.
/// - [`DriverError::WrongLinkedOfferLauncherId`] for pre-split offer coins of another vault.
/// - [`DriverError::UnsupportedAssertedOptionPayment`] for an asserted payment of an option contract.
/// - [`DriverError::InvalidVaultAmount`] if a vault isn't recreated with [`VAULT_AMOUNT`].
/// - [`DriverError::MixedVaultMessageEnvelope`] or [`DriverError::VaultMessageEnvelopeChangesVault`] for a vault
///   message envelope that does something else.
/// - [`DriverError::InvalidVaultControllerSpend`], [`DriverError::InvalidVaultControllerEnvelope`],
///   [`DriverError::VaultControllerChangesVault`], [`DriverError::InvalidControlledVaultPuzzle`],
///   [`DriverError::WrongControlledVaultMessage`], [`DriverError::ControlledVaultTooDeep`], or
///   [`DriverError::DuplicateControlledVault`] for an invalid authorization of a controlled vault.
/// - [`DriverError::WrongSigningVaultPuzzleHash`] if the revealed coin of the vault being signed for doesn't match.
///
/// # Example
///
/// A vault-owned XCH coin sends 900 mojos to Bob and reserves a 100 mojo fee:
///
/// ```
/// use chia_protocol::{Bytes32, Coin};
/// use chia_puzzle_types::Memos;
/// use chia_sdk_driver::{
///     ClearSigningAsset, P2Singleton, Reveals, SpendContext, TransferType,
///     calculate_vault_puzzle_message, parse_vault_transaction,
/// };
/// use chia_sdk_types::{Conditions, MessageFlags, MessageSide};
/// use clvm_utils::ToTreeHash;
///
/// let mut ctx = SpendContext::new();
///
/// // The vault being signed for, and its current custody puzzle hash.
/// let launcher_id = Bytes32::new([1; 32]);
/// let custody_hash = Bytes32::new([2; 32]);
///
/// // An XCH coin at the vault's first p2 puzzle hash.
/// let p2 = P2Singleton::new(launcher_id, 0);
/// let coin = Coin::new(Bytes32::new([3; 32]), p2.tree_hash().into(), 1_000);
///
/// // The conditions the coin will output. The vault authorizes them by sending the coin a message.
/// let bob = Bytes32::new([4; 32]);
/// let coin_conditions =
///     ctx.delegated_spend(Conditions::new().create_coin(bob, 900, Memos::None).reserve_fee(100))?;
/// let coin_spend = p2.spend(&mut ctx, custody_hash, 1, coin_conditions)?;
/// ctx.spend(coin, coin_spend)?;
/// let coin_spends = ctx.take();
///
/// // The delegated spend that the vault's custody signs, which sends the message and recreates the vault.
/// let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
///     | MessageFlags::COIN.encode(MessageSide::Receiver);
/// let message = ctx.tree_hash(coin_conditions.puzzle).to_vec().into();
/// let coin_id = ctx.alloc(&coin.coin_id())?;
/// let delegated_spend = ctx.delegated_spend(
///     Conditions::new()
///         .send_message(mode, message, vec![coin_id])
///         .create_coin(custody_hash, 1, Memos::None),
/// )?;
///
/// // The vault's current custody hash is revealed too, so that the message to sign commits to it.
/// let mut reveals = Reveals::from_coin_spends(&mut ctx, &coin_spends)?;
/// reveals.reveal_signing_vault(custody_hash, None);
///
/// let tx = parse_vault_transaction(reveals, &mut ctx, launcher_id, delegated_spend)?;
///
/// // The vault is recreated with the same custody, so its configuration doesn't change.
/// assert_eq!(tx.vault_child.unwrap().custody_hash, custody_hash);
/// assert_eq!(tx.reserved_fee, 100);
///
/// let child = &tx.spends[0].children[0];
/// assert_eq!(child.memos.p2_puzzle_hash, bob);
/// assert_eq!(child.asset.coin().amount, 900);
/// assert_eq!(child.transfer_type, TransferType::Sent);
///
/// // Every mojo spent is accounted for by the payment to Bob and the fee.
/// let xch = tx.asset_flows.iter().find(|flow| flow.asset == ClearSigningAsset::Xch).unwrap();
/// assert_eq!(xch.input_amount, 1_000);
/// assert_eq!(xch.output_amount, 900);
/// assert_eq!(xch.unaccounted_amount, 0);
///
/// // A key member that signs the vault's puzzle hash signs this message. If the revealed custody hash were wrong,
/// // the signature couldn't be used to spend the vault.
/// let signing_vault = tx.signing_vault.unwrap();
/// let message = calculate_vault_puzzle_message(tx.delegated_puzzle_hash, signing_vault.puzzle_hash);
/// assert_eq!(message.len(), 64);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn parse_vault_transaction(
    reveals: Reveals,
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    delegated_spend: Spend,
) -> Result<VaultTransaction, DriverError> {
    let signed_delegated_puzzle_hash = tree_hash(ctx, delegated_spend.puzzle).into();

    // The vault being signed for can only be spent with a signature on its puzzle hash or coin id, so the revealed
    // custody hash and coin are correct if the message that's signed is calculated from them.
    let signing_vault = reveals
        .signing_vault()
        .map(|revealed| {
            let puzzle_hash: Bytes32 =
                SingletonArgs::curry_tree_hash(launcher_id, revealed.custody_hash.into()).into();

            if let Some(coin) = revealed.coin {
                if coin.puzzle_hash != puzzle_hash {
                    return Err(DriverError::WrongSigningVaultPuzzleHash);
                }

                if coin.amount != VAULT_AMOUNT {
                    return Err(DriverError::InvalidVaultAmount);
                }
            }

            Ok(SigningVault {
                custody_hash: revealed.custody_hash,
                puzzle_hash,
                coin: revealed.coin,
            })
        })
        .transpose()?;

    let mut launcher_id = launcher_id;
    let mut custody_hash = signing_vault.map(|signing_vault| signing_vault.custody_hash);
    let mut delegated_spend = delegated_spend;
    let mut authorization_path = Vec::new();
    let mut is_envelope_path = true;

    // Each vault can only be reached once, so that the chain can't be followed around a cycle forever.
    let mut visited = HashSet::from([launcher_id]);

    while let Some(parsed) =
        parse_vault_authorization(&reveals, ctx, custody_hash, delegated_spend)?
    {
        if authorization_path.len() >= MAX_CONTROLLED_VAULT_DEPTH {
            return Err(DriverError::ControlledVaultTooDeep);
        }

        if !visited.insert(parsed.authorization.launcher_id) {
            return Err(DriverError::DuplicateControlledVault);
        }

        launcher_id = parsed.authorization.launcher_id;
        custody_hash = Some(parsed.authorization.custody_hash);
        delegated_spend = parsed.delegated_spend;
        is_envelope_path &= parsed.is_envelope;
        authorization_path.push(parsed.authorization);
    }

    let mut transaction = parse_vault(reveals, ctx, launcher_id, custody_hash, delegated_spend)?;

    // A vault that authorizes another vault to sign messages must also send its message to the controller message coin,
    // so that its spend can never be valid on chain, even if the controlled vault's custody can receive the message some
    // other way than with a vault message envelope. This is the same check that `verify_controlled_vault_signed_message`
    // does.
    if !transaction.signed_messages.is_empty() && !is_envelope_path {
        return Err(DriverError::InvalidVaultControllerEnvelope);
    }

    transaction.delegated_puzzle_hash = signed_delegated_puzzle_hash;
    transaction.authorization_path = authorization_path;
    transaction.signing_vault = signing_vault;

    Ok(transaction)
}

/// A vault's authorization of the spend of a controlled vault.
struct ParsedVaultAuthorization {
    authorization: VaultAuthorization,
    /// The controlled vault's delegated spend.
    delegated_spend: Spend,
    /// Whether the authorization is a [`vault_controller_envelope`](crate::vault_controller_envelope).
    is_envelope: bool,
}

/// Parses a vault's delegated spend as the authorization of a revealed controlled vault's spend, or returns [`None`] if
/// it doesn't authorize one. The custody hash is known for a controlled vault, and for the vault being signed for if
/// it's revealed.
fn parse_vault_authorization(
    reveals: &Reveals,
    ctx: &mut SpendContext,
    custody_hash: Option<Bytes32>,
    delegated_spend: Spend,
) -> Result<Option<ParsedVaultAuthorization>, DriverError> {
    let vault_spend = parse_vault_delegated_spend(&mut Facts::default(), ctx, delegated_spend)?;

    // A message to the controller message coin can never be received, so it only makes the spend invalid on chain,
    // which a vault that authorizes another vault to sign messages must do. It's only allowed in a controller envelope.
    let has_controller_marker = vault_spend
        .messages
        .iter()
        .any(|message| message.spent_coin_id == VAULT_CONTROLLER_MESSAGE_COIN_ID);

    let mut authorizes_controlled_vault = vault_spend
        .messages
        .iter()
        .any(|message| reveals.controlled_vault(message.spent_coin_id).is_some());

    for message in &vault_spend.puzzle_messages {
        authorizes_controlled_vault |= reveals
            .controlled_vault_by_puzzle_hash(message.puzzle_hash)?
            .is_some();
    }

    if !has_controller_marker && !authorizes_controlled_vault {
        return Ok(None);
    }

    // A vault that authorizes the spend of a controlled vault can only recreate itself and send that one message, which
    // is how Cloud Wallet builds the spends of vault controllers. That way, everything a transaction does is done by the
    // vault at the end of the chain.
    let conditions = parse_delegated_spend(ctx, delegated_spend)?;

    let Some(controller_spend) = parse_vault_controller_spend(ctx, &conditions)? else {
        return Err(if has_controller_marker {
            DriverError::InvalidVaultControllerEnvelope
        } else {
            DriverError::InvalidVaultControllerSpend
        });
    };

    if controller_spend.vault_child.amount != VAULT_AMOUNT {
        return Err(DriverError::InvalidVaultAmount);
    }

    // A controlled vault must also be recreated as it was, since its custody hash is known.
    if let Some(custody_hash) = custody_hash
        && controller_spend.vault_child != VaultOutput::new(custody_hash, VAULT_AMOUNT)
    {
        return Err(DriverError::VaultControllerChangesVault);
    }

    // A message to a puzzle hash is sent to a controlled vault whose singleton member is fast-forwardable. It isn't
    // allowed for any other coin, since any coin with the puzzle hash could receive it, which would let a different
    // coin than the one that's revealed be spent. A vault's puzzle hash commits to its launcher id and custody hash, so
    // the controlled vault's delegated spend is authorized the same way on any of its coins.
    let controlled_vault = *match controller_spend.controlled_vault {
        ControlledVaultReceiver::CoinId(coin_id) => reveals.controlled_vault(coin_id),
        ControlledVaultReceiver::PuzzleHash(puzzle_hash) => {
            reveals.controlled_vault_by_puzzle_hash(puzzle_hash)?
        }
    }
    .ok_or(DriverError::MissingSpend)?;

    // A message authorizes whatever the coin's puzzle does with it, so the controlled vault's coin spend must be
    // revealed, and its puzzle must be a vault. Otherwise, an asset that the vault owns (such as an NFT, which is also a
    // singleton) could be revealed as a controlled vault, which would hide that it's being spent.
    let coin_spend = reveals
        .coin_spend(controlled_vault.coin.coin_id())
        .ok_or(DriverError::MissingSpend)?;

    if VaultLayers::parse_puzzle(ctx, coin_spend.puzzle)?.is_none() {
        return Err(DriverError::InvalidControlledVaultPuzzle);
    }

    let controlled_delegated_puzzle_hash =
        tree_hash(ctx, controlled_vault.delegated_spend.puzzle).into();

    // A member of the recovery path is passed the delegated puzzle hash wrapped in the recovery path's delegated puzzle
    // wrappers, which only restrict what the delegated puzzle can do.
    let signed_delegated_puzzle_hash = match controlled_vault.recovery {
        Some(recovery) => vault_recovery_delegated_puzzle_hash(
            controlled_delegated_puzzle_hash,
            recovery.left_side_subtree_hash,
            recovery.recovery_timelock,
            recovery.version,
        ),
        None => controlled_delegated_puzzle_hash,
    };

    if controller_spend.message.as_ref() != signed_delegated_puzzle_hash.as_ref() {
        return Err(DriverError::WrongControlledVaultMessage);
    }

    Ok(Some(ParsedVaultAuthorization {
        authorization: VaultAuthorization {
            controller_child: controller_spend.vault_child,
            coin: controlled_vault.coin,
            launcher_id: controlled_vault.launcher_id,
            custody_hash: controlled_vault.custody_hash,
            recovery: controlled_vault.recovery,
        },
        delegated_spend: controlled_vault.delegated_spend,
        is_envelope: controller_spend.is_envelope,
    }))
}

/// Parses the spend of the vault whose transaction it is, which is the vault being signed for, or the vault at the end
/// of its authorization path. The custody hash is known for a controlled vault, and for the vault being signed for if
/// it's revealed.
fn parse_vault(
    mut reveals: Reveals,
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    custody_hash: Option<Bytes32>,
    delegated_spend: Spend,
) -> Result<VaultTransaction, DriverError> {
    let mut facts = Facts::default();

    let vault_spend = parse_vault_delegated_spend(&mut facts, ctx, delegated_spend)?;

    // Every vault is assumed to have an amount of 1, which is enforced here so that it stays that way. The vault's
    // amount isn't otherwise known, since the coin of the vault being signed for isn't given, and a controlled vault
    // whose message is sent to its puzzle hash could be spent with any of its coins. If the amount could be more than
    // 1, recreating it with 1 would hide that the rest is spent as a fee.
    if vault_spend
        .child
        .is_some_and(|child| child.amount != VAULT_AMOUNT)
    {
        return Err(DriverError::InvalidVaultAmount);
    }

    let mut parsed_spends = HashMap::new();
    let mut parse_errors = HashMap::new();

    for spend in reveals.coin_spends().copied().collect::<Vec<_>>() {
        // Spends which aren't linked to the vault are ignored, so they shouldn't prevent the
        // transaction from being parsed. The error is only returned if the spend is linked.
        let parsed_spend = match parse_spend(&mut reveals, ctx, &spend) {
            Ok(parsed_spend) => parsed_spend,
            Err(error) => {
                parse_errors.insert(spend.coin.coin_id(), error);
                continue;
            }
        };

        if let Some(
            CustodyInfo::P2Singleton(P2SingletonInfo {
                launcher_id: spend_launcher_id,
                ..
            })
            | CustodyInfo::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
                launcher_id: spend_launcher_id,
                ..
            }),
        ) = &parsed_spend.custody
            && spend_launcher_id != &launcher_id
        {
            continue;
        }

        parsed_spends.insert(spend.coin.coin_id(), parsed_spend);
    }

    // A message to a controlled vault or to the controller message coin would have made this vault part of the
    // authorization path, so each message here authorizes the spend of a coin.
    let mut messages_by_coin: IndexMap<Bytes32, Vec<VaultMessage>> = IndexMap::new();

    for message in vault_spend.messages {
        messages_by_coin
            .entry(message.spent_coin_id)
            .or_default()
            .push(message);
    }

    // A message to a puzzle hash is only allowed for a controlled vault, since any coin with the puzzle hash could
    // receive it, which would let a different coin than the one that's revealed be spent.
    if !vault_spend.puzzle_messages.is_empty() {
        return Err(DriverError::MissingSpend);
    }

    // The time bounds decide which clawback spends can be trusted, so they're recorded up front from the spends
    // that the vault authorizes and that can't be replaced. Any mismatched message fails the transaction below.
    for coin_id in messages_by_coin.keys() {
        let Some(ParsedSpend {
            clawback: None,
            custody: Some(custody),
            ..
        }) = parsed_spends.get(coin_id)
        else {
            continue;
        };

        if !custody.receives_message() {
            continue;
        }

        for condition in custody.conditions() {
            match condition {
                Condition::AssertSecondsAbsolute(condition) => {
                    facts.update_actual_earliest_time(condition.seconds);
                }
                Condition::AssertBeforeSecondsAbsolute(condition) => {
                    facts.update_actual_expiration_time(condition.seconds);
                }
                _ => {}
            }
        }
    }

    let mut verified_spends = Vec::new();
    let mut issuances: Vec<Issuance> = Vec::new();

    // Indices of verified sender path spends whose conditions haven't been recorded, since the expiration time
    // didn't rule out the other paths yet when they were verified.
    let mut untrusted_senders = Vec::new();

    // Children of verified spends that are yet to be visited.
    let mut stack: IndexSet<Bytes32> = IndexSet::new();

    for (coin_id, messages) in messages_by_coin {
        let spend = *reveals
            .coin_spend(coin_id)
            .ok_or(DriverError::MissingSpend)?;
        let Some(parsed_spend) = parsed_spends.remove(&coin_id) else {
            return Err(parse_errors
                .remove(&coin_id)
                .unwrap_or(DriverError::MissingSpend));
        };

        // The sender path could consume the message instead, unless the clawback has expired.
        if let Some(ClawbackInfo {
            clawback,
            path: ClawbackPath::Receiver,
        }) = parsed_spend.clawback
            && !is_expired(&facts, &clawback)
        {
            return Err(DriverError::UnguaranteedClawbackExpiry);
        }

        let Some(verified_spend) = verify_spend(
            &mut reveals,
            &mut facts,
            ctx,
            spend,
            launcher_id,
            parsed_spend,
            &messages,
            &mut issuances,
        )?
        else {
            return Err(DriverError::InvalidLinkedCustody);
        };

        push_verified_spend(
            &facts,
            &mut verified_spends,
            &mut untrusted_senders,
            &mut stack,
            verified_spend,
        );
    }

    // Coins that were visited before anything asserted their spend, and the number of verified spends when they
    // were last revisited.
    let mut unasserted: IndexSet<Bytes32> = IndexSet::new();
    let mut revisited_count = verified_spends.len();

    // Once a clawback has expired, anyone can push it through to the receiver without a signature, so the vault
    // only asserts that the clawback coin is spent. The coin can then only be spent by its receiver (the receiver
    // path or the push-through path), unless it's clawed back by the sender. The sender path asserts that the
    // clawback hasn't expired, so if the vault asserts that it has, the clawback is going to the receiver.
    //
    // The children of a push-through can assert other spends, including other push-throughs, which is why this
    // repeats until nothing else qualifies.
    loop {
        while let Some(coin_id) = stack.pop() {
            if !facts.is_spend_asserted(coin_id) {
                unasserted.insert(coin_id);
                continue;
            }

            // Spends without custody stay parsed, since they may be verifiable push-throughs.
            if parsed_spends
                .get(&coin_id)
                .is_none_or(|spend| spend.custody.is_none())
            {
                continue;
            }

            let Some(parsed_spend) = parsed_spends.remove(&coin_id) else {
                continue;
            };

            // Without a vault message, the receiver path could be replaced with the sender path.
            if let Some(ClawbackInfo {
                clawback,
                path: ClawbackPath::Receiver,
            }) = parsed_spend.clawback
                && !parsed_spend
                    .custody
                    .as_ref()
                    .is_some_and(CustodyInfo::receives_message)
                && !is_expired(&facts, &clawback)
            {
                continue;
            }

            let spend = *reveals
                .coin_spend(coin_id)
                .ok_or(DriverError::MissingSpend)?;

            let Some(verified_spend) = verify_spend(
                &mut reveals,
                &mut facts,
                ctx,
                spend,
                launcher_id,
                parsed_spend,
                &[],
                &mut issuances,
            )?
            else {
                continue;
            };

            push_verified_spend(
                &facts,
                &mut verified_spends,
                &mut untrusted_senders,
                &mut stack,
                verified_spend,
            );
        }

        // Spends verified later in the traversal can lower the expiration time, which can make a sender path spend
        // trusted after the fact, so its conditions are recorded then.
        let (trusted_senders, still_untrusted): (Vec<usize>, Vec<usize>) = untrusted_senders
            .into_iter()
            .partition(|&index| !is_untrusted_sender(&facts, &verified_spends[index]));
        untrusted_senders = still_untrusted;

        for &index in &trusted_senders {
            let spend = &verified_spends[index];
            record_facts(
                &mut facts,
                spend.custody.conditions(),
                spend.clawback,
                spend.custody.receives_message(),
            );
        }

        // A spend can be asserted after the traversal has already visited its coin, so the coins that weren't
        // asserted are revisited whenever something new is verified or trusted.
        if (verified_spends.len() > revisited_count || !trusted_senders.is_empty())
            && !unasserted.is_empty()
        {
            revisited_count = verified_spends.len();
            stack.extend(unasserted.drain(..));
            continue;
        }

        // The revealed coin spends are iterated in order rather than the parsed spends, which are unordered.
        let push_throughs: Vec<(Bytes32, ClawbackV2)> = reveals
            .coin_spends()
            .filter_map(|spend| {
                let coin_id = spend.coin.coin_id();
                match parsed_spends.get(&coin_id)?.clawback {
                    Some(ClawbackInfo {
                        clawback,
                        path: ClawbackPath::PushThrough,
                    }) if spend.coin.amount == clawback.amount
                        && facts.is_spend_asserted(coin_id)
                        && is_expired(&facts, &clawback) =>
                    {
                        Some((coin_id, clawback))
                    }
                    _ => None,
                }
            })
            .collect();

        if push_throughs.is_empty() {
            break;
        }

        for (coin_id, clawback) in push_throughs {
            let mut parsed_spend = parsed_spends
                .remove(&coin_id)
                .ok_or(DriverError::MissingSpend)?;

            let spend = *reveals
                .coin_spend(coin_id)
                .ok_or(DriverError::MissingSpend)?;

            // The conditions are determined by the clawback rather than by the revealed solution, since the
            // clawback's puzzle hash commits to them.
            let push_through_path = ctx.alloc(&clawback.push_through_path())?;
            let conditions =
                parse_delegated_spend(ctx, Spend::new(push_through_path, NodePtr::NIL))?;
            parsed_spend.custody = Some(CustodyInfo::DelegatedConditions(conditions));

            let Some(verified_spend) = verify_spend(
                &mut reveals,
                &mut facts,
                ctx,
                spend,
                launcher_id,
                parsed_spend,
                &[],
                &mut issuances,
            )?
            else {
                continue;
            };

            push_verified_spend(
                &facts,
                &mut verified_spends,
                &mut untrusted_senders,
                &mut stack,
                verified_spend,
            );
        }
    }

    // A spend that the transaction asserts is linked even without a vault message, so it can't be
    // ignored if it couldn't be parsed. The revealed coin spends are iterated in order, so that the
    // error is deterministic.
    for spend in reveals.coin_spends() {
        let coin_id = spend.coin.coin_id();

        if facts.is_spend_asserted(coin_id)
            && let Some(error) = parse_errors.remove(&coin_id)
        {
            return Err(error);
        }
    }

    // If the transaction expires after the required expiration time of the spend,
    // we can't guarantee that the transaction will expire when the spend expires,
    // which is a security vulnerability.
    if facts.required_expiration_time().is_some_and(|required| {
        facts
            .actual_expiration_time()
            .is_none_or(|expiration| expiration > required)
    }) {
        return Err(DriverError::UnguaranteedClawBack);
    }

    let delegated_puzzle_hash = tree_hash(ctx, delegated_spend.puzzle).into();

    let p2_puzzle_hashes = calculate_p2_puzzle_hashes(&reveals, launcher_id);
    let p2_puzzle_hash_set = p2_puzzle_hashes.iter().copied().collect();
    let reserved_fee = facts.reserved_fees().try_into()?;
    let asserted_payments = parse_asserted_requested_payments(&reveals, &facts, ctx)?;
    let split_payments = split_asserted_payments(
        &asserted_payments,
        &p2_puzzle_hash_set,
        reveals.asset_info(),
    );
    let linked_offer = build_linked_offer(
        &reveals,
        ctx,
        &verified_spends,
        launcher_id,
        &p2_puzzle_hash_set,
    )?;

    // Hydrate ephemerally spent bulletin children.
    let mut bulletins = HashMap::new();

    for spend in &verified_spends {
        if let ParsedAsset::Bulletin(bulletin) = &spend.asset {
            bulletins.insert(bulletin.coin.coin_id(), bulletin.clone());
        }
    }

    for spend in &mut verified_spends {
        for child in &mut spend.children {
            if matches!(child.asset, ParsedAsset::Xch(_))
                && let Some(bulletin) = bulletins.remove(&child.asset.coin().coin_id())
            {
                child.asset = ParsedAsset::Bulletin(bulletin);
            }
        }
    }

    let asset_flows = build_asset_flows(
        &verified_spends,
        &split_payments.received_payments,
        &split_payments.external_payments,
        &vault_spend.drop_coins,
        &issuances,
        reserved_fee,
    );

    // A vault that signs messages must be recreated as it was, which is also what `verify_vault_signed_message`
    // requires. This can only be checked if the vault's custody hash is known.
    if !vault_spend.signed_messages.is_empty()
        && let Some(custody_hash) = custody_hash
        && vault_spend.child != Some(VaultOutput::new(custody_hash, VAULT_AMOUNT))
    {
        return Err(DriverError::VaultMessageEnvelopeChangesVault);
    }

    Ok(VaultTransaction {
        launcher_id,
        custody_hash,
        vault_child: vault_spend.child,
        drop_coins: vault_spend.drop_coins,
        spends: verified_spends,
        issuances,
        asserted_payments,
        received_payments: split_payments.received_payments,
        external_payments: split_payments.external_payments,
        asset_flows,
        linked_offer,
        reserved_fee,
        p2_puzzle_hashes,
        delegated_puzzle_hash,
        signed_messages: vault_spend.signed_messages,
        authorization_path: Vec::new(),
        signing_vault: None,
    })
}

/// Whether the transaction can only be valid once the clawback has expired, which rules out its sender path.
fn is_expired(facts: &Facts, clawback: &ClawbackV2) -> bool {
    facts
        .actual_earliest_time()
        .is_some_and(|earliest| earliest >= clawback.seconds)
}

/// Adds a verified spend, queues its children to be visited, and keeps track of it if it's a sender path spend whose
/// conditions couldn't be recorded yet.
fn push_verified_spend(
    facts: &Facts,
    verified_spends: &mut Vec<VerifiedSpend>,
    untrusted_senders: &mut Vec<usize>,
    stack: &mut IndexSet<Bytes32>,
    verified_spend: VerifiedSpend,
) {
    for child in &verified_spend.children {
        stack.insert(child.asset.coin().coin_id());
    }

    if is_untrusted_sender(facts, &verified_spend) {
        untrusted_senders.push(verified_spends.len());
    }

    verified_spends.push(verified_spend);
}

/// Whether the transaction expires before the clawback, which rules out the paths a sender path spend could be
/// replaced with.
fn is_sender_trusted(facts: &Facts, clawback: &ClawbackV2) -> bool {
    facts
        .actual_expiration_time()
        .is_some_and(|expiration| expiration <= clawback.seconds)
}

/// Whether a verified spend takes the sender path, but its conditions couldn't be recorded yet.
fn is_untrusted_sender(facts: &Facts, spend: &VerifiedSpend) -> bool {
    matches!(
        spend.clawback,
        Some(ClawbackInfo {
            clawback,
            path: ClawbackPath::Sender,
        }) if !is_sender_trusted(facts, &clawback)
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_spend(
    reveals: &mut Reveals,
    facts: &mut Facts,
    allocator: &mut SpendContext,
    spend: RevealedCoinSpend,
    launcher_id: Bytes32,
    parsed_spend: ParsedSpend,
    messages: &[VaultMessage],
    issuances: &mut Vec<Issuance>,
) -> Result<Option<VerifiedSpend>, DriverError> {
    let Some(custody) = parsed_spend.custody else {
        return Ok(None);
    };

    let conditions = custody.conditions();

    // The coin id is committed to by the conditions, and therefore the message. Since the coin id
    // depends on the fixed conditions, they can't be made to receive the same message.
    if matches!(&custody, CustodyInfo::P2ConditionsOrSingleton(_))
        && !conditions.contains(&Condition::assert_my_coin_id(spend.coin.coin_id()))
    {
        return Err(DriverError::MissingP2ConditionsOrSingletonAssertion);
    }

    if messages.is_empty() && custody.receives_message() {
        return Err(DriverError::MissingVaultMessage);
    }

    let conditions_hash = if custody.receives_message() {
        let delegated_puzzle = clvm_quote!(conditions).to_clvm(allocator)?;
        Some(tree_hash(allocator, delegated_puzzle))
    } else {
        None
    };

    let run_cat_tail = if matches!(parsed_spend.asset, ParsedAsset::Cat(_)) {
        parse_run_cat_tail(allocator, conditions)?
    } else {
        None
    };

    let mut issuance = if let Some(run_cat_tail) = run_cat_tail {
        let cat_solution = CatSolution::<NodePtr>::from_clvm(allocator, spend.solution)?;

        Some(Issuance {
            coin_id: spend.coin.coin_id(),
            asset_id: run_cat_tail.asset_id,
            extra_delta: cat_solution.extra_delta,
            kind: run_cat_tail.kind,
            verified: false,
        })
    } else {
        None
    };

    // The messages are sent by the vault whose transaction it is (the controlled vault at the end of the authorization
    // path, if there is one), so they can only authorize the extra delta of a TAIL that requires a message from it.
    // Otherwise the coin's p2 puzzle could receive the message instead, and the extra delta would be authorized by
    // another singleton.
    let vault_singleton_struct_hash: Bytes32 = SingletonStruct::new(launcher_id).tree_hash().into();

    let mut tail_matched = false;
    let mut custody_matched = false;

    for message in messages {
        if let Some(hash) = conditions_hash
            && message.data.as_ref() == hash.as_ref()
        {
            if custody_matched {
                return Err(DriverError::DuplicateVaultMessage);
            }

            custody_matched = true;
        } else if let Some(issuance) = &mut issuance
            && let IssuanceKind::EverythingWithSingleton {
                singleton_struct_hash,
                ..
            } = issuance.kind
            && singleton_struct_hash == vault_singleton_struct_hash
            && message.data == get_extra_delta_message(issuance.extra_delta)
        {
            if tail_matched {
                return Err(DriverError::DuplicateVaultMessage);
            }

            tail_matched = true;
            issuance.verified = true;
        } else {
            return Err(DriverError::UnmatchedVaultMessage);
        }
    }

    if custody.receives_message() && !custody_matched {
        return Err(DriverError::WrongConditions);
    }

    if let Some(time) = parsed_spend.required_expiration_time {
        facts.update_required_expiration_time(time);
    }

    record_facts(
        facts,
        conditions,
        parsed_spend.clawback,
        custody.receives_message(),
    );

    let children = parse_children(reveals, allocator, &parsed_spend.asset, spend, conditions)?;

    // Issuances are either:
    // 1. Invalid, in which case the transaction will atomically fail because this spend is verified
    // 2. Valid, in which case the information is correct
    if let Some(issuance) = issuance {
        issuances.push(issuance);
    }

    Ok(Some(VerifiedSpend {
        asset: parsed_spend.asset,
        clawback: parsed_spend.clawback,
        custody,
        children,
        revoked: parsed_spend.revoked,
    }))
}

/// Records what a verified spend's conditions guarantee about the transaction.
///
/// A vault message is received by a coin rather than a puzzle, so another path of a clawback can consume the message
/// without outputting the conditions it commits to. A clawback spend can therefore only be trusted once the time
/// bounds rule out the paths it could be replaced with. The time bounds can't come from clawback spends themselves.
fn record_facts(
    facts: &mut Facts,
    conditions: &[Condition],
    clawback: Option<ClawbackInfo>,
    is_message_bound: bool,
) {
    let is_trusted = clawback.is_none_or(|info| match info.path {
        ClawbackPath::Sender => is_sender_trusted(facts, &info.clawback),
        ClawbackPath::Receiver => is_message_bound && is_expired(facts, &info.clawback),
        ClawbackPath::PushThrough => false,
    });

    if !is_trusted {
        return;
    }

    for condition in conditions {
        match condition {
            Condition::AssertPuzzleAnnouncement(condition) => {
                facts.assert_puzzle_announcement(condition.announcement_id);
            }
            Condition::AssertConcurrentSpend(condition) => {
                facts.assert_spend(condition.coin_id);
            }
            Condition::AssertBeforeSecondsAbsolute(condition) if clawback.is_none() => {
                facts.update_actual_expiration_time(condition.seconds);
            }
            Condition::ReserveFee(condition) => {
                facts.add_reserved_fees(condition.amount);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct AssetFlowTotals {
    asset: ClearSigningAsset,
    input_amount: u128,
    output_amount: u128,
    issued_amount: u128,
    melted_amount: u128,
    received_amount: u128,
    paid_amount: u128,
}

fn build_asset_flows(
    spends: &[VerifiedSpend],
    received_payments: &[AssertedPayment],
    external_payments: &[AssertedPayment],
    drop_coins: &[DropCoin],
    issuances: &[Issuance],
    reserved_fee: u64,
) -> Vec<AssetFlow> {
    let child_coin_ids: HashSet<Bytes32> = spends
        .iter()
        .flat_map(|spend| spend.children.iter())
        .map(|child| child.asset.coin().coin_id())
        .collect();
    let spend_by_coin_id: HashMap<Bytes32, &VerifiedSpend> = spends
        .iter()
        .map(|spend| (spend.asset.coin().coin_id(), spend))
        .collect();
    let xch_child_coin_ids: HashSet<Bytes32> = spends
        .iter()
        .filter(|spend| matches!(spend.asset, ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_)))
        .flat_map(|spend| spend.children.iter())
        .map(|child| child.asset.coin().coin_id())
        .collect();

    let mut flows = IndexMap::<Option<Bytes32>, AssetFlowTotals>::new();

    for spend in spends {
        if !child_coin_ids.contains(&spend.asset.coin().coin_id()) {
            asset_flow_mut(&mut flows, asset_from_parsed(&spend.asset)).input_amount +=
                u128::from(spend.asset.coin().amount);
        }

        for child in &spend.children {
            if !spend_by_coin_id.contains_key(&child.asset.coin().coin_id())
                && child.transfer_type != TransferType::Offered
            {
                asset_flow_mut(&mut flows, asset_from_parsed(&child.asset)).output_amount +=
                    u128::from(child.asset.coin().amount);
            }
        }
    }

    for asserted_payment in received_payments {
        asset_flow_mut(&mut flows, asserted_payment.asset).received_amount +=
            u128::from(asserted_payment.payment.amount);
    }

    for asserted_payment in external_payments {
        asset_flow_mut(&mut flows, asserted_payment.asset).paid_amount +=
            u128::from(asserted_payment.payment.amount);
    }

    for drop_coin in drop_coins {
        asset_flow_mut(&mut flows, ClearSigningAsset::Xch).output_amount +=
            u128::from(drop_coin.amount);
    }

    for spend in spends {
        if matches!(spend.asset, ParsedAsset::Nft(_))
            && xch_child_coin_ids.contains(&spend.asset.coin().parent_coin_info)
        {
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount +=
                u128::from(spend.asset.coin().amount);
        }
    }

    for issuance in issuances {
        let Some(spend) = spend_by_coin_id.get(&issuance.coin_id) else {
            continue;
        };

        let ParsedAsset::Cat(cat) = &spend.asset else {
            continue;
        };

        if cat.info.asset_id != issuance.asset_id {
            continue;
        }

        let cat_asset = asset_from_parsed(&spend.asset);

        if xch_child_coin_ids.contains(&issuance.coin_id) {
            let amount = u128::from(spend.asset.coin().amount);
            asset_flow_mut(&mut flows, cat_asset).issued_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount += amount;
        }

        // The extra delta comes from the solution, so it can't be relied on unless it was verified.
        if !issuance.verified {
            continue;
        }

        // The extra delta comes from an untrusted solution, so it can be `i64::MIN`.
        let amount = u128::from(issuance.extra_delta.unsigned_abs());

        if issuance.extra_delta > 0 {
            asset_flow_mut(&mut flows, cat_asset).issued_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).melted_amount += amount;
        } else if issuance.extra_delta < 0 {
            asset_flow_mut(&mut flows, cat_asset).melted_amount += amount;
            asset_flow_mut(&mut flows, ClearSigningAsset::Xch).issued_amount += amount;
        }
    }

    flows
        .into_values()
        .filter_map(|flow| {
            // Payments and fees may be covered by other parties, so this is floored at zero.
            let unaccounted_amount = (flow.input_amount + flow.issued_amount).saturating_sub(
                flow.output_amount
                    + flow.melted_amount
                    + flow.paid_amount
                    + if matches!(flow.asset, ClearSigningAsset::Xch) {
                        u128::from(reserved_fee)
                    } else {
                        0
                    },
            );

            let include = flow.input_amount > 0
                || flow.output_amount > 0
                || flow.received_amount > 0
                || flow.paid_amount > 0
                || flow.issued_amount > 0
                || flow.melted_amount > 0;

            include.then_some((flow, unaccounted_amount))
        })
        .map(|(flow, unaccounted_amount)| AssetFlow {
            asset: flow.asset,
            input_amount: flow.input_amount,
            output_amount: flow.output_amount,
            issued_amount: flow.issued_amount,
            melted_amount: flow.melted_amount,
            received_amount: flow.received_amount,
            paid_amount: flow.paid_amount,
            unaccounted_amount,
        })
        .collect()
}

fn asset_flow_mut(
    flows: &mut IndexMap<Option<Bytes32>, AssetFlowTotals>,
    asset: ClearSigningAsset,
) -> &mut AssetFlowTotals {
    flows
        .entry(asset_flow_key(asset))
        .or_insert_with(|| AssetFlowTotals {
            asset,
            input_amount: 0,
            output_amount: 0,
            issued_amount: 0,
            melted_amount: 0,
            received_amount: 0,
            paid_amount: 0,
        })
}

fn asset_flow_key(asset: ClearSigningAsset) -> Option<Bytes32> {
    match asset {
        ClearSigningAsset::Xch => None,
        ClearSigningAsset::Cat { asset_id, .. }
        | ClearSigningAsset::Nft {
            launcher_id: asset_id,
            ..
        } => Some(asset_id),
    }
}

fn asset_from_parsed(asset: &ParsedAsset) -> ClearSigningAsset {
    match asset {
        ParsedAsset::Xch(_) | ParsedAsset::Bulletin(_) => ClearSigningAsset::Xch,
        ParsedAsset::Cat(cat) => ClearSigningAsset::Cat {
            asset_id: cat.info.asset_id,
            hidden_puzzle_hash: cat.info.hidden_puzzle_hash,
        },
        ParsedAsset::Nft(nft) => ClearSigningAsset::Nft {
            launcher_id: nft.info.launcher_id,
            metadata: nft.info.metadata,
            metadata_updater_puzzle_hash: nft.info.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: nft.info.royalty_puzzle_hash,
            royalty_basis_points: nft.info.royalty_basis_points,
        },
    }
}

fn calculate_p2_puzzle_hashes(reveals: &Reveals, launcher_id: Bytes32) -> Vec<Bytes32> {
    let mut p2_puzzle_hashes = Vec::new();

    for nonce in reveals.vault_nonces() {
        p2_puzzle_hashes.push(
            mips_puzzle_hash(
                nonce,
                vec![],
                SingletonMember::new(launcher_id).curry_tree_hash(),
                true,
            )
            .into(),
        );
    }

    p2_puzzle_hashes
}

/// The children of the spends that aren't spent again by another spend in the list, which are the coins that remain
/// at the end of the transaction (as far as the verified spends are concerned).
pub fn iter_final_children(spends: &[VerifiedSpend]) -> impl Iterator<Item = &ParsedChild> {
    let spent_coin_ids: HashSet<Bytes32> = spends
        .iter()
        .map(|spend| spend.asset.coin().coin_id())
        .collect();

    spends
        .iter()
        .flat_map(|spend| spend.children.iter())
        .filter(move |child| !spent_coin_ids.contains(&child.asset.coin().coin_id()))
}
