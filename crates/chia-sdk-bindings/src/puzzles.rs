use bindy::{Error, Result};
use chia_protocol::Bytes32;
use chia_puzzle_types::{cat::CatArgs, nft, standard::StandardArgs};
use clvmr::NodePtr;

use crate::{Coin, LineageProof, Program, Spend};

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

#[derive(Clone)]
pub struct Nft {
    pub coin: Coin,
    pub lineage_proof: LineageProof,
    pub info: NftInfo,
}

impl From<chia_sdk_driver::Nft<Program>> for Nft {
    fn from(value: chia_sdk_driver::Nft<Program>) -> Self {
        Self {
            coin: value.coin.into(),
            lineage_proof: value.proof.into(),
            info: value.info.into(),
        }
    }
}

#[derive(Clone)]
pub struct NftInfo {
    pub launcher_id: Bytes32,
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
}

impl From<chia_sdk_driver::NftInfo<Program>> for NftInfo {
    fn from(value: chia_sdk_driver::NftInfo<Program>) -> Self {
        Self {
            launcher_id: value.launcher_id,
            metadata: value.metadata,
            metadata_updater_puzzle_hash: value.metadata_updater_puzzle_hash,
            current_owner: value.current_owner,
            royalty_puzzle_hash: value.royalty_puzzle_hash,
            royalty_ten_thousandths: value.royalty_ten_thousandths,
            p2_puzzle_hash: value.p2_puzzle_hash,
        }
    }
}

#[derive(Clone)]
pub struct ParsedNft {
    pub info: NftInfo,
    pub p2_puzzle: Program,
}

#[derive(Clone)]
pub struct NftMetadata {
    pub edition_number: u64,
    pub edition_total: u64,
    pub data_uris: Vec<String>,
    pub data_hash: Option<Bytes32>,
    pub metadata_uris: Vec<String>,
    pub metadata_hash: Option<Bytes32>,
    pub license_uris: Vec<String>,
    pub license_hash: Option<Bytes32>,
}

impl From<nft::NftMetadata> for NftMetadata {
    fn from(value: nft::NftMetadata) -> Self {
        Self {
            edition_number: value.edition_number,
            edition_total: value.edition_total,
            data_uris: value.data_uris,
            data_hash: value.data_hash,
            metadata_uris: value.metadata_uris,
            metadata_hash: value.metadata_hash,
            license_uris: value.license_uris,
            license_hash: value.license_hash,
        }
    }
}

impl From<NftMetadata> for nft::NftMetadata {
    fn from(value: NftMetadata) -> Self {
        nft::NftMetadata {
            edition_number: value.edition_number,
            edition_total: value.edition_total,
            data_uris: value.data_uris,
            data_hash: value.data_hash,
            metadata_uris: value.metadata_uris,
            metadata_hash: value.metadata_hash,
            license_uris: value.license_uris,
            license_hash: value.license_hash,
        }
    }
}

#[derive(Clone)]
pub struct NftMint {
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub p2_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub owner: Option<DidOwner>,
}

impl From<NftMint> for chia_sdk_driver::NftMint<NodePtr> {
    fn from(value: NftMint) -> Self {
        chia_sdk_driver::NftMint {
            metadata: value.metadata.1,
            metadata_updater_puzzle_hash: value.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: value.royalty_puzzle_hash,
            royalty_ten_thousandths: value.royalty_ten_thousandths,
            p2_puzzle_hash: value.p2_puzzle_hash,
            owner: value.owner.map(Into::into),
        }
    }
}

#[derive(Clone)]
pub struct DidOwner {
    pub did_id: Bytes32,
    pub inner_puzzle_hash: Bytes32,
}

impl From<DidOwner> for chia_sdk_driver::DidOwner {
    fn from(value: DidOwner) -> Self {
        chia_sdk_driver::DidOwner {
            did_id: value.did_id,
            inner_puzzle_hash: value.inner_puzzle_hash,
        }
    }
}

#[derive(Clone)]
pub struct MintedNfts {
    pub nfts: Vec<Nft>,
    pub parent_conditions: Vec<Program>,
}

pub fn standard_puzzle_hash(synthetic_key: PublicKey) -> Result<Bytes32> {
    Ok(StandardArgs::curry_tree_hash(synthetic_key.0).into())
}

pub fn cat_puzzle_hash(asset_id: Bytes32, inner_puzzle_hash: Bytes32) -> Result<Bytes32> {
    Ok(CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into()).into())
}
