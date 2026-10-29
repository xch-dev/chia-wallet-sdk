//! The upstream `chia-pos2` crate compiles C++ code, which can't be built for
//! `wasm32-unknown-unknown`. `chia-protocol` depends on it unconditionally, so this
//! crate is patched in its place with a pure Rust replacement for the subset of the
//! API that `chia-protocol` uses.
//!
//! Since `[patch]` only applies within this workspace, published crates still depend
//! on the upstream `chia-pos2`. Nothing in the SDK validates v2 proofs of space, so the
//! only behavior difference is that `ProofOfSpace::quality_string` returns `None`.

pub const NUM_CHAIN_LINKS: usize = 16;

pub type Bytes32 = [u8; 32];

#[repr(C)]
#[derive(Default, Clone)]
pub struct QualityChain {
    pub chain_links: [u64; NUM_CHAIN_LINKS],
}

/// Validating v2 proofs of space requires the C++ implementation, so every proof is
/// treated as invalid and no quality string is returned.
pub fn quality_string_from_proof(
    _plot_id: &Bytes32,
    _k: u8,
    _strength: u8,
    _proof: &[u8],
) -> Option<QualityChain> {
    None
}

pub fn serialize_quality(
    fragments: &[u64; NUM_CHAIN_LINKS],
    strength: u8,
) -> [u8; NUM_CHAIN_LINKS * 8 + 1] {
    let mut ret = [0_u8; NUM_CHAIN_LINKS * 8 + 1];

    ret[0] = strength;
    let mut idx = 1;
    for cl in fragments {
        ret[idx..(idx + 8)].clone_from_slice(&cl.to_le_bytes());
        idx += 8;
    }
    ret
}
