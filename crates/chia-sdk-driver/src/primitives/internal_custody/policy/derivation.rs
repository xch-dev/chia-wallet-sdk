use super::{
    Bytes32, DriverError, InternalCustodyError, InternalCustodyInfo, InternalCustodyPolicy,
    InternalCustodyProof, InternalCustodyThresholds, MAX_POLICY_BYTES, MAX_POLICY_PROOFS,
    PublicKey,
};
use crate::{SpendContext, StandardLayer};
use chia_puzzle_types::DeriveSynthetic;
use chia_sdk_types::MerkleTree;
use clvm_utils::ToTreeHash;

impl InternalCustodyPolicy {
    /// Derive Python-compatible policy roots and ordered inclusion proofs.
    /// The supplied `puzzle_root` is replaced by the derived commitment. Expansion
    /// is bounded before generating combinations; unsupported sizes fail closed.
    pub fn derive(
        ctx: &mut SpendContext,
        info: InternalCustodyInfo,
        keys: Vec<PublicKey>,
        thresholds: InternalCustodyThresholds,
    ) -> Result<Self, DriverError> {
        let mut policy = Self::new(info, keys, thresholds, None, vec![], vec![])?;
        policy.keys.sort_by_key(PublicKey::to_bytes);
        // Every supported threshold contributes leaves. Bound the complete level,
        // not each combination independently (and do so before recursion/allocation).
        let mut count = 0usize;
        for k in thresholds.minimum..=thresholds.maximum {
            count = count
                .checked_add(combination_count(policy.keys.len(), k as usize))
                .ok_or(InternalCustodyError::InvalidPolicy)?;
        }
        if count > MAX_POLICY_PROOFS {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        if thresholds.required < thresholds.maximum {
            policy.next_root = Some(
                Self::derive(
                    ctx,
                    info,
                    policy.keys.clone(),
                    InternalCustodyThresholds {
                        required: thresholds.required + 1,
                        ..thresholds
                    },
                )?
                .info
                .puzzle_root,
            );
        }
        let mut filters = Vec::new();
        let standard = policy.combinations(thresholds.required as usize);
        let leaves: Vec<_> = standard
            .iter()
            .map(|pk| StandardLayer::new(pk.derive_synthetic()).tree_hash().into())
            .collect();
        let tree = MerkleTree::new(&leaves);
        let filter = policy.authorization_filter(ctx, tree.root(), 1)?;
        filters.push(ctx.tree_hash(filter).into());
        append_proofs(&mut policy.leaf_proofs, &leaves, &tree);
        if let Some(next_root) = policy.next_root {
            let mut leaves = Vec::new();
            for pk in policy.combinations(thresholds.required as usize + 1) {
                let puzzle = policy.lock_puzzle(ctx, pk, next_root)?;
                leaves.push(ctx.tree_hash(puzzle).into());
            }
            let tree = MerkleTree::new(&leaves);
            let filter = policy.authorization_filter(ctx, tree.root(), 0)?;
            filters.push(ctx.tree_hash(filter).into());
            append_proofs(&mut policy.leaf_proofs, &leaves, &tree);
        }
        for k in thresholds.minimum..thresholds.required {
            let leaves: Vec<_> = policy
                .combinations(k as usize)
                .iter()
                .map(|pk| StandardLayer::new(pk.derive_synthetic()).tree_hash().into())
                .collect();
            let tree = MerkleTree::new(&leaves);
            let multiple = u8::try_from(1 + thresholds.required - k)
                .map_err(|_| InternalCustodyError::InvalidPolicy)?;
            let filter = policy.authorization_filter(ctx, tree.root(), multiple)?;
            filters.push(ctx.tree_hash(filter).into());
            append_proofs(&mut policy.leaf_proofs, &leaves, &tree);
        }
        let tree = MerkleTree::new(&filters);
        policy.info.puzzle_root = tree.root();
        append_proofs(&mut policy.filter_proofs, &filters, &tree);
        // Respect the same total-byte bound used by the interchange decoder.
        if policy.to_reference_bytes().len() > MAX_POLICY_BYTES {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Ok(policy)
    }

    fn combinations(&self, k: usize) -> Vec<PublicKey> {
        fn visit(keys: &[PublicKey], k: usize, sum: PublicKey, out: &mut Vec<PublicKey>) {
            if k == 0 {
                out.push(sum);
                return;
            }
            for i in 0..=keys.len() - k {
                visit(&keys[i + 1..], k - 1, sum + &keys[i], out);
            }
        }
        let mut out = Vec::new();
        visit(&self.keys, k, PublicKey::default(), &mut out);
        out
    }
}
fn combination_count(n: usize, k: usize) -> usize {
    debug_assert!(k <= n);
    (0..k).fold(1, |value, i| value * (n - i) / (i + 1))
}
fn append_proofs(out: &mut Vec<InternalCustodyProof>, leaves: &[Bytes32], tree: &MerkleTree) {
    for leaf in leaves {
        let entry = InternalCustodyProof {
            leaf: *leaf,
            proof: tree.proof(*leaf).expect("leaf belongs to this tree"),
        };
        if let Some(existing) = out.iter_mut().find(|p| p.leaf == *leaf) {
            *existing = entry;
        } else {
            out.push(entry);
        }
    }
}
