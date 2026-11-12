use crate::{DriverError, InternalCustodyError};
use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::MerkleProof;
use chia_sdk_types::Mod;
use std::collections::HashSet;

mod derivation;

// Resource limits for untrusted public derivations; these are application support
// bounds, not limits imposed by the custody puzzles.
const MAX_POLICY_BYTES: usize = 128 * 1024;
const MAX_POLICY_KEYS: usize = 16;
const MAX_POLICY_PROOFS: usize = 1024;
const MAX_MERKLE_DEPTH: usize = 32;

/// Public parameters curried into a legacy custody singleton. Supplying these
/// parameters does not establish their origin. The verifier checks them against
/// the exact spend; applications own any separate policy recognition/authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyInfo {
    /// Permanent identity of the custody singleton.
    pub launcher_id: Bytes32,
    /// Merkle root committing to the authorized custody puzzles.
    pub puzzle_root: Bytes32,
    /// Minimum singleton age in seconds before a withdrawal or absorption.
    pub withdrawal_timelock: u64,
    /// ACH completion delay in seconds.
    pub payment_clawback: u64,
    /// Rekey completion delay in seconds.
    pub rekey_clawback: u64,
    /// Base rekey timelock in seconds.
    pub rekey_increment: u64,
    /// Additional penalty in seconds for slower rekey paths.
    pub slow_rekey: u64,
}
/// Signing threshold metadata from the public legacy derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyThresholds {
    /// Number of keys in the normal payment signing subset.
    pub required: u32,
    /// Maximum supported signing threshold.
    pub maximum: u32,
    /// Minimum supported signing threshold.
    pub minimum: u32,
}
/// A committed puzzle hash paired with its inclusion proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalCustodyProof {
    /// Puzzle hash committed as a Merkle leaf.
    pub leaf: Bytes32,
    /// Merkle inclusion path for the leaf.
    pub proof: MerkleProof,
}
/// Bounded public derivation data for authorizing a selected subset. The puzzle
/// root and inclusion proofs, not the public-key list alone, enforce authorization.
#[derive(Debug, Clone)]
pub struct InternalCustodyPolicy {
    info: InternalCustodyInfo,
    pub(super) drop_puzzle_hashes: chia_sdk_types::puzzles::InternalCustodyPuzzleHashes,
    keys: Vec<PublicKey>,
    next_root: Option<Bytes32>,
    thresholds: InternalCustodyThresholds,
    pub(super) filter_proofs: Vec<InternalCustodyProof>,
    pub(super) leaf_proofs: Vec<InternalCustodyProof>,
}
impl InternalCustodyPolicy {
    /// Validate and construct bounded public custody policy data.
    pub fn new(
        info: InternalCustodyInfo,
        keys: Vec<PublicKey>,
        thresholds: InternalCustodyThresholds,
        next_root: Option<Bytes32>,
        filter_proofs: Vec<InternalCustodyProof>,
        leaf_proofs: Vec<InternalCustodyProof>,
    ) -> Result<Self, DriverError> {
        if keys.is_empty()
            || keys.len() > MAX_POLICY_KEYS
            || keys.iter().any(PublicKey::is_inf)
            || thresholds.minimum == 0
            || thresholds.minimum > thresholds.required
            || thresholds.required > thresholds.maximum
            || thresholds.maximum as usize > keys.len()
            || filter_proofs.len() > MAX_POLICY_PROOFS
            || leaf_proofs.len() > MAX_POLICY_PROOFS
            || filter_proofs
                .iter()
                .chain(&leaf_proofs)
                .any(|p| p.proof.root(p.leaf).is_none())
        {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        let unique: HashSet<_> = keys.iter().map(PublicKey::to_bytes).collect();
        if unique.len() != keys.len() {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        let mut policy = Self {
            drop_puzzle_hashes: chia_sdk_types::puzzles::InternalCustodyPuzzleHashes {
                rekey: Bytes32::default(),
                payment: Bytes32::default(),
            },
            info,
            keys,
            next_root,
            thresholds,
            filter_proofs,
            leaf_proofs,
        };
        policy.drop_puzzle_hashes = chia_sdk_types::puzzles::InternalCustodyPuzzleHashes {
            rekey: chia_sdk_types::puzzles::InternalCustodyDropArgs {
                inner_puzzle: super::payment::DEFAULT_INNER,
                merkle_root: policy.rekey_tree().root(),
            }
            .curry_tree_hash()
            .into(),
            payment: chia_sdk_types::puzzles::InternalCustodyDropArgs {
                inner_puzzle: super::payment::DEFAULT_INNER,
                merkle_root: policy.payment_completion_tree().1.root(),
            }
            .curry_tree_hash()
            .into(),
        };
        if policy.to_legacy_bytes().len() > MAX_POLICY_BYTES {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Ok(policy)
    }
    /// Return the committed singleton parameters.
    pub fn info(&self) -> InternalCustodyInfo {
        self.info
    }
    /// Return the public key list in the supplied derivation.
    pub fn keys(&self) -> &[PublicKey] {
        &self.keys
    }
    /// Return the supplied signing threshold metadata.
    pub fn thresholds(&self) -> InternalCustodyThresholds {
        self.thresholds
    }

    /// Precommitted next security-level root, when present.
    pub fn next_root(&self) -> Option<Bytes32> {
        self.next_root
    }

    /// Decode the bounded, big-endian streamable `RootDerivation` format emitted
    /// by Chia-Network/internal-custody. This is public configuration data, not
    /// a signing request. The optional next-root field is metadata and is not
    /// used to authorize a payment under the current root.
    pub fn from_legacy_bytes(raw: &[u8]) -> Result<Self, DriverError> {
        if raw.len() > MAX_POLICY_BYTES {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        let mut r = Reader { raw, pos: 0 };
        let info = InternalCustodyInfo {
            launcher_id: r.take::<32>()?.into(),
            puzzle_root: r.take::<32>()?.into(),
            withdrawal_timelock: r.u64()?,
            payment_clawback: r.u64()?,
            rekey_clawback: r.u64()?,
            rekey_increment: r.u64()?,
            slow_rekey: r.u64()?,
        };
        let count = r.count(MAX_POLICY_KEYS)?;
        let mut keys = Vec::with_capacity(count);
        for _ in 0..count {
            keys.push(
                PublicKey::from_bytes(&r.take()?)
                    .map_err(|_| InternalCustodyError::InvalidPolicy)?,
            );
        }
        let thresholds = InternalCustodyThresholds {
            required: r.u32()?,
            maximum: r.u32()?,
            minimum: r.u32()?,
        };
        let next_root = match r.take::<1>()?[0] {
            0 => None,
            1 => Some(r.take::<32>()?.into()),
            _ => return Err(InternalCustodyError::InvalidPolicy.into()),
        };
        let filters = r.proofs()?;
        let leaves = r.proofs()?;
        if r.pos != raw.len() {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Self::new(info, keys, thresholds, next_root, filters, leaves)
    }
    /// Independently derive the public key/threshold definition and require its
    /// root and next-level commitment to match this policy. Proof order is not
    /// authority; construction checks inclusion proofs separately.
    pub fn verify_definition(&self, ctx: &mut crate::SpendContext) -> Result<(), DriverError> {
        let derived = Self::derive(ctx, self.info, self.keys.clone(), self.thresholds)?;
        if derived.info.puzzle_root != self.info.puzzle_root || derived.next_root != self.next_root
        {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Ok(())
    }
    /// Encode the exact Python streamable `RootDerivation` format.
    pub fn to_legacy_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(self.info.launcher_id.as_ref());
        out.extend_from_slice(self.info.puzzle_root.as_ref());
        for value in [
            self.info.withdrawal_timelock,
            self.info.payment_clawback,
            self.info.rekey_clawback,
            self.info.rekey_increment,
            self.info.slow_rekey,
        ] {
            out.extend_from_slice(&value.to_be_bytes());
        }
        out.extend_from_slice(
            &u32::try_from(self.keys.len())
                .expect("bounded key count")
                .to_be_bytes(),
        );
        for key in &self.keys {
            out.extend_from_slice(&key.to_bytes());
        }
        for value in [
            self.thresholds.required,
            self.thresholds.maximum,
            self.thresholds.minimum,
        ] {
            out.extend_from_slice(&value.to_be_bytes());
        }
        out.push(u8::from(self.next_root.is_some()));
        if let Some(root) = self.next_root {
            out.extend_from_slice(root.as_ref());
        }
        for proofs in [&self.filter_proofs, &self.leaf_proofs] {
            out.extend_from_slice(
                &u32::try_from(proofs.len())
                    .expect("bounded proof count")
                    .to_be_bytes(),
            );
            for entry in proofs {
                out.extend_from_slice(entry.leaf.as_ref());
                out.extend_from_slice(&entry.proof.path.to_be_bytes());
                out.extend_from_slice(
                    &u32::try_from(entry.proof.proof.len())
                        .expect("bounded proof depth")
                        .to_be_bytes(),
                );
                for sibling in &entry.proof.proof {
                    out.extend_from_slice(sibling.as_ref());
                }
            }
        }
        out
    }
}
struct Reader<'a> {
    raw: &'a [u8],
    pos: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], DriverError> {
        let end = self
            .pos
            .checked_add(N)
            .ok_or(InternalCustodyError::InvalidPolicy)?;
        let bytes = self
            .raw
            .get(self.pos..end)
            .ok_or(InternalCustodyError::InvalidPolicy)?;
        self.pos = end;
        bytes
            .try_into()
            .map_err(|_| InternalCustodyError::InvalidPolicy.into())
    }
    fn u32(&mut self) -> Result<u32, DriverError> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    fn u64(&mut self) -> Result<u64, DriverError> {
        Ok(u64::from_be_bytes(self.take()?))
    }
    fn count(&mut self, max: usize) -> Result<usize, DriverError> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(InternalCustodyError::InvalidPolicy.into());
        }
        Ok(n)
    }
    fn proofs(&mut self) -> Result<Vec<InternalCustodyProof>, DriverError> {
        let count = self.count(MAX_POLICY_PROOFS)?;
        let mut proofs = Vec::with_capacity(count);
        for _ in 0..count {
            let leaf = self.take::<32>()?.into();
            let path = self.u32()?;
            let depth = self.count(MAX_MERKLE_DEPTH)?;
            let mut siblings = Vec::with_capacity(depth);
            for _ in 0..depth {
                siblings.push(self.take::<32>()?.into());
            }
            proofs.push(InternalCustodyProof {
                leaf,
                proof: MerkleProof::new(path, siblings),
            });
        }
        Ok(proofs)
    }
}
