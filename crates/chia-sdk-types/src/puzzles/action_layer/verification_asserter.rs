use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::LineageProof;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const VERIFICATION_ASSERTER_PUZZLE: [u8; 434] = hex!(
    "
    ff02ffff01ff04ffff04ff04ffff04ffff0bffff0bff2effff0bff0affff0bff
    0aff36ff0580ffff0bff0affff0bff3effff0bff0affff0bff0aff36ffff0bff
    ff0102ffff0bffff0101ff0580ffff0bffff0102ffff0bffff0101ffff30ffff
    30ff819fffff0bff2effff0bff0affff0bff0aff36ff0580ffff0bff0affff0b
    ff3effff0bff0affff0bff0aff36ff1780ffff0bff0affff0bff3effff0bff0a
    ffff0bff0aff36ff82015f80ffff0bff0aff36ff26808080ff26808080ff2680
    8080ff8202df80ff0bff81ff8080ffff0bffff0101ff0b80808080ffff0bff0a
    ffff0bff3effff0bff0affff0bff0aff36ffff02ff2fff81bf8080ffff0bff0a
    ff36ff26808080ff26808080ff26808080ff8080ff808080ff8080ffff04ffff
    01ff3fff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5
    d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878
    a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4
    b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b715
    2a6e7298a91ce119a63400ade7c5ff018080
    "
);

pub const VERIFICATION_ASSERTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d33c552997cea95b0b66253b34f93c9126bd72839853194a2d03d95d1cc902a4
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct VerificationAsserterArgs<P> {
    pub singleton_mod_hash: Bytes32,
    pub launcher_puzzle_hash: Bytes32,
    pub verifier_singleton_struct_hash: Bytes32,
    pub verification_inner_puzzle_maker: P,
}

impl<P> VerificationAsserterArgs<P> {
    pub fn new(
        verifier_singleton_struct_hash: Bytes32,
        verification_inner_puzzle_maker: P,
    ) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
            verifier_singleton_struct_hash,
            verification_inner_puzzle_maker,
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct VerificationAsserterSolution<S> {
    pub verifier_proof: LineageProof,
    pub verification_inner_puzzle_maker_solution: S,
    #[clvm(rest)]
    pub launcher_amount: u64,
}

impl<P> Mod for VerificationAsserterArgs<P> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&VERIFICATION_ASSERTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        VERIFICATION_ASSERTER_PUZZLE_HASH
    }
}

pub const CATALOG_VERIFICATION_MAKER_PUZZLE: [u8; 299] = hex!(
    "
    ff02ffff01ff0bff16ffff0bff04ffff0bff04ff1aff0580ffff0bff04ffff0b
    ff1effff0bff04ffff0bff04ff1affff0bffff0101ff058080ffff0bff04ffff
    0bff1effff0bff04ffff0bff04ff1affff0bffff0102ffff0bffff0101ff0b80
    ffff0bffff0102ff17ffff0bffff0102ff2fffff0bffff0101ff5f8080808080
    ffff0bff04ff1aff12808080ff12808080ff12808080ffff04ffff01ff02ffff
    a04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c778545
    9aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718b
    a7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f680
    6923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce1
    19a63400ade7c5ff018080
    "
);

pub const CATALOG_VERIFICATION_MAKER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    cd2caba380e2bb21e209f8b5cad9d832a20bec53b5ffd3e29db51e4041a3d266
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct CatalogVerificationInnerPuzzleMakerArgs {
    pub verification_inner_puzzle_self_hash: Bytes32,
    pub version: u32,
    pub tail_hash_hash: Bytes32,
    pub data_hash_hash: Bytes32,
}

impl CatalogVerificationInnerPuzzleMakerArgs {
    pub fn new(
        verification_inner_puzzle_self_hash: Bytes32,
        version: u32,
        tail_hash_hash: TreeHash,
        data_hash_hash: TreeHash,
    ) -> Self {
        Self {
            verification_inner_puzzle_self_hash,
            version,
            tail_hash_hash: tail_hash_hash.into(),
            data_hash_hash: data_hash_hash.into(),
        }
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct CatalogVerificationInnerPuzzleMakerSolution {
    pub comment: String,
}

impl Mod for CatalogVerificationInnerPuzzleMakerArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&CATALOG_VERIFICATION_MAKER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        CATALOG_VERIFICATION_MAKER_PUZZLE_HASH
    }
}
