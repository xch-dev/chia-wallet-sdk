use chia_protocol::Bytes32;
use chia_puzzle_types::{
    offer::{NotarizedPayment, Payment},
    Memos,
};
use hex_literal::hex;

use crate::{
    CreateDidAction, Delta, Deltas, DriverError, FeeAction, HashedPtr, Id, IssueCatAction,
    MeltSingletonAction, MintNftAction, MintOptionAction, OptionType, RunTailAction, SendAction,
    SettleAction, Spend, SpendContext, Spends, TailIssuance, TransferNftById, UpdateDidAction,
    UpdateNftAction,
};

pub const BURN_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "000000000000000000000000000000000000000000000000000000000000dead"
));

#[derive(Debug, Clone)]
pub enum Action {
    Send(SendAction),
    Settle(SettleAction),
    CreateDid(CreateDidAction),
    UpdateDid(UpdateDidAction),
    MintNft(MintNftAction),
    UpdateNft(UpdateNftAction),
    IssueCat(IssueCatAction),
    RunTail(RunTailAction),
    MintOption(MintOptionAction),
    MeltSingleton(MeltSingletonAction),
    Fee(FeeAction),
}

impl Action {
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(id, puzzle_hash, amount, memos))
    }

    pub fn settle(id: Id, notarized_payment: NotarizedPayment) -> Self {
        Self::Settle(SettleAction::new(id, notarized_payment))
    }

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

    pub fn burn(id: Id, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(id, BURN_PUZZLE_HASH, amount, memos))
    }

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

    pub fn create_empty_did() -> Self {
        Self::CreateDid(CreateDidAction::default())
    }

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

    pub fn mint_empty_nft() -> Self {
        Self::mint_nft(HashedPtr::NIL, Bytes32::default(), Bytes32::default(), 0, 1)
    }

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

    pub fn mint_empty_royalty_nft(royalty_puzzle_hash: Bytes32, royalty_basis_points: u16) -> Self {
        Self::mint_nft(
            HashedPtr::NIL,
            Bytes32::default(),
            royalty_puzzle_hash,
            royalty_basis_points,
            1,
        )
    }

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

    pub fn update_nft(
        id: Id,
        metadata_update_spends: Vec<Spend>,
        transfer: Option<TransferNftById>,
    ) -> Self {
        Self::UpdateNft(UpdateNftAction::new(id, metadata_update_spends, transfer))
    }

    pub fn issue_cat(tail_spend: Spend, hidden_puzzle_hash: Option<Bytes32>, amount: u64) -> Self {
        Self::IssueCat(IssueCatAction::new(
            TailIssuance::Multiple(tail_spend),
            hidden_puzzle_hash,
            amount,
        ))
    }

    pub fn single_issue_cat(hidden_puzzle_hash: Option<Bytes32>, amount: u64) -> Self {
        Self::IssueCat(IssueCatAction::new(
            TailIssuance::Single,
            hidden_puzzle_hash,
            amount,
        ))
    }

    pub fn run_tail(id: Id, tail_spend: Spend, supply_delta: Delta) -> Self {
        Self::RunTail(RunTailAction::new(id, tail_spend, supply_delta))
    }

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

    pub fn melt_singleton(id: Id, amount: u64) -> Self {
        Self::MeltSingleton(MeltSingletonAction::new(id, amount))
    }

    pub fn fee(amount: u64) -> Self {
        Self::Fee(FeeAction::new(amount))
    }
}

pub trait SpendAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize);

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
