use chia::puzzles::nft;
use chia_wallet_sdk::{self as sdk, Primitive};
use napi::bindgen_prelude::*;

use crate::{
    traits::{FromJs, IntoJs, IntoRust},
    ClvmAllocator, Coin, CoinSpend, LineageProof, Program, Spend,
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

impl FromJs<Nft> for sdk::Nft<nft::NftMetadata> {
    fn from_js(nft: Nft) -> Result<Self> {
        Ok(sdk::Nft {
            coin: nft.coin.into_rust()?,
            proof: nft.lineage_proof.into_rust()?,
            info: nft.info.into_rust()?,
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

impl FromJs<NftInfo> for sdk::NftInfo<nft::NftMetadata> {
    fn from_js(info: NftInfo) -> Result<Self> {
        Ok(sdk::NftInfo {
            launcher_id: info.launcher_id.into_rust()?,
            metadata: info.metadata.into_rust()?,
            metadata_updater_puzzle_hash: info.metadata_updater_puzzle_hash.into_rust()?,
            current_owner: info.current_owner.map(IntoRust::into_rust).transpose()?,
            royalty_puzzle_hash: info.royalty_puzzle_hash.into_rust()?,
            royalty_ten_thousandths: info.royalty_ten_thousandths,
            p2_puzzle_hash: info.p2_puzzle_hash.into_rust()?,
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

impl FromJs<NftMetadata> for nft::NftMetadata {
    fn from_js(metadata: NftMetadata) -> Result<Self> {
        Ok(nft::NftMetadata {
            edition_number: metadata.edition_number.into_rust()?,
            edition_total: metadata.edition_total.into_rust()?,
            data_uris: metadata.data_uris,
            data_hash: metadata.data_hash.map(IntoRust::into_rust).transpose()?,
            metadata_uris: metadata.metadata_uris,
            metadata_hash: metadata
                .metadata_hash
                .map(IntoRust::into_rust)
                .transpose()?,
            license_uris: metadata.license_uris,
            license_hash: metadata.license_hash.map(IntoRust::into_rust).transpose()?,
        })
    }
}

#[napi(object)]
pub struct ParsedNft {
    pub info: NftInfo,
    pub inner_puzzle: ClassInstance<Program>,
}

pub fn parse_nft_info(
    env: Env,
    clvm: Reference<ClvmAllocator>,
    puzzle: &Program,
) -> Result<Option<ParsedNft>> {
    let puzzle = sdk::Puzzle::parse(&clvm.0.allocator, puzzle.ptr);

    let Some((nft_info, inner_puzzle)) =
        sdk::NftInfo::<nft::NftMetadata>::parse(&clvm.0.allocator, puzzle)
            .map_err(|error| Error::from_reason(error.to_string()))?
    else {
        return Ok(None);
    };

    Ok(Some(ParsedNft {
        info: nft_info.into_js()?,
        inner_puzzle: Program {
            ctx: clvm,
            ptr: inner_puzzle.ptr(),
        }
        .into_instance(env)?,
    }))
}

pub fn parse_unspent_nft(
    clvm: &mut ClvmAllocator,
    parent_coin: Coin,
    parent_puzzle: &Program,
    parent_solution: &Program,
    coin: Coin,
) -> Result<Option<Nft>> {
    let parent_puzzle = sdk::Puzzle::parse(&clvm.0.allocator, parent_puzzle.ptr);

    let Some(nft) = sdk::Nft::<nft::NftMetadata>::from_parent_spend(
        &mut clvm.0.allocator,
        parent_coin.into_rust()?,
        parent_puzzle,
        parent_solution.ptr,
        coin.into_rust()?,
    )
    .map_err(|error| Error::from_reason(error.to_string()))?
    else {
        return Ok(None);
    };

    Ok(Some(nft.into_js()?))
}

pub fn spend_nft(clvm: &mut ClvmAllocator, nft: Nft, inner_spend: Spend) -> Result<Vec<CoinSpend>> {
    let ctx = &mut clvm.0;
    let nft = sdk::Nft::<nft::NftMetadata>::from_js(nft)?;

    nft.spend(
        ctx,
        sdk::Spend {
            puzzle: inner_spend.puzzle.ptr,
            solution: inner_spend.solution.ptr,
        },
    )
    .map_err(|error| Error::from_reason(error.to_string()))?;

    ctx.take().into_iter().map(IntoJs::into_js).collect()
}
