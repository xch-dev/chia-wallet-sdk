use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use hex_literal::hex;

use crate::{
    CreateDidAction, Deltas, DriverError, HashedPtr, Id, IssueCatAction, MeltCatAction,
    MintNftAction, SendAction, Spend, SpendContext, Spends, TailIssuance, TransferNftById,
    UpdateDidAction, UpdateNftAction,
};

pub const BURN_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "000000000000000000000000000000000000000000000000000000000000dead"
));

#[derive(Debug, Clone)]
pub enum Action {
    Send(SendAction),
    CreateDid(CreateDidAction),
    UpdateDid(UpdateDidAction),
    MintNft(MintNftAction),
    UpdateNft(UpdateNftAction),
    IssueCat(IssueCatAction),
    MeltCat(MeltCatAction),
}

impl Action {
    pub fn send(id: Id, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(Some(id), puzzle_hash, amount, memos))
    }

    pub fn send_xch(puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(None, puzzle_hash, amount, memos))
    }

    pub fn burn(id: Id, amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(Some(id), BURN_PUZZLE_HASH, amount, memos))
    }

    pub fn burn_xch(amount: u64, memos: Memos) -> Self {
        Self::Send(SendAction::new(None, BURN_PUZZLE_HASH, amount, memos))
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
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
    ) -> Self {
        Self::UpdateDid(UpdateDidAction::new(
            id,
            recovery_list_hash,
            num_verifications_required,
            metadata,
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
            None,
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
            Some(parent_did_id),
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

    pub fn update_nft(
        id: Id,
        metadata_update_spends: Vec<Spend>,
        transfer: Option<TransferNftById>,
    ) -> Self {
        Self::UpdateNft(UpdateNftAction::new(id, metadata_update_spends, transfer))
    }

    pub fn issue_cat(tail_spend: Spend, amount: u64) -> Self {
        Self::IssueCat(IssueCatAction::new(
            TailIssuance::Multiple(tail_spend),
            amount,
        ))
    }

    pub fn single_issue_cat(amount: u64) -> Self {
        Self::IssueCat(IssueCatAction::new(TailIssuance::Single, amount))
    }

    pub fn melt_cat(id: Id, tail_spend: Spend, amount: u64) -> Self {
        Self::MeltCat(MeltCatAction::new(id, tail_spend, amount))
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
            Action::CreateDid(action) => action.calculate_delta(deltas, index),
            Action::UpdateDid(action) => action.calculate_delta(deltas, index),
            Action::MintNft(action) => action.calculate_delta(deltas, index),
            Action::UpdateNft(action) => action.calculate_delta(deltas, index),
            Action::IssueCat(action) => action.calculate_delta(deltas, index),
            Action::MeltCat(action) => action.calculate_delta(deltas, index),
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
            Action::CreateDid(action) => action.spend(ctx, spends, index),
            Action::UpdateDid(action) => action.spend(ctx, spends, index),
            Action::MintNft(action) => action.spend(ctx, spends, index),
            Action::UpdateNft(action) => action.spend(ctx, spends, index),
            Action::IssueCat(action) => action.spend(ctx, spends, index),
            Action::MeltCat(action) => action.spend(ctx, spends, index),
        }
    }
}
