use bindy::{Error, Result};
use chia_bls::{PublicKey, SecretKey};
use chia_protocol::{Bytes32, Coin};

use crate::{K1PublicKey, K1SecretKey, R1PublicKey, R1SecretKey};

/// `range_vec_with_seed` derives the pairs from the seeds `seed..seed + count`, which overflows
/// (and panics in debug builds) if the last seed doesn't fit in a `u64`.
fn check_seed_range(seed: u64, count: u32) -> Result<()> {
    if count > 0 && seed.checked_add(u64::from(count) - 1).is_none() {
        return Err(Error::Custom(format!(
            "Seed range starting at {seed} with {count} pairs overflows u64"
        )));
    }
    Ok(())
}

#[derive(Clone)]
pub struct BlsPair {
    pub sk: SecretKey,
    pub pk: PublicKey,
}

impl BlsPair {
    pub fn from_seed(seed: u64) -> Result<Self> {
        Ok(chia_sdk_test::BlsPair::new(seed).into())
    }

    pub fn many_from_seed(seed: u64, count: u32) -> Result<Vec<Self>> {
        check_seed_range(seed, count)?;
        let pairs = chia_sdk_test::BlsPair::range_vec_with_seed(seed, count.try_into().unwrap());
        Ok(pairs.into_iter().map(Into::into).collect())
    }
}

impl From<chia_sdk_test::BlsPair> for BlsPair {
    fn from(value: chia_sdk_test::BlsPair) -> Self {
        Self {
            sk: value.sk,
            pk: value.pk,
        }
    }
}
#[derive(Clone)]
pub struct BlsPairWithCoin {
    pub sk: SecretKey,
    pub pk: PublicKey,
    pub puzzle_hash: Bytes32,
    pub coin: Coin,
}

impl From<chia_sdk_test::BlsPairWithCoin> for BlsPairWithCoin {
    fn from(value: chia_sdk_test::BlsPairWithCoin) -> Self {
        Self {
            sk: value.sk,
            pk: value.pk,
            puzzle_hash: value.puzzle_hash,
            coin: value.coin,
        }
    }
}
#[derive(Clone)]
pub struct K1Pair {
    pub sk: K1SecretKey,
    pub pk: K1PublicKey,
}

impl K1Pair {
    pub fn from_seed(seed: u64) -> Result<Self> {
        Ok(chia_sdk_test::K1Pair::new(seed).into())
    }

    pub fn many_from_seed(seed: u64, count: u32) -> Result<Vec<Self>> {
        check_seed_range(seed, count)?;
        let pairs = chia_sdk_test::K1Pair::range_vec_with_seed(seed, count.try_into().unwrap());
        Ok(pairs.into_iter().map(Into::into).collect())
    }
}

impl From<chia_sdk_test::K1Pair> for K1Pair {
    fn from(value: chia_sdk_test::K1Pair) -> Self {
        Self {
            sk: K1SecretKey(value.sk),
            pk: K1PublicKey(value.pk),
        }
    }
}
#[derive(Clone)]
pub struct R1Pair {
    pub sk: R1SecretKey,
    pub pk: R1PublicKey,
}

impl R1Pair {
    pub fn from_seed(seed: u64) -> Result<Self> {
        Ok(chia_sdk_test::R1Pair::new(seed).into())
    }

    pub fn many_from_seed(seed: u64, count: u32) -> Result<Vec<Self>> {
        check_seed_range(seed, count)?;
        let pairs = chia_sdk_test::R1Pair::range_vec_with_seed(seed, count.try_into().unwrap());
        Ok(pairs.into_iter().map(Into::into).collect())
    }
}

impl From<chia_sdk_test::R1Pair> for R1Pair {
    fn from(value: chia_sdk_test::R1Pair) -> Self {
        Self {
            sk: R1SecretKey(value.sk),
            pk: R1PublicKey(value.pk),
        }
    }
}
