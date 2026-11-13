use super::{InternalCustodyError, InternalCustodyPolicy};
use crate::{DriverError, SpendContext};
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_sdk_types::{
    Conditions, Mod,
    puzzles::{
        InternalCustodyAchClawbackArgs, InternalCustodyAchClawbackSolution,
        InternalCustodyMerkleSolution, P2SingletonArgs,
    },
};

impl InternalCustodyPolicy {
    /// Cancel an unspent ACH coin, returning its full value to the launcher's
    /// p2-singleton deposit puzzle. Requires the policy committed when that
    /// payment was created, which may differ from the live singleton's policy.
    /// Clawback has no expiry; completion and cancellation compete for the input.
    pub fn payment_clawback_spend(
        &self,
        ctx: &mut SpendContext,
        pending: Coin,
        destination: Bytes32,
        signers: &[PublicKey],
        fee_announcement: bool,
    ) -> Result<CoinSpend, DriverError> {
        let puzzle = self.pending_payment_puzzle(ctx, destination)?;
        if pending.amount == 0
            || !pending.amount.is_multiple_of(2)
            || ctx.tree_hash(puzzle) != pending.puzzle_hash.into()
        {
            return Err(DriverError::WrongPuzzleHash);
        }
        let return_hash = P2SingletonArgs::new(self.info().launcher_id)
            .curry_tree_hash()
            .into();
        let mut conditions = Conditions::new().create_coin(
            return_hash,
            pending.amount,
            chia_puzzle_types::Memos::None,
        );
        if fee_announcement {
            conditions = conditions.create_coin_announcement(Bytes::default());
        }
        let context = ctx.alloc(&(self.info().puzzle_root, destination))?;
        let (filter, proof, solution) = self.authorize_conditions(
            ctx,
            signers,
            super::authorization::ConditionsAuthorization::Standard,
            conditions,
            context,
        )?;
        let clawback = InternalCustodyAchClawbackArgs {
            return_puzzle_hash: return_hash,
        };
        let (_, tree) = self.info().payment_completion_tree();
        let branch = ctx.curry(clawback)?;
        let solution = InternalCustodyMerkleSolution {
            puzzle: branch,
            proof: tree
                .proof(clawback.curry_tree_hash().into())
                .ok_or(InternalCustodyError::InvalidPolicy)?,
            solution: InternalCustodyAchClawbackSolution {
                amount: pending.amount,
                puzzle: filter,
                proof,
                solution,
            },
        };
        let solution = ctx.alloc(&solution)?;
        Ok(CoinSpend::new(
            pending,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        ))
    }
}
