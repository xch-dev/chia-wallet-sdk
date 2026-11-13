use crate::{DriverError, InternalCustodyError, SpendContext};
use chia_bls::PublicKey;
use chia_protocol::CoinSpend;
use chia_puzzle_types::DeriveSynthetic;
use chia_sdk_signer::{AggSigConstants, RequiredBlsSignature};
use chia_sdk_types::{
    Condition,
    conditions::{AggSig, AggSigKind},
};
use clvm_traits::FromClvm;
use clvmr::{
    ChiaDialect, ClvmFlags, NodePtr, run_program,
    serde::{node_from_bytes, node_to_bytes},
};

const MAX_PROGRAM_BYTES: usize = 256 * 1024;

// Cross-check the emitted authorization against the selected roster.
// Exact reconstruction fixes the delegated program; execution must still emit the
// corresponding standard or lock signature form.
pub(super) enum ExpectedAuthorization {
    Unsigned,
    Standard(PublicKey),
    Lock(PublicKey),
}
impl ExpectedAuthorization {
    fn matches(&self, key: PublicKey, message: &[u8]) -> bool {
        match self {
            Self::Unsigned => false,
            Self::Standard(aggregate) => key == aggregate.derive_synthetic() && message.len() == 32,
            Self::Lock(aggregate) => key == *aggregate && message.is_empty(),
        }
    }
    fn is_signed(&self) -> bool {
        !matches!(self, Self::Unsigned)
    }
}

pub(super) struct Execution {
    pub signature: Option<RequiredBlsSignature>,
    pub relative_seconds: Vec<u64>,
}

pub(super) fn execute(
    ctx: &mut SpendContext,
    spends: &[CoinSpend],
    decoded: &[(NodePtr, NodePtr)],
    authorization: &ExpectedAuthorization,
    constants: &AggSigConstants,
    max_cost: u64,
) -> Result<Execution, DriverError> {
    let mut remaining = max_cost;
    let mut signature = None;
    let mut coin_announcements = std::collections::HashSet::new();
    let mut puzzle_announcements = std::collections::HashSet::new();
    let mut coin_assertions = Vec::new();
    let mut puzzle_assertions = Vec::new();
    let mut relative_seconds = Vec::with_capacity(spends.len());
    for (spend, &(puzzle, solution)) in spends.iter().zip(decoded) {
        let mut delay = 0;
        let output = run_budgeted(ctx, puzzle, solution, &mut remaining)?;
        for condition in Vec::<Condition>::from_clvm(&**ctx, output)? {
            match condition {
                Condition::AssertSecondsRelative(c) => {
                    delay = delay.max(c.seconds);
                }
                Condition::AggSigMe(c)
                    if signature.is_none()
                        && authorization.matches(c.public_key, c.message.as_ref()) =>
                {
                    signature = Some(RequiredBlsSignature::try_from_condition(
                        &spend.coin,
                        AggSig {
                            kind: AggSigKind::Me,
                            public_key: c.public_key,
                            message: c.message,
                        },
                        constants,
                    )?);
                }
                // Other can also hold unrepresentable known conditions, such as
                // a relative delay larger than u64. Those must fail closed too.
                Condition::AggSigMe(_)
                | Condition::AggSigUnsafe(_)
                | Condition::AggSigParent(_)
                | Condition::AggSigPuzzle(_)
                | Condition::AggSigAmount(_)
                | Condition::AggSigPuzzleAmount(_)
                | Condition::AggSigParentAmount(_)
                | Condition::AggSigParentPuzzle(_)
                | Condition::Other(_) => {
                    return Err(InternalCustodyError::UnexpectedConditions.into());
                }
                Condition::AssertMyParentId(c) if c.parent_id != spend.coin.parent_coin_info => {
                    return Err(InternalCustodyError::CoinAssertion.into());
                }
                Condition::AssertMyAmount(c) if c.amount != spend.coin.amount => {
                    return Err(InternalCustodyError::CoinAssertion.into());
                }
                Condition::AssertMyPuzzleHash(c) if c.puzzle_hash != spend.coin.puzzle_hash => {
                    return Err(InternalCustodyError::CoinAssertion.into());
                }
                Condition::CreateCoinAnnouncement(c) => {
                    coin_announcements.insert(chia_sdk_types::announcement_id(
                        spend.coin.coin_id(),
                        c.message,
                    ));
                }
                Condition::CreatePuzzleAnnouncement(c) => {
                    puzzle_announcements.insert(chia_sdk_types::announcement_id(
                        spend.coin.puzzle_hash,
                        c.message,
                    ));
                }
                Condition::AssertCoinAnnouncement(c) => coin_assertions.push(c.announcement_id),
                Condition::AssertPuzzleAnnouncement(c) => puzzle_assertions.push(c.announcement_id),
                _ => (), // Canonical exact reconstruction fixes all remaining effects/conditions.
            }
        }
        relative_seconds.push(delay);
    }
    if coin_assertions
        .iter()
        .any(|id| !coin_announcements.contains(id))
        || puzzle_assertions
            .iter()
            .any(|id| !puzzle_announcements.contains(id))
    {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    if signature.is_some() != authorization.is_signed() {
        return Err(InternalCustodyError::UnexpectedConditions.into());
    }
    Ok(Execution {
        signature,
        relative_seconds,
    })
}

// CLVM interprets zero as unlimited, so exhaustion must fail before execution.
fn run_budgeted(
    ctx: &mut SpendContext,
    puzzle: NodePtr,
    solution: NodePtr,
    remaining: &mut u64,
) -> Result<NodePtr, DriverError> {
    if *remaining == 0 {
        return Err(InternalCustodyError::InvalidProgram.into());
    }
    let reduction = run_program(
        ctx,
        &ChiaDialect::new(ClvmFlags::empty()),
        puzzle,
        solution,
        *remaining,
    )?;
    *remaining = remaining
        .checked_sub(reduction.0)
        .ok_or(InternalCustodyError::InvalidProgram)?;
    Ok(reduction.1)
}

pub(super) fn canonical_program(
    ctx: &mut SpendContext,
    bytes: &[u8],
) -> Result<NodePtr, DriverError> {
    if bytes.is_empty() || bytes.len() > MAX_PROGRAM_BYTES {
        return Err(InternalCustodyError::InvalidProgram.into());
    }
    let node = node_from_bytes(ctx, bytes)?;
    if node_to_bytes(ctx, node)? != bytes {
        return Err(InternalCustodyError::InvalidProgram.into());
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::{ExpectedAuthorization, execute};
    use crate::{DriverError, InternalCustodyError, SpendContext};
    use chia_protocol::{Coin, CoinSpend};
    use chia_sdk_signer::AggSigConstants;
    use clvm_traits::{clvm_list, clvm_quote};
    use clvmr::NodePtr;

    #[test]
    fn execution_rejects_relative_seconds_outside_u64() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let puzzle = ctx.alloc(&clvm_quote!(clvm_list!(clvm_list!(
            80u8,
            u128::from(u64::MAX) + 1
        ))))?;
        let coin = Coin::new([1; 32].into(), ctx.tree_hash(puzzle).into(), 1);
        let spend = CoinSpend::new(coin, ctx.serialize(&puzzle)?, ctx.serialize(&NodePtr::NIL)?);
        let result = execute(
            ctx,
            &[spend],
            &[(puzzle, NodePtr::NIL)],
            &ExpectedAuthorization::Unsigned,
            &AggSigConstants::new([2; 32].into()),
            500_000_000,
        );
        assert!(matches!(
            result,
            Err(DriverError::InternalCustody(
                InternalCustodyError::UnexpectedConditions
            ))
        ));
        Ok(())
    }
}
