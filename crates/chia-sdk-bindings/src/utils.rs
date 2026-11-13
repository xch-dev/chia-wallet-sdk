use bindy::Result;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS};
use rand::{Rng, SeedableRng, rngs::SysRng};
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
    let mut rng = ChaCha20Rng::try_from_rng(&mut SysRng).expect("failed to seed rng from the OS");
    let mut buffer = vec![0; bytes as usize];
    rng.fill_bytes(&mut buffer);
    Ok(Bytes::new(buffer))
}

pub fn select_coins(coins: Vec<Coin>, amount: u64) -> Result<Vec<Coin>> {
    Ok(chia_sdk_utils::select_coins(coins, amount)?)
}

pub fn spend_bundle_cost(
    coin_spends: Vec<CoinSpend>,
    mainnet: bool,
    height: Option<u32>,
) -> Result<u64> {
    let constants = if mainnet {
        &*MAINNET_CONSTANTS
    } else {
        &*TESTNET11_CONSTANTS
    };
    Ok(chia_sdk_driver::spend_bundle_cost(
        &coin_spends,
        constants,
        height,
    )?)
}

pub fn minimum_fee(cost: u64) -> Result<u64> {
    Ok(chia_sdk_driver::minimum_fee(cost))
}

pub fn recommended_fee(cost: u64, market_fee_per_cost: Option<f64>) -> Result<u64> {
    Ok(chia_sdk_driver::recommended_fee(cost, market_fee_per_cost))
}
