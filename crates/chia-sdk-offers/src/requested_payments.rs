use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::offer::{NotarizedPayment, SettlementPaymentsSolution};
use chia_sdk_driver::{SpendContext, SpendError};
use clvm_traits::ToNodePtr;

pub fn parse_payments(
    ctx: &mut SpendContext<'_>,
    coin_spend: &CoinSpend,
) -> Result<Option<Vec<NotarizedPayment>>, SpendError> {
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

    Ok(Some(settlement_solution.notarized_payments))
}

pub fn payment_coin_spend<P>(
    ctx: &mut SpendContext<'_>,
    puzzle: &P,
    notarized_payments: Vec<NotarizedPayment>,
) -> Result<CoinSpend, SpendError>
where
    P: ToNodePtr,
{
    let puzzle = ctx.alloc(puzzle)?;
    let puzzle_hash = ctx.tree_hash(puzzle).into();
    let puzzle_reveal = ctx.serialize(&puzzle)?;
    let solution = ctx.serialize(&SettlementPaymentsSolution { notarized_payments })?;

    Ok(CoinSpend {
        coin: Coin::new(Bytes32::default(), puzzle_hash, 0),
        puzzle_reveal,
        solution,
    })
}
