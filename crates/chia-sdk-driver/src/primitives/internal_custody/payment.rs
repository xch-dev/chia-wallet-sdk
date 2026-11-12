use crate::{
    DriverError, InternalCustodyError, InternalCustodyLayer, InternalCustodyPaymentAction,
    InternalCustodyPaymentOptions, InternalCustodyPolicy, InternalCustodyWithdrawal, Layer,
    P2SingletonLayer, SpendContext,
};
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{
    Memos, Proof,
    singleton::{SingletonArgs, SingletonSolution, SingletonStruct},
};
use chia_sdk_types::{
    Conditions, MerkleTree, Mod,
    puzzles::{
        InternalCustodyAchClawbackArgs, InternalCustodyAchCompletionArgs,
        InternalCustodyMerkleArgs, InternalCustodyMerkleSolution, InternalCustodyPaymentArguments,
        InternalCustodySpendSolution, P2SingletonArgs,
    },
};
use clvm_traits::{clvm_curried_args, clvm_list};
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

impl InternalCustodyPolicy {
    /// Construct the committed inner custody layer.
    pub fn inner_layer(&self, ctx: &mut SpendContext) -> Result<InternalCustodyLayer, DriverError> {
        let info = self.info();
        let rekey = self.rekey_puzzle(ctx)?;
        let payment = self.payment_puzzle(ctx)?;
        Ok(InternalCustodyLayer {
            puzzle_root: info.puzzle_root,
            rekey_puzzle_hash: ctx.tree_hash(rekey).into(),
            payment_puzzle_hash: ctx.tree_hash(payment).into(),
            withdrawal_timelock: info.withdrawal_timelock,
            rekey_increment: info.rekey_increment,
            slow_rekey: info.slow_rekey,
        })
    }
    /// Construct the full singleton puzzle for this policy.
    pub fn singleton_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let inner_puzzle = self.inner_layer(ctx)?.construct_puzzle(ctx)?;
        ctx.curry(SingletonArgs {
            singleton_struct: SingletonStruct::new(self.info().launcher_id),
            inner_puzzle,
        })
    }
    pub(crate) fn payment_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let (_, tree) = self.payment_completion_tree();
        let root = tree.root();
        let inner_puzzle = ctx.alloc(&clvm_list!(2u8, 2u8, 5u8))?;
        ctx.curry(InternalCustodyMerkleArgs {
            inner_puzzle,
            merkle_root: root,
        })
    }
    pub(super) fn payment_completion_tree(&self) -> (InternalCustodyAchCompletionArgs, MerkleTree) {
        let complete = InternalCustodyAchCompletionArgs {
            timelock: self.info().payment_clawback,
        };
        let clawback = InternalCustodyAchClawbackArgs {
            return_puzzle_hash: P2SingletonArgs::new(self.info().launcher_id)
                .curry_tree_hash()
                .into(),
        };
        let tree = MerkleTree::new(&[
            complete.curry_tree_hash().into(),
            clawback.curry_tree_hash().into(),
        ]);
        (complete, tree)
    }
    pub(crate) fn rekey_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let root = self.rekey_tree().root();
        let inner_puzzle = ctx.alloc(&clvm_list!(2u8, 2u8, 5u8))?;
        ctx.curry(InternalCustodyMerkleArgs {
            inner_puzzle,
            merkle_root: root,
        })
    }
    pub(crate) fn pending_payment_puzzle(
        &self,
        ctx: &mut SpendContext,
        destination: Bytes32,
    ) -> Result<NodePtr, DriverError> {
        let payment = self.payment_puzzle(ctx)?;
        ctx.alloc(&CurriedProgram {
            program: payment,
            args: clvm_curried_args!((self.info().puzzle_root, destination)),
        })
    }
    /// Complete a previously authorized pending payment. No custody signature is
    /// required: its destination and delay are already fixed by the pending coin.
    /// The caller must establish chain inclusion and relative-time eligibility.
    pub fn completion_spend(
        &self,
        ctx: &mut SpendContext,
        pending: Coin,
        destination: Bytes32,
    ) -> Result<CoinSpend, DriverError> {
        let puzzle = self.pending_payment_puzzle(ctx, destination)?;
        if pending.amount == 0
            || !pending.amount.is_multiple_of(2)
            || ctx.tree_hash(puzzle) != pending.puzzle_hash.into()
        {
            return Err(DriverError::WrongPuzzleHash);
        }
        let (complete, tree) = self.payment_completion_tree();
        let complete_hash = complete.curry_tree_hash();
        let proof = tree
            .proof(complete_hash.into())
            .ok_or(InternalCustodyError::InvalidPolicy)?;
        let complete_puzzle = ctx.curry(complete)?;
        let solution = InternalCustodyMerkleSolution {
            puzzle: complete_puzzle,
            proof,
            solution: clvm_list!(pending.amount),
        };
        Ok(CoinSpend::new(
            pending,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        ))
    }
    /// Construct an authorized withdrawal, deposit absorption, or both. The first
    /// spend is the singleton; remaining inputs are p2-singleton deposits in caller order. No fee inputs or
    /// arbitrary delegated conditions are permitted here.
    /// Deposit totals must be even to preserve the singleton's odd amount.
    /// Pass no withdrawal for claim-only; it creates no pending payment.
    /// Callers must check current input availability and relative-time eligibility.
    pub fn payment_spends(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        proof: Proof,
        withdrawal: Option<InternalCustodyWithdrawal>,
        signers: &[PublicKey],
        deposits: &[Coin],
    ) -> Result<Vec<CoinSpend>, DriverError> {
        self.payment_spends_with_options(
            ctx,
            coin,
            proof,
            InternalCustodyPaymentOptions {
                action: withdrawal.map_or(
                    InternalCustodyPaymentAction::Absorb {
                        destination_context: Bytes32::default(),
                    },
                    InternalCustodyPaymentAction::Withdraw,
                ),
                fee_announcement: false,
            },
            signers,
            deposits,
        )
    }
    /// Construct the exact supported Python payment form, preserving deposit
    /// order and absorb-only context. The only extra condition permitted is the
    /// reference CLI's empty fee announcement; no fee inputs are included.
    pub fn payment_spends_with_options(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        proof: Proof,
        options: InternalCustodyPaymentOptions,
        signers: &[PublicKey],
        deposits: &[Coin],
    ) -> Result<Vec<CoinSpend>, DriverError> {
        let (amount, destination) = match options.action {
            InternalCustodyPaymentAction::Withdraw(p) if p.amount > 0 => (p.amount, p.destination),
            InternalCustodyPaymentAction::Withdraw(_) => {
                return Err(InternalCustodyError::InvalidPayment.into());
            }
            InternalCustodyPaymentAction::Absorb {
                destination_context,
            } => (0, destination_context),
        };
        let incoming = self.validate_deposits(coin, deposits)?;
        let remaining = internal_custody_remaining_amount(coin.amount, incoming, amount)?;
        if !amount.is_multiple_of(2) || coin.amount.is_multiple_of(2) || remaining == 0 {
            return Err(InternalCustodyError::InvalidPayment.into());
        }
        let inner = self.inner_layer(ctx)?.construct_puzzle(ctx)?;
        let pending = self.pending_payment_puzzle(ctx, destination)?;
        let mut conditions =
            Conditions::new().create_coin(ctx.tree_hash(inner).into(), remaining, Memos::None);
        if amount > 0 {
            conditions = conditions.create_coin(ctx.tree_hash(pending).into(), amount, Memos::None);
        }
        let deposit_layer = P2SingletonLayer::new(self.info().launcher_id);
        let inner_hash: Bytes32 = ctx.tree_hash(inner).into();
        let mut deposit_spends = Vec::with_capacity(deposits.len());
        for deposit in deposits {
            let id = deposit.coin_id();
            conditions = conditions
                .create_puzzle_announcement(id.into())
                .assert_coin_announcement(deposit_announcement(id).into());
            let spend = deposit_layer.spend(ctx, id, inner_hash)?;
            deposit_spends.push(CoinSpend::new(
                *deposit,
                ctx.serialize(&spend.puzzle)?,
                ctx.serialize(&spend.solution)?,
            ));
        }
        if options.fee_announcement {
            conditions = conditions.create_coin_announcement(chia_protocol::Bytes::default());
        }
        let context = ctx.alloc(&(self.info().puzzle_root, destination))?;
        let (filter, filter_proof, filter_solution) =
            self.authorize_conditions(ctx, signers, 1, conditions, context)?;
        let solution = SingletonSolution {
            lineage_proof: proof,
            amount: coin.amount,
            inner_solution: InternalCustodySpendSolution {
                amount: coin.amount,
                spend_type: 2,
                action: InternalCustodyPaymentArguments {
                    outgoing: amount,
                    incoming,
                    destination,
                },
                filter_puzzle: filter,
                filter_proof,
                filter_solution,
            },
        };
        let puzzle = self.singleton_puzzle(ctx)?;
        if ctx.tree_hash(puzzle) != coin.puzzle_hash.into() {
            return Err(DriverError::WrongPuzzleHash);
        }
        let mut spends = vec![CoinSpend::new(
            coin,
            ctx.serialize(&puzzle)?,
            ctx.serialize(&solution)?,
        )];
        spends.extend(deposit_spends);
        Ok(spends)
    }

    fn validate_deposits(&self, singleton: Coin, deposits: &[Coin]) -> Result<u64, DriverError> {
        let puzzle_hash: Bytes32 = P2SingletonArgs::new(self.info().launcher_id)
            .curry_tree_hash()
            .into();
        if deposits.len() >= super::MAX_INTERNAL_CUSTODY_INPUTS
            || deposits
                .iter()
                .map(Coin::coin_id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != deposits.len()
        {
            return Err(InternalCustodyError::InvalidPayment.into());
        }
        let mut total = 0u64;
        for coin in deposits {
            if coin.puzzle_hash != puzzle_hash || *coin == singleton {
                return Err(InternalCustodyError::InvalidPayment.into());
            }
            total = total
                .checked_add(coin.amount)
                .ok_or(InternalCustodyError::InvalidPayment)?;
        }
        if !total.is_multiple_of(2) {
            return Err(InternalCustodyError::InvalidPayment.into());
        }
        Ok(total)
    }
}

pub(crate) fn internal_custody_remaining_amount(
    amount: u64,
    incoming: u64,
    outgoing: u64,
) -> Result<u64, DriverError> {
    let remaining = (u128::from(amount) + u128::from(incoming))
        .checked_sub(u128::from(outgoing))
        .ok_or(InternalCustodyError::InvalidPayment)?;
    u64::try_from(remaining).map_err(|_| InternalCustodyError::InvalidPayment.into())
}

pub(crate) fn deposit_announcement(id: Bytes32) -> [u8; 32] {
    let mut hash = chia_sha2::Sha256::new();
    hash.update(id.as_ref());
    hash.update(b"$");
    hash.finalize()
}
