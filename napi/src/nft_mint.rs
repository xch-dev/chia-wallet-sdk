use chia::{
    clvm_traits::{FromClvm, ToClvm},
    protocol::Program,
    puzzles::nft::{self, NFT_METADATA_UPDATER_PUZZLE_HASH},
};
use chia_wallet_sdk as sdk;
use napi::bindgen_prelude::*;

use crate::{
    traits::{IntoJs, IntoRust},
    CoinSpend, Nft, NftMetadata,
};

#[napi(object)]
pub struct NftMint {
    pub metadata: NftMetadata,
    pub p2_puzzle_hash: Uint8Array,
    pub royalty_puzzle_hash: Uint8Array,
    pub royalty_ten_thousandths: u16,
}

#[napi(object)]
pub struct MintedNfts {
    pub nfts: Vec<Nft>,
    pub coin_spends: Vec<CoinSpend>,
    pub parent_conditions: Vec<Uint8Array>,
}

#[napi]
pub fn mint_nfts(parent_coin_id: Uint8Array, nft_mints: Vec<NftMint>) -> Result<MintedNfts> {
    let parent_coin_id = parent_coin_id.into_rust()?;

    let mut ctx = sdk::SpendContext::new();
    let mut result = MintedNfts {
        nfts: Vec::new(),
        coin_spends: Vec::new(),
        parent_conditions: Vec::new(),
    };

    let len = nft_mints.len();

    for (i, nft_mint) in nft_mints.into_iter().enumerate() {
        let (conditions, nft) = sdk::IntermediateLauncher::new(parent_coin_id, i, len)
            .create(&mut ctx)
            .map_err(|error| Error::from_reason(error.to_string()))?
            .mint_nft(
                &mut ctx,
                sdk::NftMint::<nft::NftMetadata> {
                    metadata: nft_mint.metadata.into_rust()?,
                    p2_puzzle_hash: nft_mint.p2_puzzle_hash.into_rust()?,
                    royalty_puzzle_hash: nft_mint.royalty_puzzle_hash.into_rust()?,
                    royalty_ten_thousandths: nft_mint.royalty_ten_thousandths,
                    metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
                    owner: None,
                },
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;

        result.nfts.push(nft.into_js()?);

        for condition in conditions {
            let condition = condition
                .to_clvm(&mut ctx.allocator)
                .map_err(|error| Error::from_reason(error.to_string()))?;

            let bytes = Program::from_clvm(&ctx.allocator, condition)
                .map_err(|error| Error::from_reason(error.to_string()))?;

            result
                .parent_conditions
                .push(Uint8Array::new(bytes.to_vec()));
        }
    }

    result.coin_spends.extend(
        ctx.take()
            .into_iter()
            .map(IntoJs::into_js)
            .collect::<Result<Vec<_>>>()?,
    );

    Ok(result)
}
