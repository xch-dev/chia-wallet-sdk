use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, standard::StandardArgs};

use super::PublicKey;

pub fn standard_puzzle_hash(synthetic_key: PublicKey) -> Bytes32 {
    StandardArgs::curry_tree_hash(synthetic_key.0).into()
}

pub fn cat_puzzle_hash(asset_id: Bytes32, inner_puzzle_hash: Bytes32) -> Bytes32 {
    CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into()).into()
}
