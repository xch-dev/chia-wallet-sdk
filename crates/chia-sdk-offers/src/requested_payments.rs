use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::offer::{NotarizedPayment, SettlementPaymentsSolution};
use chia_sdk_driver::{SpendContext, SpendError};

#[derive(Debug, Clone)]
pub struct RequestedPayments {
    puzzle_hash: Bytes32,
    puzzle_reveal: Program,
    notarized_payments: Vec<NotarizedPayment>,
}

impl RequestedPayments {
    pub fn new(
        puzzle_hash: Bytes32,
        puzzle_reveal: Program,
        notarized_payments: Vec<NotarizedPayment>,
    ) -> Self {
        Self {
            puzzle_hash,
            puzzle_reveal,
            notarized_payments,
        }
    }

    pub fn from_coin_spend(
        ctx: &mut SpendContext<'_>,
        coin_spend: CoinSpend,
    ) -> Result<Option<Self>, SpendError> {
        if coin_spend.coin.parent_coin_info != Bytes32::default() {
            return Ok(None);
        }

        if coin_spend.coin.amount != 0 {
            return Ok(None);
        }

        let puzzle = ctx.alloc(&coin_spend.puzzle_reveal)?;
        let puzzle_hash = Bytes32::from(ctx.tree_hash(puzzle));
        if puzzle_hash != coin_spend.coin.puzzle_hash {
            return Ok(None);
        }

        let solution = ctx.alloc(&coin_spend.solution)?;
        let settlement_solution = ctx.extract::<SettlementPaymentsSolution>(solution)?;

        Ok(Some(Self {
            puzzle_hash,
            puzzle_reveal: coin_spend.puzzle_reveal,
            notarized_payments: settlement_solution.notarized_payments,
        }))
    }

    pub fn into_coin_spend(self, ctx: &mut SpendContext<'_>) -> Result<CoinSpend, SpendError> {
        let solution = ctx.serialize(&SettlementPaymentsSolution {
            notarized_payments: self.notarized_payments,
        })?;

        Ok(CoinSpend {
            coin: Coin::new(Bytes32::default(), self.puzzle_hash, 0),
            puzzle_reveal: self.puzzle_reveal,
            solution,
        })
    }

    pub fn extend(&mut self, notarized_payments: impl IntoIterator<Item = NotarizedPayment>) {
        self.notarized_payments.extend(notarized_payments);
    }

    pub fn puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    pub fn puzzle_reveal(&self) -> &Program {
        &self.puzzle_reveal
    }

    pub fn notarized_payments(&self) -> &[NotarizedPayment] {
        &self.notarized_payments
    }
}
