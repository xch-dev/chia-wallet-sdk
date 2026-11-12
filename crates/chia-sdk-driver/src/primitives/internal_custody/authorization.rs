use super::{InternalCustodyError, InternalCustodyPolicy};
use crate::{Layer, SpendContext, SpendWithConditions, StandardLayer};
use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzle_types::DeriveSynthetic;
use chia_sdk_types::{
    Conditions, MerkleProof, Mod,
    puzzles::{
        InternalCustodyFilterArgs, InternalCustodyFilterConfig, InternalCustodyLockArgs,
        InternalCustodyMerkleSolution, InternalCustodyOnlyRekeyArgs, InternalCustodyPaymentFilter,
        InternalCustodyRekeyAndPaymentArgs, InternalCustodyRekeyFilter, P2SingletonArgs,
    },
};
use clvm_traits::clvm_list;
use clvmr::NodePtr;
use std::collections::HashSet;

pub(super) struct Authorization {
    pub filter: NodePtr,
    pub proof: MerkleProof,
    pub leaf: NodePtr,
    pub leaf_proof: MerkleProof,
    pub public_key: PublicKey,
}
impl InternalCustodyPolicy {
    pub(crate) fn aggregate_key(
        &self,
        signers: &[PublicKey],
    ) -> Result<PublicKey, crate::DriverError> {
        if signers.is_empty() || signers.len() > self.keys().len() {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let unique: HashSet<_> = signers.iter().map(PublicKey::to_bytes).collect();
        if unique.len() != signers.len()
            || signers
                .iter()
                .any(|p| p.is_inf() || !self.keys().contains(p))
        {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let key = signers.iter().fold(PublicKey::default(), |sum, p| sum + p);
        if key.is_inf() {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        Ok(key)
    }
    pub(crate) fn selected_key(
        &self,
        signers: &[PublicKey],
    ) -> Result<PublicKey, crate::DriverError> {
        if signers.len() != self.thresholds().required as usize {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let key = self.aggregate_key(signers)?.derive_synthetic();
        if key.is_inf() {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        Ok(key)
    }
    pub(crate) fn authorization_filter(
        &self,
        ctx: &mut SpendContext,
        root: Bytes32,
        multiple: u8,
    ) -> Result<NodePtr, crate::DriverError> {
        let rekey = self.rekey_puzzle(ctx)?;
        let rekey = InternalCustodyRekeyFilter {
            puzzle_hash: ctx.tree_hash(rekey).into(),
            allowed_timelock: u64::from(multiple),
        };
        let filter = if multiple == 1 {
            let payment = self.payment_puzzle(ctx)?;
            ctx.curry(InternalCustodyRekeyAndPaymentArgs {
                config: InternalCustodyFilterConfig {
                    rekey,
                    payment: InternalCustodyPaymentFilter {
                        puzzle_hash: ctx.tree_hash(payment).into(),
                        clawback_puzzle_hash: P2SingletonArgs::new(self.info().launcher_id)
                            .curry_tree_hash()
                            .into(),
                    },
                },
            })?
        } else {
            ctx.curry(InternalCustodyOnlyRekeyArgs { config: rekey })?
        };
        ctx.curry(InternalCustodyFilterArgs {
            inner_puzzle: filter,
            merkle_root: root,
            extra: (),
        })
    }
    pub(crate) fn lock_puzzle(
        &self,
        ctx: &mut SpendContext,
        key: PublicKey,
        root: Bytes32,
    ) -> Result<NodePtr, crate::DriverError> {
        let rekey = self.rekey_puzzle(ctx)?;
        ctx.curry(InternalCustodyLockArgs {
            public_key: key,
            rekey_puzzle_hash: ctx.tree_hash(rekey).into(),
            next_root: root,
        })
    }
    pub(super) fn authorization(
        &self,
        ctx: &mut SpendContext,
        signers: &[PublicKey],
        multiple: u8,
    ) -> Result<Authorization, crate::DriverError> {
        let t = self.thresholds();
        let count = if multiple == 0 {
            if t.required >= t.maximum || self.next_root().is_none() {
                return Err(InternalCustodyError::InvalidSigningSet.into());
            }
            t.required + 1
        } else {
            t.required
                .checked_sub(u32::from(multiple) - 1)
                .ok_or(InternalCustodyError::InvalidSigningSet)?
        };
        if count < t.minimum || signers.len() != count as usize {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let aggregate = self.aggregate_key(signers)?;
        let public_key = if multiple == 0 {
            aggregate
        } else {
            aggregate.derive_synthetic()
        };
        if public_key.is_inf() {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let leaf = if multiple == 0 {
            self.lock_puzzle(
                ctx,
                public_key,
                self.next_root()
                    .ok_or(InternalCustodyError::InvalidPolicy)?,
            )?
        } else {
            StandardLayer::new(public_key).construct_puzzle(ctx)?
        };
        let hash: Bytes32 = ctx.tree_hash(leaf).into();
        let proof = self
            .leaf_proofs
            .iter()
            .find(|p| p.leaf == hash)
            .ok_or(InternalCustodyError::InvalidSigningSet)?;
        let root = proof
            .proof
            .root(hash)
            .ok_or(InternalCustodyError::InvalidPolicy)?;
        let filter = self.authorization_filter(ctx, root, multiple)?;
        let hash: Bytes32 = ctx.tree_hash(filter).into();
        let filter_proof = self
            .filter_proofs
            .iter()
            .find(|p| p.leaf == hash && p.proof.root(hash) == Some(self.info().puzzle_root))
            .ok_or(InternalCustodyError::InvalidPolicy)?;
        Ok(Authorization {
            filter,
            proof: filter_proof.proof.clone(),
            leaf,
            leaf_proof: proof.proof.clone(),
            public_key,
        })
    }
    pub(super) fn authorize_conditions(
        &self,
        ctx: &mut SpendContext,
        signers: &[PublicKey],
        multiple: u8,
        conditions: Conditions,
        context: NodePtr,
    ) -> Result<(NodePtr, MerkleProof, NodePtr), crate::DriverError> {
        let auth = self.authorization(ctx, signers, multiple)?;
        let spend = StandardLayer::new(auth.public_key).spend_with_conditions(ctx, conditions)?;
        let solution = ctx.alloc(&InternalCustodyMerkleSolution {
            puzzle: spend.puzzle,
            proof: auth.leaf_proof,
            solution: clvm_list!(spend.solution, context),
        })?;
        Ok((auth.filter, auth.proof, solution))
    }
}
