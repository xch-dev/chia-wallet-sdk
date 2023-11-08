use chia_bls::{PublicKey, SecretKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPair {
    pub public_key: PublicKey,
    pub secret_key: SecretKey,
}
