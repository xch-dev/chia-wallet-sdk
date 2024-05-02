use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_wallet::{
    cat::{cat_puzzle_hash, CatArgs, CAT_PUZZLE_HASH},
    offer::SETTLEMENT_PAYMENTS_PUZZLE_HASH,
};
use clvm_utils::{tree_hash_atom, tree_hash_pair, CurriedProgram};
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{
    AssertPuzzleAnnouncement, NotarizedPayment, Payment, SettlementPaymentsSolution, SpendContext,
    SpendError,
};

pub struct OfferRequests {
    pub coin_spends: Vec<CoinSpend>,
    pub assertions: Vec<AssertPuzzleAnnouncement>,
}

pub struct OfferBuilder<'a, 'b> {
    ctx: &'a mut SpendContext<'b>,
    nonce: Bytes32,
    coin_spends: Vec<CoinSpend>,
    assertions: Vec<AssertPuzzleAnnouncement>,
}

impl<'a, 'b> OfferBuilder<'a, 'b> {
    pub fn new(ctx: &'a mut SpendContext<'b>, offered_coin_ids: Vec<Bytes32>) -> Self {
        let nonce = calculate_nonce(offered_coin_ids);
        Self::from_nonce(ctx, nonce)
    }

    pub fn from_nonce(ctx: &'a mut SpendContext<'b>, nonce: Bytes32) -> Self {
        Self {
            ctx,
            nonce,
            coin_spends: Vec::new(),
            assertions: Vec::new(),
        }
    }

    pub fn request_xch_payments(self, payments: Vec<Payment>) -> Result<Self, SpendError> {
        let puzzle = self.ctx.standard_puzzle();
        self.request_payments(puzzle, payments)
    }

    pub fn request_cat_payments(
        self,
        asset_id: Bytes32,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError> {
        let puzzle_hash = cat_puzzle_hash(asset_id.into(), SETTLEMENT_PAYMENTS_PUZZLE_HASH);

        let puzzle = if let Some(puzzle) = self.ctx.get_puzzle(&puzzle_hash) {
            puzzle
        } else {
            let cat_puzzle = self.ctx.cat_puzzle();
            let settlement_payments_puzzle = self.ctx.settlement_payments_puzzle();
            let puzzle = self.ctx.alloc(CurriedProgram {
                program: cat_puzzle,
                args: CatArgs {
                    mod_hash: CAT_PUZZLE_HASH.into(),
                    tail_program_hash: asset_id,
                    inner_puzzle: settlement_payments_puzzle,
                },
            })?;
            self.ctx.preload(puzzle_hash, puzzle);
            puzzle
        };

        self.request_payments(puzzle, payments)
    }

    pub fn request_payments(
        mut self,
        puzzle: NodePtr,
        payments: Vec<Payment>,
    ) -> Result<Self, SpendError> {
        let (coin_spend, announcement_id) =
            request_offer_payments(self.ctx, self.nonce, puzzle, payments)?;

        self.coin_spends.push(coin_spend);
        self.assertions
            .push(AssertPuzzleAnnouncement { announcement_id });

        Ok(self)
    }

    pub fn finish(self) -> OfferRequests {
        OfferRequests {
            coin_spends: self.coin_spends,
            assertions: self.assertions,
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
    let puzzle_hash = ctx.tree_hash(puzzle);

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
