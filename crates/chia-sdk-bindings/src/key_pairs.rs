use chia_protocol::Bytes32;

use crate::{Coin, K1PublicKey, K1SecretKey, PublicKey, R1PublicKey, R1SecretKey, SecretKey};

#[derive(Clone)]
pub struct BlsPair {
    pub sk: SecretKey,
    pub pk: PublicKey,
}

impl From<chia_sdk_test::BlsPair> for BlsPair {
    fn from(value: chia_sdk_test::BlsPair) -> Self {
        Self {
            sk: SecretKey(value.sk),
            pk: PublicKey(value.pk),
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
            sk: SecretKey(value.sk),
            pk: PublicKey(value.pk),
            puzzle_hash: value.puzzle_hash,
            coin: value.coin.into(),
        }
    }
}
#[derive(Clone)]
pub struct K1Pair {
    pub sk: K1SecretKey,
    pub pk: K1PublicKey,
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

impl From<chia_sdk_test::R1Pair> for R1Pair {
    fn from(value: chia_sdk_test::R1Pair) -> Self {
        Self {
            sk: R1SecretKey(value.sk),
            pk: R1PublicKey(value.pk),
        }
    }
}
