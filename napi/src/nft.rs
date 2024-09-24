use chia::puzzles::nft;
use chia_wallet_sdk::{self as sdk, Primitive};
use clvmr::{
    serde::{node_from_bytes, node_to_bytes},
    Allocator,
};
use napi::bindgen_prelude::*;

use crate::{
    traits::{IntoJs, IntoRust},
    Coin, LineageProof,
};

#[napi(object)]
pub struct Nft {
    pub coin: Coin,
    pub lineage_proof: LineageProof,
    pub info: NftInfo,
}

impl IntoJs<Nft> for sdk::Nft<nft::NftMetadata> {
    fn into_js(self) -> Result<Nft> {
        Ok(Nft {
            coin: self.coin.into_js()?,
            lineage_proof: self.proof.into_js()?,
            info: self.info.into_js()?,
        })
    }
}

#[napi(object)]
pub struct NftInfo {
    pub launcher_id: Uint8Array,
    pub metadata: NftMetadata,
    pub metadata_updater_puzzle_hash: Uint8Array,
    pub current_owner: Option<Uint8Array>,
    pub royalty_puzzle_hash: Uint8Array,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Uint8Array,
}

impl IntoJs<NftInfo> for sdk::NftInfo<nft::NftMetadata> {
    fn into_js(self) -> Result<NftInfo> {
        Ok(NftInfo {
            launcher_id: self.launcher_id.into_js()?,
            metadata: self.metadata.into_js()?,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash.into_js()?,
            current_owner: self.current_owner.map(IntoJs::into_js).transpose()?,
            royalty_puzzle_hash: self.royalty_puzzle_hash.into_js()?,
            royalty_ten_thousandths: self.royalty_ten_thousandths,
            p2_puzzle_hash: self.p2_puzzle_hash.into_js()?,
        })
    }
}

#[napi(object)]
pub struct NftMetadata {
    pub edition_number: BigInt,
    pub edition_total: BigInt,
    pub data_uris: Vec<String>,
    pub data_hash: Option<Uint8Array>,
    pub metadata_uris: Vec<String>,
    pub metadata_hash: Option<Uint8Array>,
    pub license_uris: Vec<String>,
    pub license_hash: Option<Uint8Array>,
}

impl IntoJs<NftMetadata> for nft::NftMetadata {
    fn into_js(self) -> Result<NftMetadata> {
        Ok(NftMetadata {
            edition_number: self.edition_number.into_js()?,
            edition_total: self.edition_total.into_js()?,
            data_uris: self.data_uris,
            data_hash: self.data_hash.map(IntoJs::into_js).transpose()?,
            metadata_uris: self.metadata_uris,
            metadata_hash: self.metadata_hash.map(IntoJs::into_js).transpose()?,
            license_uris: self.license_uris,
            license_hash: self.license_hash.map(IntoJs::into_js).transpose()?,
        })
    }
}

#[napi(object)]
pub struct ParsedNft {
    pub nft_info: NftInfo,
    pub inner_puzzle: Uint8Array,
}

#[napi]
pub fn parse_nft_info(puzzle_reveal: Uint8Array) -> Result<Option<ParsedNft>> {
    let mut allocator = Allocator::new();
    let ptr = node_from_bytes(&mut allocator, puzzle_reveal.as_ref())?;
    let puzzle = sdk::Puzzle::parse(&allocator, ptr);

    let Some((nft_info, inner_puzzle)) =
        sdk::NftInfo::<nft::NftMetadata>::parse(&allocator, puzzle)
            .map_err(|error| Error::from_reason(error.to_string()))?
    else {
        return Ok(None);
    };

    Ok(Some(ParsedNft {
        nft_info: nft_info.into_js()?,
        inner_puzzle: node_to_bytes(&allocator, inner_puzzle.ptr())?.into(),
    }))
}

#[napi]
pub fn parse_unspent_nft(
    parent_coin: Coin,
    parent_puzzle_reveal: Uint8Array,
    parent_solution: Uint8Array,
    coin: Coin,
) -> Result<Option<Nft>> {
    let mut allocator = Allocator::new();
    let parent_ptr = node_from_bytes(&mut allocator, parent_puzzle_reveal.as_ref())?;
    let parent_puzzle = sdk::Puzzle::parse(&allocator, parent_ptr);
    let parent_solution = node_from_bytes(&mut allocator, parent_solution.as_ref())?;

    let Some(nft) = sdk::Nft::<nft::NftMetadata>::from_parent_spend(
        &mut allocator,
        parent_coin.into_rust()?,
        parent_puzzle,
        parent_solution,
        coin.into_rust()?,
    )
    .map_err(|error| Error::from_reason(error.to_string()))?
    else {
        return Ok(None);
    };

    Ok(Some(nft.into_js()?))
}
