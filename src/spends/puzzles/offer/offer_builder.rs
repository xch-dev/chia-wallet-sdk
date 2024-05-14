use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    cat::{CatArgs, CAT_PUZZLE_HASH},
    offer::SETTLEMENT_PAYMENTS_PUZZLE_HASH,
};
use clvm_utils::{tree_hash_atom, tree_hash_pair, CurriedProgram, ToTreeHash};
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{
    AssertPuzzleAnnouncement, ChainedSpend, NotarizedPayment, Payment, SettlementPaymentsSolution,
    SpendContext, SpendError,
};

pub struct OfferBuilder {
    nonce: Bytes32,
    coin_spends: Vec<CoinSpend>,
    parent_conditions: Vec<NodePtr>,
}

impl OfferBuilder {
    pub fn new(offered_coin_ids: Vec<Bytes32>) -> Self {
        let nonce = calculate_nonce(offered_coin_ids);
        Self::from_nonce(nonce)
    }

    pub fn from_nonce(nonce: Bytes32) -> Self {
        Self {
            nonce,
            coin_spends: Vec::new(),
            parent_conditions: Vec::new(),
        }
    }

    pub fn request_xch_payments(
        self,
        ctx: &mut SpendContext,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError> {
        let puzzle = ctx.standard_puzzle();
        self.request_payments(ctx, puzzle, payments)
    }

    pub fn request_cat_payments(
        self,
        ctx: &mut SpendContext,
        asset_id: Bytes32,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError> {
        let puzzle_hash = CurriedProgram {
            program: CAT_PUZZLE_HASH,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                tail_program_hash: asset_id,
                inner_puzzle: SETTLEMENT_PAYMENTS_PUZZLE_HASH,
            },
        }
        .tree_hash();

        let puzzle = if let Some(puzzle) = ctx.get_puzzle(&puzzle_hash) {
            puzzle
        } else {
            let cat_puzzle = ctx.cat_puzzle();
            let settlement_payments_puzzle = ctx.settlement_payments_puzzle();
            let puzzle = ctx.alloc(CurriedProgram {
                program: cat_puzzle,
                args: CatArgs {
                    mod_hash: CAT_PUZZLE_HASH.into(),
                    tail_program_hash: asset_id,
                    inner_puzzle: settlement_payments_puzzle,
                },
            })?;
            ctx.preload(puzzle_hash, puzzle);
            puzzle
        };

        self.request_payments(ctx, puzzle, payments)
    }

    pub fn request_payments(
        mut self,
        ctx: &mut SpendContext,
        puzzle: NodePtr,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError> {
        let (coin_spend, announcement_id) =
            request_offer_payments(ctx, self.nonce, puzzle, payments)?;

        self.coin_spends.push(coin_spend);
        self.parent_conditions
            .push(ctx.alloc(AssertPuzzleAnnouncement { announcement_id })?);

        Ok(self)
    }

    pub fn finish(self, ctx: &mut SpendContext) -> ChainedSpend {
        for coin_spend in self.coin_spends {
            ctx.spend(coin_spend);
        }

        ChainedSpend {
            parent_conditions: self.parent_conditions,
        }
    }
}

pub fn calculate_nonce(offered_coin_ids: Vec<Bytes32>) -> Bytes32 {
    let mut coin_ids = offered_coin_ids;
    coin_ids.sort();

    let mut tree_hash = tree_hash_atom(&[]);

    for coin_id in coin_ids.into_iter().rev() {
        let item_hash = tree_hash_atom(&coin_id);
        tree_hash = tree_hash_pair(item_hash, tree_hash);
    }

    tree_hash.into()
}

pub fn request_offer_payments(
    ctx: &mut SpendContext,
    nonce: Bytes32,
    puzzle: NodePtr,
    payments: Vec<Payment>,
) -> Result<(CoinSpend, Bytes32), SpendError> {
    let puzzle_reveal = ctx.serialize(puzzle)?;
    let puzzle_hash = ctx.tree_hash(puzzle).into();

    let notarized_payment = NotarizedPayment { nonce, payments };

    let settlement_solution = ctx.serialize(SettlementPaymentsSolution {
        notarized_payments: vec![notarized_payment.clone()],
    })?;

    let coin_spend = CoinSpend::new(
        Coin::new(Bytes32::default(), puzzle_hash, 0),
        puzzle_reveal,
        settlement_solution,
    );

    let notarized_payment_ptr = ctx.alloc(notarized_payment)?;
    let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);

    let mut hasher = Sha256::new();
    hasher.update(puzzle_hash);
    hasher.update(notarized_payment_hash);
    let puzzle_announcement_id = Bytes32::new(hasher.finalize_fixed().into());

    Ok((coin_spend, puzzle_announcement_id))
}

pub fn offer_announcement_id(
    ctx: &mut SpendContext,
    puzzle_hash: Bytes32,
    notarized_payment: NotarizedPayment,
) -> Result<Bytes32, SpendError> {
    let notarized_payment = ctx.alloc(notarized_payment)?;
    let notarized_payment_hash = ctx.tree_hash(notarized_payment);

    let mut hasher = Sha256::new();
    hasher.update(puzzle_hash);
    hasher.update(notarized_payment_hash);
    Ok(Bytes32::new(hasher.finalize_fixed().into()))
}
