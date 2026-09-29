use chia_protocol::Bytes32;
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment, Payment},
};
use hex_literal::hex;

use crate::{
    CreateDidAction, Delta, Deltas, DriverError, FeeAction, HashedPtr, Id, IssueCatAction,
    MeltSingletonAction, MintNftAction, MintOptionAction, OptionType, RunTailAction, SendAction,
    SettleAction, Spend, SpendContext, Spends, TailIssuance, TransferNftById, UpdateDidAction,
    UpdateNftAction,
};

/// The puzzle hash that coins are sent to by [`Action::burn`]. There's no known puzzle with this
/// hash, so the coins can never be spent.
pub const BURN_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "000000000000000000000000000000000000000000000000000000000000dead"
));

/// A high level operation to include in a transaction. Use the constructors on this type to create
/// them, and [`Spends::apply`] to apply them.
#[derive(Debug, Clone)]
pub enum Action {
    /// See [`Action::send`].
    Send(SendAction),
    /// See [`Action::settle`].
    Settle(SettleAction),
    /// See [`Action::create_did`].
    CreateDid(CreateDidAction),
    /// See [`Action::update_did`].
    UpdateDid(UpdateDidAction),
    /// See [`Action::mint_nft`].
    MintNft(MintNftAction),
    /// See [`Action::update_nft`].
    UpdateNft(UpdateNftAction),
    /// See [`Action::issue_cat`].
    IssueCat(IssueCatAction),
    /// See [`Action::run_tail`].
    RunTail(RunTailAction),
    /// See [`Action::mint_option`].
    MintOption(MintOptionAction),
    /// See [`Action::melt_singleton`].
    MeltSingleton(MeltSingletonAction),
    /// See [`Action::fee`].
    Fee(FeeAction),
}

impl Action {
    /// Creates a coin of the asset with the puzzle hash and amount. The memos are used as given, so
    /// payments of CATs and singletons should be hinted with the puzzle hash for wallets to find them.
    ///
    /// For a singleton, this sets where it's sent, and the amount must be the singleton's amount.
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(id, puzzle_hash, amount, memos))
    }

    /// Makes a notarized payment from a settlement coin of the asset, such as a payment requested by
    /// an offer being taken.
    pub fn settle(id: Id, notarized_payment: NotarizedPayment) -> Self {
        Self::Settle(SettleAction::new(id, notarized_payment))
    }

    /// Pays an NFT royalty from a settlement coin of the asset, with the NFT's launcher id as the nonce.
    pub fn settle_royalty(
        ctx: &mut SpendContext,
        id: Id,
        launcher_id: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_amount: u64,
    ) -> Result<Self, DriverError> {
        let hint = ctx.hint(royalty_puzzle_hash)?;

        Ok(Self::settle(
            id,
            NotarizedPayment::new(
                launcher_id,
                vec![Payment::new(royalty_puzzle_hash, royalty_amount, hint)],
            ),
        ))
    }

    /// Sends the amount to [`BURN_PUZZLE_HASH`], so that it can never be spent.
    pub fn burn(id: Id, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(id, BURN_PUZZLE_HASH, amount, memos))
    }

    /// Creates a DID from an XCH coin, with the given amount. Unless it's sent elsewhere, the DID is
    /// sent to the change puzzle hash.
    pub fn create_did(
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
        amount: u64,
    ) -> Self {
        Self::CreateDid(CreateDidAction::new(
            recovery_list_hash,
            num_verifications_required,
            metadata,
            amount,
        ))
    }

    /// Creates a DID with no recovery list, empty metadata, and an amount of 1.
    pub fn create_empty_did() -> Self {
        Self::CreateDid(CreateDidAction::default())
    }

    /// Updates the DID's recovery list hash (`Some(None)` removes it), number of verifications
    /// required, or metadata. Fields that are `None` are left unchanged.
    pub fn update_did(
        id: Id,
        new_recovery_list_hash: Option<Option<Bytes32>>,
        new_num_verifications_required: Option<u64>,
        new_metadata: Option<HashedPtr>,
    ) -> Self {
        Self::UpdateDid(UpdateDidAction::new(
            id,
            new_recovery_list_hash,
            new_num_verifications_required,
            new_metadata,
        ))
    }

    /// Mints an NFT from an XCH coin, with the given amount. Unless it's sent elsewhere, the NFT is
    /// sent to the change puzzle hash.
    pub fn mint_nft(
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        amount: u64,
    ) -> Self {
        Self::MintNft(MintNftAction::new(
            Id::Xch,
            metadata,
            metadata_updater_puzzle_hash,
            royalty_puzzle_hash,
            royalty_basis_points,
            amount,
        ))
    }

    /// Mints an NFT with the DID as the launcher's parent. This does not assign the NFT to the DID;
    /// follow it with [`Action::update_nft`] and a [`TransferNftById`] to do so.
    pub fn mint_nft_from_did(
        parent_did_id: Id,
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        amount: u64,
    ) -> Self {
        Self::MintNft(MintNftAction::new(
            parent_did_id,
            metadata,
            metadata_updater_puzzle_hash,
            royalty_puzzle_hash,
            royalty_basis_points,
            amount,
        ))
    }

    /// Mints an NFT with empty metadata, no royalty, and an amount of 1.
    pub fn mint_empty_nft() -> Self {
        Self::mint_nft(HashedPtr::NIL, Bytes32::default(), Bytes32::default(), 0, 1)
    }

    /// Like [`Action::mint_empty_nft`], but with the DID as the launcher's parent.
    pub fn mint_empty_nft_from_did(parent_did_id: Id) -> Self {
        Self::mint_nft_from_did(
            parent_did_id,
            HashedPtr::NIL,
            Bytes32::default(),
            Bytes32::default(),
            0,
            1,
        )
    }

    /// Like [`Action::mint_empty_nft`], but with a royalty.
    pub fn mint_empty_royalty_nft(royalty_puzzle_hash: Bytes32, royalty_basis_points: u16) -> Self {
        Self::mint_nft(
            HashedPtr::NIL,
            Bytes32::default(),
            royalty_puzzle_hash,
            royalty_basis_points,
            1,
        )
    }

    /// Like [`Action::mint_empty_royalty_nft`], but with the DID as the launcher's parent.
    pub fn mint_empty_royalty_nft_from_did(
        parent_did_id: Id,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
    ) -> Self {
        Self::mint_nft_from_did(
            parent_did_id,
            HashedPtr::NIL,
            Bytes32::default(),
            royalty_puzzle_hash,
            royalty_basis_points,
            1,
        )
    }

    /// Updates the NFT's metadata and/or transfers it to a DID (or removes it from a DID). The
    /// metadata update spends are run in order, each against the result of the previous one.
    pub fn update_nft(
        id: Id,
        metadata_update_spends: Vec<Spend>,
        transfer: Option<TransferNftById>,
    ) -> Self {
        Self::UpdateNft(UpdateNftAction::new(id, metadata_update_spends, transfer))
    }

    /// Issues a CAT with the given TAIL. The CAT can be referred to as [`Id::New`] with the index of
    /// this action, unless coins of the same asset id are already in the [`Spends`]. In that case
    /// the issued coin joins their ring, and must be referred to as [`Id::Existing`] instead.
    pub fn issue_cat(tail_spend: Spend, hidden_puzzle_hash: Option<Bytes32>, amount: u64) -> Self {
        Self::IssueCat(IssueCatAction::new(
            TailIssuance::Multiple(tail_spend),
            hidden_puzzle_hash,
            amount,
        ))
    }

    /// Issues a CAT with a single issuance TAIL, which is derived from the coin that issues it, so
    /// no more of the CAT can ever be issued.
    pub fn single_issue_cat(hidden_puzzle_hash: Option<Bytes32>, amount: u64) -> Self {
        Self::IssueCat(IssueCatAction::new(
            TailIssuance::Single,
            hidden_puzzle_hash,
            amount,
        ))
    }

    /// Runs the TAIL of an existing CAT, to issue more of it (`supply_delta.input`) or melt some of
    /// it into XCH (`supply_delta.output`).
    pub fn run_tail(id: Id, tail_spend: Spend, supply_delta: Delta) -> Self {
        Self::RunTail(RunTailAction::new(id, tail_spend, supply_delta))
    }

    /// Mints an option contract from an XCH coin, and locks `underlying_amount` of the underlying
    /// asset in a coin that the option controls. Before the `seconds` timestamp, the owner can
    /// exercise the option by paying the strike to the creator. After it, the creator can take the
    /// underlying asset back.
    pub fn mint_option(
        creator_puzzle_hash: Bytes32,
        seconds: u64,
        underlying_id: Id,
        underlying_amount: u64,
        strike_type: OptionType,
        amount: u64,
    ) -> Self {
        Self::MintOption(MintOptionAction::new(
            creator_puzzle_hash,
            seconds,
            underlying_id,
            underlying_amount,
            strike_type,
            amount,
        ))
    }

    /// Melts a DID or option contract. The amount must be the singleton's amount, which is returned
    /// to the transaction as XCH.
    ///
    /// Melting an option authorizes its underlying coin to be exercised, but spending that coin and
    /// paying the strike to the creator must be done separately.
    pub fn melt_singleton(id: Id, amount: u64) -> Self {
        Self::MeltSingleton(MeltSingletonAction::new(id, amount))
    }

    /// Pays a fee to the farmer, which is asserted with a `RESERVE_FEE` condition.
    pub fn fee(amount: u64) -> Self {
        Self::Fee(FeeAction::new(amount))
    }
}

/// An operation that can be applied to [`Spends`]. The index is the position of the action in the
/// list, which is how [`Id::New`] refers to the assets it creates.
pub trait SpendAction {
    /// Adds the amounts that the action adds to and removes from the transaction to the deltas.
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize);

    /// Applies the action to the spends.
    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError>;
}

impl SpendAction for Action {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        match self {
            Action::Send(action) => action.calculate_delta(deltas, index),
            Action::Settle(action) => action.calculate_delta(deltas, index),
            Action::CreateDid(action) => action.calculate_delta(deltas, index),
            Action::UpdateDid(action) => action.calculate_delta(deltas, index),
            Action::MintNft(action) => action.calculate_delta(deltas, index),
            Action::UpdateNft(action) => action.calculate_delta(deltas, index),
            Action::IssueCat(action) => action.calculate_delta(deltas, index),
            Action::RunTail(action) => action.calculate_delta(deltas, index),
            Action::MintOption(action) => action.calculate_delta(deltas, index),
            Action::MeltSingleton(action) => action.calculate_delta(deltas, index),
            Action::Fee(action) => action.calculate_delta(deltas, index),
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        match self {
            Action::Send(action) => action.spend(ctx, spends, index),
            Action::Settle(action) => action.spend(ctx, spends, index),
            Action::CreateDid(action) => action.spend(ctx, spends, index),
            Action::UpdateDid(action) => action.spend(ctx, spends, index),
            Action::MintNft(action) => action.spend(ctx, spends, index),
            Action::UpdateNft(action) => action.spend(ctx, spends, index),
            Action::IssueCat(action) => action.spend(ctx, spends, index),
            Action::RunTail(action) => action.spend(ctx, spends, index),
            Action::MintOption(action) => action.spend(ctx, spends, index),
            Action::MeltSingleton(action) => action.spend(ctx, spends, index),
            Action::Fee(action) => action.spend(ctx, spends, index),
        }
    }
}
