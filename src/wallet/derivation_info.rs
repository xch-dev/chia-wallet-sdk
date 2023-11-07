use chia_bls::PublicKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationInfo {
    pub puzzle_hash: [u8; 32],
    pub synthetic_pk: PublicKey,
}
