use std::time::Duration;

use bindy::Result;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sha2::{Digest, Sha256};

/// Sets a process-wide backstop timeout (in milliseconds) for *blocking* binding calls.
///
/// IMPORTANT: this affects **only the synchronous (C++) backend**. There, async methods
/// are driven to completion on a shared Tokio runtime via `block_on`, so a peer or RPC
/// endpoint that never responds would otherwise block the calling thread forever; this
/// timeout is the backstop that unblocks it.
///
/// It has **no effect on the async backends** (C#, Node/napi, Python/pyo3, wasm), which
/// hand the future to a host runtime that can cancel it. For those — and for per-request
/// limits on any backend — use the client-level timeouts instead:
/// `RpcClientOptions { timeout_ms, connect_timeout_ms }` and
/// `PeerOptions { connect_timeout_ms, request_timeout_ms }`, which live inside the futures
/// themselves and therefore apply everywhere.
///
/// Pass `None` (or `0`) to disable it, which is the default.
pub fn set_blocking_call_timeout(timeout_ms: Option<u32>) -> Result<()> {
    let timeout = timeout_ms
        .filter(|&ms| ms > 0)
        .map(|ms| Duration::from_millis(u64::from(ms)));
    crate::runtime::set_block_on_timeout(timeout);
    Ok(())
}

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
    let mut rng = ChaCha20Rng::from_os_rng();
    let mut buffer = vec![0; bytes as usize];
    rng.fill_bytes(&mut buffer);
    Ok(Bytes::new(buffer))
}

pub fn select_coins(coins: Vec<Coin>, amount: u64) -> Result<Vec<Coin>> {
    Ok(chia_sdk_utils::select_coins(coins, amount)?)
}

pub fn spend_bundle_cost(coin_spends: Vec<CoinSpend>) -> Result<u64> {
    Ok(chia_sdk_driver::spend_bundle_cost(&coin_spends)?)
}
