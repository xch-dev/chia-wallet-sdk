use super::{InternalCustodyError, InternalCustodyInfo, InternalCustodyPolicy};
use crate::{DriverError, Layer, SpendContext};
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{LineageProof, Memos, Proof, singleton::SingletonSolution};
use chia_sdk_types::{
    Conditions, MerkleTree, Mod,
    puzzles::{
        InternalCustodyMerkleSolution, InternalCustodyRekeyArguments, InternalCustodyRekeyClawback,
        InternalCustodyRekeyClawbackSolution, InternalCustodyRekeyCompletionArgs,
        InternalCustodyRekeyState, InternalCustodySpendSolution,
    },
};
use clvm_traits::{clvm_curried_args, clvm_list};
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

/// Python's three-spend lock bundle separated into consensus-valid stages.
/// Confirm `authorization` before submitting `completion`; the second stage
/// requires no signature. Applications own persistence and chain readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalCustodyLockSpends {
    /// First stage: signed singleton spend creating the lock coin and continuation.
    pub authorization: CoinSpend,
    /// Second stage, in Python order: lock coin followed by `FINISH_REKEY` singleton.
    pub completion: [CoinSpend; 2],
}
impl InternalCustodyPolicy {
    pub(crate) fn rekey_tree(&self) -> MerkleTree {
        MerkleTree::new(&[
            InternalCustodyRekeyCompletionArgs {
                singleton_struct: chia_puzzle_types::singleton::SingletonStruct::new(
                    self.info().launcher_id,
                ),
                timelock: self.info().rekey_clawback,
            }
            .curry_tree_hash()
            .into(),
            InternalCustodyRekeyClawback::mod_hash().into(),
        ])
    }
    pub(crate) fn pending_rekey_puzzle(
        &self,
        ctx: &mut SpendContext,
        state: InternalCustodyRekeyState,
    ) -> Result<NodePtr, DriverError> {
        self.validate_rekey_state(state)?;
        let rekey = self.rekey_puzzle(ctx)?;
        ctx.alloc(&CurriedProgram {
            program: rekey,
            args: clvm_curried_args!(state),
        })
    }
    pub(crate) fn validate_rekey_state(
        &self,
        state: InternalCustodyRekeyState,
    ) -> Result<(), DriverError> {
        if state.old_root != self.info().puzzle_root
            || (state.timelock_multiple == 0 && self.next_root() != Some(state.new_root))
        {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Ok(())
    }
    fn minimum_rekey_multiple(&self, signers: &[PublicKey]) -> Result<u8, DriverError> {
        let n =
            u32::try_from(signers.len()).map_err(|_| InternalCustodyError::InvalidSigningSet)?;
        if n < self.thresholds().minimum || n > self.thresholds().required {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        u8::try_from(1 + self.thresholds().required - n)
            .map_err(|_| InternalCustodyError::InvalidSigningSet.into())
    }
    fn rekey_authorization_multiple(
        &self,
        state: InternalCustodyRekeyState,
        signers: &[PublicKey],
    ) -> Result<u8, DriverError> {
        self.validate_rekey_state(state)?;
        let minimum = self.minimum_rekey_multiple(signers)?;
        // Positive multiples are lower bounds, not exact signing-set identities.
        // Zero belongs exclusively to the separate, uncancellable lock path.
        if state.timelock_multiple < minimum {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        Ok(minimum)
    }
    pub(crate) fn check_update(&self, new: &Self) -> Result<(), DriverError> {
        if (InternalCustodyInfo {
            puzzle_root: self.info().puzzle_root,
            ..new.info()
        }) != self.info()
        {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Ok(())
    }
    /// Start a normal rekey or slower recovery using its authorized subset.
    /// The new policy must preserve the launcher's immutable puzzle state.
    /// This convenience form chooses the subset's minimum delay and checks the
    /// replacement policy metadata; `start_rekey_spend_with_state` accepts a raw
    /// target root and an explicit longer delay for offline reconstruction.
    pub fn start_rekey_spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        proof: Proof,
        new_policy: &Self,
        signers: &[PublicKey],
        fee_announcement: bool,
    ) -> Result<CoinSpend, DriverError> {
        self.check_update(new_policy)?;
        let state = InternalCustodyRekeyState {
            new_root: new_policy.info().puzzle_root,
            old_root: self.info().puzzle_root,
            timelock_multiple: self.minimum_rekey_multiple(signers)?,
        };
        self.start_rekey_spend_with_state(ctx, coin, proof, state, signers, fee_announcement)
    }
    /// Start a rekey with an explicit committed root and delay multiple. The
    /// selected subset authorizes its minimum delay or any longer positive delay.
    /// This low-level constructor does not establish the target key definition;
    /// clear signing independently verifies that definition against the root.
    pub fn start_rekey_spend_with_state(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        proof: Proof,
        state: InternalCustodyRekeyState,
        signers: &[PublicKey],
        fee_announcement: bool,
    ) -> Result<CoinSpend, DriverError> {
        let minimum = self.rekey_authorization_multiple(state, signers)?;
        let pending = self.pending_rekey_puzzle(ctx, state)?;
        // Python order: rekey coin, extra conditions, singleton continuation.
        let mut conditions =
            Conditions::new().create_coin(ctx.tree_hash(pending).into(), 0, Memos::None);
        if fee_announcement {
            conditions = conditions.create_coin_announcement(Bytes::default());
        }
        let inner = self.inner_layer(ctx)?.construct_puzzle(ctx)?;
        conditions = conditions.create_coin(ctx.tree_hash(inner).into(), coin.amount, Memos::None);
        let context = ctx.alloc(&state)?;
        let (filter, filter_proof, filter_solution) = self.authorize_conditions(
            ctx,
            signers,
            super::authorization::ConditionsAuthorization::rekey(minimum)?,
            conditions,
            context,
        )?;
        self.start_rekey(
            ctx,
            coin,
            proof,
            state,
            (filter, filter_proof, filter_solution),
        )
    }
    fn start_rekey(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        proof: Proof,
        state: InternalCustodyRekeyState,
        authorization: (NodePtr, chia_sdk_types::MerkleProof, NodePtr),
    ) -> Result<CoinSpend, DriverError> {
        let (filter, filter_proof, filter_solution) = authorization;
        let puzzle = self.singleton_puzzle(ctx)?;
        if coin.amount.is_multiple_of(2) || ctx.tree_hash(puzzle) != coin.puzzle_hash.into() {
            return Err(DriverError::WrongPuzzleHash);
        }
        let solution = ctx.alloc(&SingletonSolution {
            lineage_proof: proof,
            amount: coin.amount,
            inner_solution: InternalCustodySpendSolution {
                amount: coin.amount,
                spend_type: chia_sdk_types::puzzles::InternalCustodySpendType::StartRekey,
                action: InternalCustodyRekeyArguments {
                    timelock_multiple: state.timelock_multiple,
                    new_root: state.new_root,
                },
                filter_puzzle: filter,
                filter_proof,
                filter_solution,
            },
        })?;
        Ok(CoinSpend::new(
            coin,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        ))
    }
    /// Cancel a pending normal/slow rekey. A lock (zero multiple) cannot be cancelled.
    /// Bind the exact pending state independently of the subset's minimum delay.
    /// The target roster is neither required nor trusted when vetoing an attempt.
    pub fn rekey_clawback_spend(
        &self,
        ctx: &mut SpendContext,
        pending: Coin,
        state: InternalCustodyRekeyState,
        signers: &[PublicKey],
        fee_announcement: bool,
    ) -> Result<CoinSpend, DriverError> {
        let minimum = self.rekey_authorization_multiple(state, signers)?;
        let puzzle = self.pending_rekey_puzzle(ctx, state)?;
        if pending.amount != 0 || ctx.tree_hash(puzzle) != pending.puzzle_hash.into() {
            return Err(DriverError::WrongPuzzleHash);
        }
        let mut conditions = Conditions::new().create_coin(pending.puzzle_hash, 0, Memos::None);
        if fee_announcement {
            conditions = conditions.create_coin_announcement(Bytes::default());
        }
        let context = ctx.alloc(&state)?;
        let (filter, proof, solution) = self.authorize_conditions(
            ctx,
            signers,
            super::authorization::ConditionsAuthorization::rekey(minimum)?,
            conditions,
            context,
        )?;
        let branch = ctx.alloc_mod::<InternalCustodyRekeyClawback>()?;
        let solution = ctx.alloc(&InternalCustodyMerkleSolution {
            puzzle: branch,
            proof: self
                .rekey_tree()
                .proof(InternalCustodyRekeyClawback::mod_hash().into())
                .ok_or(InternalCustodyError::InvalidPolicy)?,
            solution: InternalCustodyRekeyClawbackSolution {
                puzzle_hash: pending.puzzle_hash,
                puzzle: filter,
                proof,
                solution,
            },
        })?;
        Ok(CoinSpend::new(
            pending,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        ))
    }
    /// Complete a pending rekey/lock using the current old-policy singleton.
    /// The two inputs mutually authorize each other. No custody signature is needed.
    /// Return order matches Python's `get_rekey_completion_spend` (singleton first).
    pub fn complete_rekey_spends(
        &self,
        ctx: &mut SpendContext,
        singleton: Coin,
        pending: Coin,
        state: InternalCustodyRekeyState,
        proofs: (Proof, LineageProof),
    ) -> Result<[CoinSpend; 2], DriverError> {
        let puzzle = self.singleton_puzzle(ctx)?;
        let pending_puzzle = self.pending_rekey_puzzle(ctx, state)?;
        if singleton.amount.is_multiple_of(2)
            || ctx.tree_hash(puzzle) != singleton.puzzle_hash.into()
            || pending.amount != 0
            || ctx.tree_hash(pending_puzzle) != pending.puzzle_hash.into()
        {
            return Err(DriverError::WrongPuzzleHash);
        }
        let solution = ctx.alloc(&SingletonSolution {
            lineage_proof: proofs.0,
            amount: singleton.amount,
            inner_solution: InternalCustodySpendSolution {
                amount: singleton.amount,
                spend_type: chia_sdk_types::puzzles::InternalCustodySpendType::FinishRekey,
                action: InternalCustodyRekeyArguments {
                    timelock_multiple: state.timelock_multiple,
                    new_root: state.new_root,
                },
                filter_puzzle: NodePtr::NIL,
                filter_proof: NodePtr::NIL,
                filter_solution: NodePtr::NIL,
            },
        })?;
        let singleton_spend = CoinSpend::new(
            singleton,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        );
        let complete = InternalCustodyRekeyCompletionArgs {
            singleton_struct: chia_puzzle_types::singleton::SingletonStruct::new(
                self.info().launcher_id,
            ),
            timelock: self.info().rekey_clawback,
        };
        let complete_puzzle = ctx.curry(complete)?;
        let solution = ctx.alloc(&InternalCustodyMerkleSolution {
            puzzle: complete_puzzle,
            proof: self
                .rekey_tree()
                .proof(complete.curry_tree_hash().into())
                .ok_or(InternalCustodyError::InvalidPolicy)?,
            solution: clvm_list!(proofs.1),
        })?;
        let pending_spend = CoinSpend::new(
            pending,
            ctx.serialize(&pending_puzzle)?,
            ctx.serialize(&solution)?,
        );
        Ok([singleton_spend, pending_spend])
    }
    /// Increase the threshold to the precommitted next level, using its dedicated
    /// higher-threshold, unsynthesized-key authorization. Confirm stage one first.
    pub fn increase_security_spends(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        proof: Proof,
        signers: &[PublicKey],
    ) -> Result<InternalCustodyLockSpends, DriverError> {
        let state = InternalCustodyRekeyState {
            new_root: self
                .next_root()
                .ok_or(InternalCustodyError::InvalidPolicy)?,
            old_root: self.info().puzzle_root,
            timelock_multiple: 0,
        };
        let auth = self.authorization(ctx, signers, super::authorization::AuthKind::Lock)?;
        let inner = self.inner_layer(ctx)?.construct_puzzle(ctx)?;
        let inner_hash: Bytes32 = ctx.tree_hash(inner).into();
        let leaf_solution = ctx.alloc(&clvm_list!(state.old_root, (inner_hash, coin.amount)))?;
        let context = ctx.alloc(&clvm_list!(state.new_root, state.old_root, 0u8))?;
        let filter_solution = ctx.alloc(&InternalCustodyMerkleSolution {
            puzzle: auth.leaf,
            proof: auth.leaf_proof,
            solution: clvm_list!(leaf_solution, context),
        })?;
        let authorization = self.start_rekey(
            ctx,
            coin,
            proof,
            state,
            (auth.filter, auth.proof, filter_solution),
        )?;
        let pending_puzzle = self.pending_rekey_puzzle(ctx, state)?;
        let pending = Coin::new(coin.coin_id(), ctx.tree_hash(pending_puzzle).into(), 0);
        let continuation = Coin::new(coin.coin_id(), coin.puzzle_hash, coin.amount);
        let lineage = LineageProof {
            parent_parent_coin_info: coin.parent_coin_info,
            parent_inner_puzzle_hash: inner_hash,
            parent_amount: coin.amount,
        };
        let [singleton, drop] = self.complete_rekey_spends(
            ctx,
            continuation,
            pending,
            state,
            (Proof::Lineage(lineage), lineage),
        )?;
        Ok(InternalCustodyLockSpends {
            authorization,
            completion: [drop, singleton],
        })
    }
}
