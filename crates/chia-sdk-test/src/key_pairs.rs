use bip39::Mnemonic;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::standard::StandardArgs;
use chia_secp::{K1PublicKey, K1SecretKey, R1PublicKey, R1SecretKey};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone)]
pub struct BlsPair {
    pub sk: SecretKey,
    pub pk: PublicKey,
    pub puzzle_hash: Bytes32,
}

impl Default for BlsPair {
    fn default() -> Self {
        Self::new(0)
    }
}

impl BlsPair {
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let entropy: [u8; 32] = rng.gen();
        let mnemonic = Mnemonic::from_entropy(&entropy).unwrap();
        let seed = mnemonic.to_seed("");
        let sk = SecretKey::from_seed(&seed);
        let pk = sk.public_key();
        Self {
            sk,
            pk,
            puzzle_hash: StandardArgs::curry_tree_hash(pk).into(),
        }
    }

    pub fn range<const N: usize>() -> [Self; N] {
        Self::range_with_seed(0)
    }

    pub fn range_with_seed<const N: usize>(seed: u64) -> [Self; N] {
        Self::range_vec_with_seed(seed, N).try_into().unwrap()
    }

    pub fn range_vec(length: usize) -> Vec<Self> {
        Self::range_vec_with_seed(0, length)
    }

    pub fn range_vec_with_seed(seed: u64, length: usize) -> Vec<Self> {
        let mut results = Vec::new();

        for i in 0..length {
            results.push(Self::new(seed + i as u64));
        }

        results
    }
}

#[derive(Debug, Clone)]
pub struct BlsPairWithCoin {
    pub sk: SecretKey,
    pub pk: PublicKey,
    pub puzzle_hash: Bytes32,
    pub coin: Coin,
}

impl BlsPairWithCoin {
    pub fn new(pair: BlsPair, coin: Coin) -> Self {
        Self {
            sk: pair.sk,
            pk: pair.pk,
            puzzle_hash: pair.puzzle_hash,
            coin,
        }
    }
}

#[derive(Debug, Clone)]
pub struct K1Pair {
    pub sk: K1SecretKey,
    pub pk: K1PublicKey,
}

impl Default for K1Pair {
    fn default() -> Self {
        Self::new(0)
    }
}

impl K1Pair {
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let sk = K1SecretKey::from_bytes(&rng.gen()).unwrap();
        let pk = sk.public_key();
        Self { sk, pk }
    }

    pub fn range<const N: usize>() -> [Self; N] {
        Self::range_with_seed(0)
    }

    pub fn range_with_seed<const N: usize>(seed: u64) -> [Self; N] {
        Self::range_vec_with_seed(seed, N).try_into().unwrap()
    }

    pub fn range_vec(length: usize) -> Vec<Self> {
        Self::range_vec_with_seed(0, length)
    }

    pub fn range_vec_with_seed(seed: u64, length: usize) -> Vec<Self> {
        let mut results = Vec::new();

        for i in 0..length {
            results.push(Self::new(seed + i as u64));
        }

        results
    }
}

#[derive(Debug, Clone)]
pub struct R1Pair {
    pub sk: R1SecretKey,
    pub pk: R1PublicKey,
}

impl Default for R1Pair {
    fn default() -> Self {
        Self::new(0)
    }
}

impl R1Pair {
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let sk = R1SecretKey::from_bytes(&rng.gen()).unwrap();
        let pk = sk.public_key();
        Self { sk, pk }
    }

    pub fn range<const N: usize>() -> [Self; N] {
        Self::range_with_seed(0)
    }

    pub fn range_with_seed<const N: usize>(seed: u64) -> [Self; N] {
        Self::range_vec_with_seed(seed, N).try_into().unwrap()
    }

    pub fn range_vec(length: usize) -> Vec<Self> {
        Self::range_vec_with_seed(0, length)
    }

    pub fn range_vec_with_seed(seed: u64, length: usize) -> Vec<Self> {
        let mut results = Vec::new();

        for i in 0..length {
            results.push(Self::new(seed + i as u64));
        }

        results
    }
}
