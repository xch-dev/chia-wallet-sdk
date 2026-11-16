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

/// A recovery multiple is strictly greater than the standard multiple of one.
#[derive(Clone, Copy)]
pub(super) struct RecoveryMultiple(u8);
impl RecoveryMultiple {
    fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy)]
pub(super) enum ConditionsAuthorization {
    Standard,
    Recovery(RecoveryMultiple),
}
impl ConditionsAuthorization {
    pub fn rekey(multiple: u8) -> Result<Self, crate::DriverError> {
        match multiple {
            0 => Err(InternalCustodyError::InvalidSigningSet.into()),
            1 => Ok(Self::Standard),
            m => Ok(Self::Recovery(RecoveryMultiple(m))),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum AuthKind {
    Lock,
    Conditions(ConditionsAuthorization),
}

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
    pub(super) fn authorization_filter(
        &self,
        ctx: &mut SpendContext,
        root: Bytes32,
        kind: AuthKind,
    ) -> Result<NodePtr, crate::DriverError> {
        let rekey = |allowed_timelock| InternalCustodyRekeyFilter {
            puzzle_hash: self.drop_puzzle_hashes.rekey,
            allowed_timelock,
        };
        let filter = match kind {
            AuthKind::Conditions(ConditionsAuthorization::Standard) => {
                ctx.curry(InternalCustodyRekeyAndPaymentArgs {
                    config: InternalCustodyFilterConfig {
                        rekey: rekey(1),
                        payment: InternalCustodyPaymentFilter {
                            puzzle_hash: self.drop_puzzle_hashes.payment,
                            clawback_puzzle_hash: P2SingletonArgs::new(self.info().launcher_id)
                                .curry_tree_hash()
                                .into(),
                        },
                    },
                })?
            }
            AuthKind::Lock => ctx.curry(InternalCustodyOnlyRekeyArgs { config: rekey(0) })?,
            AuthKind::Conditions(ConditionsAuthorization::Recovery(m)) => {
                ctx.curry(InternalCustodyOnlyRekeyArgs {
                    config: rekey(u64::from(m.get())),
                })?
            }
        };
        ctx.curry(InternalCustodyFilterArgs {
            inner_puzzle: filter,
            merkle_root: root,
            curried_args: (),
        })
    }
    pub(crate) fn lock_puzzle(
        &self,
        ctx: &mut SpendContext,
        key: PublicKey,
        root: Bytes32,
    ) -> Result<NodePtr, crate::DriverError> {
        ctx.curry(InternalCustodyLockArgs {
            public_key: key,
            rekey_puzzle_hash: self.drop_puzzle_hashes.rekey,
            next_root: root,
        })
    }
    pub(super) fn authorization(
        &self,
        ctx: &mut SpendContext,
        signers: &[PublicKey],
        kind: AuthKind,
    ) -> Result<Authorization, crate::DriverError> {
        let t = self.thresholds();
        let (count, synthetic) = match kind {
            AuthKind::Lock => {
                if t.required >= t.maximum || self.next_root().is_none() {
                    return Err(InternalCustodyError::InvalidSigningSet.into());
                }
                (t.required + 1, false)
            }
            AuthKind::Conditions(ConditionsAuthorization::Standard) => (t.required, true),
            AuthKind::Conditions(ConditionsAuthorization::Recovery(m)) => (
                t.required
                    .checked_sub(u32::from(m.get()) - 1)
                    .ok_or(InternalCustodyError::InvalidSigningSet)?,
                true,
            ),
        };
        if count < t.minimum || signers.len() != count as usize {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let aggregate = self.aggregate_key(signers)?;
        let public_key = if synthetic {
            aggregate.derive_synthetic()
        } else {
            aggregate
        };
        if public_key.is_inf() {
            return Err(InternalCustodyError::InvalidSigningSet.into());
        }
        let leaf = if synthetic {
            StandardLayer::new(public_key).construct_puzzle(ctx)?
        } else {
            self.lock_puzzle(
                ctx,
                public_key,
                self.next_root()
                    .ok_or(InternalCustodyError::InvalidPolicy)?,
            )?
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
        let filter = self.authorization_filter(ctx, root, kind)?;
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
        kind: ConditionsAuthorization,
        conditions: Conditions,
        context: NodePtr,
    ) -> Result<(NodePtr, MerkleProof, NodePtr), crate::DriverError> {
        let auth = self.authorization(ctx, signers, AuthKind::Conditions(kind))?;
        let spend = StandardLayer::new(auth.public_key).spend_with_conditions(ctx, conditions)?;
        let solution = ctx.alloc(&InternalCustodyMerkleSolution {
            puzzle: spend.puzzle,
            proof: auth.leaf_proof,
            solution: clvm_list!(spend.solution, context),
        })?;
        Ok((auth.filter, auth.proof, solution))
    }
}

#[cfg(test)]
mod tests {
    use super::ConditionsAuthorization;
    #[test]
    fn rekey_authorization_classifies_every_u8_multiple() {
        for value in 0..=255 {
            match ConditionsAuthorization::rekey(value) {
                Err(_) => assert_eq!(value, 0),
                Ok(ConditionsAuthorization::Standard) => assert_eq!(value, 1),
                Ok(ConditionsAuthorization::Recovery(multiple)) => {
                    assert!(value > 1);
                    assert_eq!(multiple.get(), value);
                }
            }
        }
    }
}
