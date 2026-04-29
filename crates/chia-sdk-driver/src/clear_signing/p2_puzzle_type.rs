use chia_protocol::Bytes32;
use chia_sdk_types::Condition;

/// Classification of where a child coin's value is going, based on its inner p2 puzzle hash.
///
/// Used to give the user a high-level idea of what each child coin represents, without making
/// them inspect raw puzzle hashes. For example, an offer pre-split coin shows up as a child going
/// to a `P2ConditionsOrSingleton` puzzle, but at the user's level the more meaningful description
/// is "this is locked into the offer that will be created if the offer is taken later."
#[derive(Debug, Clone)]
pub enum P2PuzzleType {
    /// The inner p2 puzzle hash is the settlement puzzle hash. The coin is being paid to a
    /// settlement layer in *this* transaction (e.g. taking an existing offer).
    Offered,
    /// The inner p2 puzzle hash is the canonical burn puzzle hash. The coin cannot be spent.
    Burned,
    /// The inner p2 puzzle hash resolves to a `P2ConditionsOrSingleton` revealed in this
    /// transaction. The coin becomes a pre-split coin for the offer described in
    /// [`VaultTransaction::linked_offer`](crate::VaultTransaction::linked_offer). It can still be
    /// cancelled by the vault via the singleton path of the puzzle, so the user is not locked in.
    OfferPreSplit(OfferPreSplitInfo),
    /// Anything else — a regular send to some other puzzle hash, an unrecognized custom puzzle, or
    /// a clawback-wrapped output that doesn't fall into one of the above categories.
    Unknown,
}

/// Details of an offer pre-split coin's `P2ConditionsOrSingleton` puzzle.
///
/// The vault is *not* signing for the future spend of this coin — only for its creation. The
/// trust comes from the puzzle itself: because it's a `P2ConditionsOrSingleton` curried with this
/// vault's launcher id, the vault can always cancel it via the singleton path. We surface the
/// fixed conditions so the UI can describe what *would* happen if a taker spends this coin via
/// the fixed path.
#[derive(Debug, Clone)]
pub struct OfferPreSplitInfo {
    /// Singleton (vault) launcher id allowed to cancel/redirect this coin.
    pub launcher_id: Bytes32,
    /// Vault nonce for the singleton path of the `P2ConditionsOrSingleton`.
    pub nonce: usize,
    /// Tree hash of `clvm_quote!(fixed_conditions)`. Pinned into the curried puzzle hash.
    pub fixed_delegated_puzzle_hash: Bytes32,
    /// Raw fixed conditions. Surfaced as-is so the UI can describe the future spend without
    /// committing to a specific format.
    pub fixed_conditions: Vec<Condition>,
    /// Sum of `CreateCoin` amounts in `fixed_conditions` whose puzzle hash is the settlement
    /// puzzle hash — i.e. the value that pays into the settlement layer when the offer is taken.
    /// For CAT pre-splits this is also computed against the inner (cat-unwrapped) puzzle hash.
    pub settlement_amount: u64,
}
