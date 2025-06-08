use bindy::Result;
use chia_protocol::{Bytes, Bytes32};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sha2::{Digest, Sha256};

pub fn from_hex(value: String) -> Result<Bytes> {
    Ok(hex::decode(value)?.into())
}

pub fn to_hex(value: Bytes) -> Result<String> {
    Ok(hex::encode(value))
}

pub fn bytes_equal(lhs: Bytes, rhs: Bytes) -> Result<bool> {
    Ok(lhs == rhs)
}

pub fn tree_hash_atom(atom: Bytes) -> Result<Bytes32> {
    Ok(clvm_utils::tree_hash_atom(&atom).into())
}

pub fn tree_hash_pair(first: Bytes32, rest: Bytes32) -> Result<Bytes32> {
    Ok(clvm_utils::tree_hash_pair(first.into(), rest.into()).into())
}

pub fn sha256(value: Bytes) -> Result<Bytes32> {
    let mut hasher = Sha256::new();
    hasher.update(value);
    let hash: [u8; 32] = hasher.finalize().into();
    Ok(hash.into())
}

pub fn curry_tree_hash(program: Bytes32, args: Vec<Bytes32>) -> Result<Bytes32> {
    Ok(clvm_utils::curry_tree_hash(
        program.into(),
        &args.into_iter().map(Into::into).collect::<Vec<_>>(),
    )
    .to_bytes()
    .into())
}

pub fn generate_bytes(bytes: u32) -> Result<Bytes> {
    let mut rng = ChaCha20Rng::from_entropy();
    let mut buffer = vec![0; bytes as usize];
    rng.fill_bytes(&mut buffer);
    Ok(Bytes::new(buffer))
}
