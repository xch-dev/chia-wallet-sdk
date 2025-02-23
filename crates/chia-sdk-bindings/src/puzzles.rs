use bindy::{Error, Result};
use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, standard::StandardArgs};

use crate::{Coin, LineageProof, Spend};

use super::PublicKey;

#[derive(Clone)]
pub struct Cat {
    pub coin: Coin,
    pub lineage_proof: Option<LineageProof>,
    pub asset_id: Bytes32,
    pub p2_puzzle_hash: Bytes32,
}

impl TryFrom<Cat> for chia_sdk_driver::Cat {
    type Error = Error;

    fn try_from(value: Cat) -> Result<Self> {
        Ok(chia_sdk_driver::Cat::new(
            value.coin.into(),
            value.lineage_proof.map(TryInto::try_into).transpose()?,
            value.asset_id,
            value.p2_puzzle_hash,
        ))
    }
}

#[derive(Clone)]
pub struct CatSpend {
    pub cat: Cat,
    pub spend: Spend,
}

impl TryFrom<CatSpend> for chia_sdk_driver::CatSpend {
    type Error = Error;

    fn try_from(value: CatSpend) -> Result<Self> {
        Ok(chia_sdk_driver::CatSpend::new(
            value.cat.try_into()?,
            value.spend.into(),
        ))
    }
}

pub fn standard_puzzle_hash(synthetic_key: PublicKey) -> Result<Bytes32> {
    Ok(StandardArgs::curry_tree_hash(synthetic_key.0).into())
}

pub fn cat_puzzle_hash(asset_id: Bytes32, inner_puzzle_hash: Bytes32) -> Result<Bytes32> {
    Ok(CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into()).into())
}
