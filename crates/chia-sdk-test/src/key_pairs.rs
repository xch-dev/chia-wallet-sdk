use bip39::Mnemonic;
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::standard::StandardArgs;
use chia_secp::{K1PublicKey, K1SecretKey, R1PublicKey, R1SecretKey};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// A deterministic BLS key pair and its standard puzzle hash, for use in tests.
///
/// The secret key is derived from a seed by generating BIP-39 entropy with a seeded random number
/// generator, so the same seed always gives the same key. The [`Default`] key pair uses seed 0.
///
/// # Example
///
/// ```
/// use chia_puzzle_types::standard::StandardArgs;
/// use chia_sdk_test::BlsPair;
///
/// let alice = BlsPair::new(0);
/// assert_eq!(alice.pk, alice.sk.public_key());
/// assert_eq!(alice.puzzle_hash, StandardArgs::curry_tree_hash(alice.pk).into());
///
/// // Key pairs are deterministic.
/// assert_eq!(alice.pk, BlsPair::default().pk);
///
/// // A range starts at seed 0 by default, and each key uses the next seed.
/// let [first, second] = BlsPair::range();
/// assert_eq!(first.pk, alice.pk);
/// assert_eq!(second.pk, BlsPair::new(1).pk);
/// ```
#[derive(Debug, Clone)]
pub struct BlsPair {
    /// The secret key.
    pub sk: SecretKey,
    /// The public key of [`BlsPair::sk`].
    pub pk: PublicKey,
    /// The hash of the standard puzzle ([`StandardArgs`]) curried with [`BlsPair::pk`]
    /// directly, without deriving a synthetic key.
    pub puzzle_hash: Bytes32,
}

impl Default for BlsPair {
    fn default() -> Self {
        Self::new(0)
    }
}

impl BlsPair {
    /// Creates the key pair for the given seed, along with its standard puzzle hash.
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let entropy: [u8; 32] = rng.random();
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

    /// Creates `N` key pairs with seeds `0` through `N - 1`.
    pub fn range<const N: usize>() -> [Self; N] {
        Self::range_with_seed(0)
    }

    /// Creates `N` key pairs with seeds `seed` through `seed + N - 1`.
    pub fn range_with_seed<const N: usize>(seed: u64) -> [Self; N] {
        Self::range_vec_with_seed(seed, N).try_into().unwrap()
    }

    /// Creates `length` key pairs with seeds `0` through `length - 1`.
    pub fn range_vec(length: usize) -> Vec<Self> {
        Self::range_vec_with_seed(0, length)
    }

    /// Creates `length` key pairs with seeds `seed` through `seed + length - 1`.
    pub fn range_vec_with_seed(seed: u64, length: usize) -> Vec<Self> {
        let mut results = Vec::new();

        for i in 0..length {
            results.push(Self::new(seed + i as u64));
        }

        results
    }
}

/// A [`BlsPair`] along with a coin locked to its puzzle hash.
///
/// This is returned by [`Simulator::bls`](crate::Simulator::bls).
#[derive(Debug, Clone)]
pub struct BlsPairWithCoin {
    /// The secret key.
    pub sk: SecretKey,
    /// The public key of [`BlsPairWithCoin::sk`].
    pub pk: PublicKey,
    /// The hash of the standard puzzle ([`StandardArgs`]) curried with [`BlsPairWithCoin::pk`].
    pub puzzle_hash: Bytes32,
    /// The coin. It is normally locked to [`BlsPairWithCoin::puzzle_hash`], but
    /// [`BlsPairWithCoin::new`] doesn't check this.
    pub coin: Coin,
}

impl BlsPairWithCoin {
    /// Combines a key pair with a coin.
    pub fn new(pair: BlsPair, coin: Coin) -> Self {
        Self {
            sk: pair.sk,
            pk: pair.pk,
            puzzle_hash: pair.puzzle_hash,
            coin,
        }
    }
}

/// A deterministic secp256k1 key pair, for use in tests.
///
/// The secret key is 32 bytes from a random number generator seeded with the given seed, so the
/// same seed always gives the same key. The [`Default`] key pair uses seed 0.
///
/// # Example
///
/// ```
/// use chia_sdk_test::K1Pair;
///
/// let pair = K1Pair::new(0);
/// assert_eq!(pair.pk, pair.sk.public_key());
///
/// let pairs = K1Pair::range_vec_with_seed(10, 3);
/// assert_eq!(pairs.len(), 3);
/// assert_eq!(pairs[0].pk, K1Pair::new(10).pk);
/// ```
#[derive(Debug, Clone)]
pub struct K1Pair {
    /// The secret key.
    pub sk: K1SecretKey,
    /// The public key of [`K1Pair::sk`].
    pub pk: K1PublicKey,
}

impl Default for K1Pair {
    fn default() -> Self {
        Self::new(0)
    }
}

impl K1Pair {
    /// Creates the key pair for the given seed.
    ///
    /// # Panics
    ///
    /// Panics if the random bytes aren't a valid secret key, which is astronomically unlikely.
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let sk = K1SecretKey::from_bytes(&rng.random()).unwrap();
        let pk = sk.public_key();
        Self { sk, pk }
    }

    /// Creates `N` key pairs with seeds `0` through `N - 1`.
    pub fn range<const N: usize>() -> [Self; N] {
        Self::range_with_seed(0)
    }

    /// Creates `N` key pairs with seeds `seed` through `seed + N - 1`.
    pub fn range_with_seed<const N: usize>(seed: u64) -> [Self; N] {
        Self::range_vec_with_seed(seed, N).try_into().unwrap()
    }

    /// Creates `length` key pairs with seeds `0` through `length - 1`.
    pub fn range_vec(length: usize) -> Vec<Self> {
        Self::range_vec_with_seed(0, length)
    }

    /// Creates `length` key pairs with seeds `seed` through `seed + length - 1`.
    pub fn range_vec_with_seed(seed: u64, length: usize) -> Vec<Self> {
        let mut results = Vec::new();

        for i in 0..length {
            results.push(Self::new(seed + i as u64));
        }

        results
    }
}

/// A deterministic secp256r1 (P-256) key pair, for use in tests.
///
/// The secret key is 32 bytes from a random number generator seeded with the given seed, so the
/// same seed always gives the same key. The [`Default`] key pair uses seed 0.
///
/// # Example
///
/// ```
/// use chia_sdk_test::R1Pair;
///
/// let [a, b, c] = R1Pair::range_with_seed(5);
/// assert_eq!(a.pk, R1Pair::new(5).pk);
/// assert_eq!(c.pk, c.sk.public_key());
/// assert_ne!(a.pk, b.pk);
/// ```
#[derive(Debug, Clone)]
pub struct R1Pair {
    /// The secret key.
    pub sk: R1SecretKey,
    /// The public key of [`R1Pair::sk`].
    pub pk: R1PublicKey,
}

impl Default for R1Pair {
    fn default() -> Self {
        Self::new(0)
    }
}

impl R1Pair {
    /// Creates the key pair for the given seed.
    ///
    /// # Panics
    ///
    /// Panics if the random bytes aren't a valid secret key, which is astronomically unlikely.
    pub fn new(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let sk = R1SecretKey::from_bytes(&rng.random()).unwrap();
        let pk = sk.public_key();
        Self { sk, pk }
    }

    /// Creates `N` key pairs with seeds `0` through `N - 1`.
    pub fn range<const N: usize>() -> [Self; N] {
        Self::range_with_seed(0)
    }

    /// Creates `N` key pairs with seeds `seed` through `seed + N - 1`.
    pub fn range_with_seed<const N: usize>(seed: u64) -> [Self; N] {
        Self::range_vec_with_seed(seed, N).try_into().unwrap()
    }

    /// Creates `length` key pairs with seeds `0` through `length - 1`.
    pub fn range_vec(length: usize) -> Vec<Self> {
        Self::range_vec_with_seed(0, length)
    }

    /// Creates `length` key pairs with seeds `seed` through `seed + length - 1`.
    pub fn range_vec_with_seed(seed: u64, length: usize) -> Vec<Self> {
        let mut results = Vec::new();

        for i in 0..length {
            results.push(Self::new(seed + i as u64));
        }

        results
    }
}
