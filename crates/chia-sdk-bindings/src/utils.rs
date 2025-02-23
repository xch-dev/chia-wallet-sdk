use bindy::Result;
use chia_protocol::{Bytes, Bytes32};
use sha2::{Digest, Sha256};

pub fn from_hex(value: String) -> Result<Bytes> {
    Ok(hex::decode(value)?.into())
}

pub fn to_hex(value: Bytes) -> String {
    hex::encode(value)
}

pub fn bytes_equal(lhs: Bytes, rhs: Bytes) -> bool {
    lhs == rhs
}

pub fn tree_hash_atom(atom: Bytes32) -> Bytes32 {
    clvm_utils::tree_hash_atom(&atom).into()
}

pub fn tree_hash_pair(first: Bytes32, rest: Bytes32) -> Bytes32 {
    clvm_utils::tree_hash_pair(first.into(), rest.into()).into()
}

pub fn sha256(value: Bytes) -> Bytes32 {
    let mut hasher = Sha256::new();
    hasher.update(value);
    let hash: [u8; 32] = hasher.finalize().into();
    hash.into()
}

pub fn curry_tree_hash(program: Bytes32, args: Vec<Bytes32>) -> Bytes32 {
    clvm_utils::curry_tree_hash(
        program.into(),
        &args.into_iter().map(Into::into).collect::<Vec<_>>(),
    )
    .to_bytes()
    .into()
}
