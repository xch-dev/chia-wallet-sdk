use bindy::{Error, Result};
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::{cat::CatArgs, nft, standard::StandardArgs};
use clvmr::NodePtr;

use crate::{LineageProof, Program, Puzzle, Spend};

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
            value.coin,
            value.lineage_proof.map(TryInto::try_into).transpose()?,
            value.asset_id,
            value.p2_puzzle_hash,
        ))
    }
}

impl From<chia_sdk_driver::Cat> for Cat {
    fn from(value: chia_sdk_driver::Cat) -> Self {
        Self {
            coin: value.coin,
            lineage_proof: value.lineage_proof.map(Into::into),
            asset_id: value.asset_id,
            p2_puzzle_hash: value.p2_puzzle_hash,
        }
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
pub struct ParsedCat {
    pub asset_id: Bytes32,
    pub p2_puzzle: Puzzle,
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
            coin: value.coin,
            lineage_proof: value.proof.into(),
            info: value.info.into(),
        }
    }
}

impl From<Nft> for chia_sdk_driver::Nft<Program> {
    fn from(value: Nft) -> Self {
        Self {
            coin: value.coin,
            proof: value.lineage_proof.into(),
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

impl From<NftInfo> for chia_sdk_driver::NftInfo<Program> {
    fn from(value: NftInfo) -> Self {
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
    pub p2_puzzle: Puzzle,
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

#[derive(Clone)]
pub struct Did {
    pub coin: Coin,
    pub lineage_proof: LineageProof,
    pub info: DidInfo,
}

#[derive(Clone)]
pub struct DidInfo {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: Program,
    pub p2_puzzle_hash: Bytes32,
}

impl From<chia_sdk_driver::Did<Program>> for Did {
    fn from(value: chia_sdk_driver::Did<Program>) -> Self {
        Self {
            coin: value.coin,
            lineage_proof: value.proof.into(),
            info: value.info.into(),
        }
    }
}

impl From<Did> for chia_sdk_driver::Did<Program> {
    fn from(value: Did) -> Self {
        Self {
            coin: value.coin,
            proof: value.lineage_proof.into(),
            info: value.info.into(),
        }
    }
}

impl From<chia_sdk_driver::DidInfo<Program>> for DidInfo {
    fn from(value: chia_sdk_driver::DidInfo<Program>) -> Self {
        Self {
            launcher_id: value.launcher_id,
            recovery_list_hash: value.recovery_list_hash,
            num_verifications_required: value.num_verifications_required,
            metadata: value.metadata,
            p2_puzzle_hash: value.p2_puzzle_hash,
        }
    }
}

impl From<DidInfo> for chia_sdk_driver::DidInfo<Program> {
    fn from(value: DidInfo) -> Self {
        Self {
            launcher_id: value.launcher_id,
            recovery_list_hash: value.recovery_list_hash,
            num_verifications_required: value.num_verifications_required,
            metadata: value.metadata,
            p2_puzzle_hash: value.p2_puzzle_hash,
        }
    }
}

#[derive(Clone)]
pub struct ParsedDid {
    pub info: DidInfo,
    pub p2_puzzle: Puzzle,
}

pub fn standard_puzzle_hash(synthetic_key: PublicKey) -> Result<Bytes32> {
    Ok(StandardArgs::curry_tree_hash(synthetic_key).into())
}

pub fn cat_puzzle_hash(asset_id: Bytes32, inner_puzzle_hash: Bytes32) -> Result<Bytes32> {
    Ok(CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into()).into())
}

#[derive(Clone)]
pub struct StreamingPuzzleInfo {
    pub recipient: Bytes32,
    pub clawback_ph: Option<Bytes32>,
    pub end_time: u64,
    pub last_payment_time: u64,
}

impl From<chia_sdk_driver::StreamingPuzzleInfo> for StreamingPuzzleInfo {
    fn from(value: chia_sdk_driver::StreamingPuzzleInfo) -> Self {
        Self {
            recipient: value.recipient,
            clawback_ph: value.clawback_ph,
            end_time: value.end_time,
            last_payment_time: value.last_payment_time,
        }
    }
}

impl From<StreamingPuzzleInfo> for chia_sdk_driver::StreamingPuzzleInfo {
    fn from(value: StreamingPuzzleInfo) -> Self {
        Self {
            recipient: value.recipient,
            clawback_ph: value.clawback_ph,
            end_time: value.end_time,
            last_payment_time: value.last_payment_time,
        }
    }
}

impl StreamingPuzzleInfo {
    pub fn amount_to_be_paid(&self, my_coin_amount: u64, payment_time: u64) -> Result<u64> {
        // LAST_PAYMENT_TIME + (to_pay * (END_TIME - LAST_PAYMENT_TIME) / my_amount) = payment_time
        // to_pay = my_amount * (payment_time - LAST_PAYMENT_TIME) / (END_TIME - LAST_PAYMENT_TIME)
        Ok(my_coin_amount * (payment_time - self.last_payment_time)
            / (self.end_time - self.last_payment_time))
    }

    pub fn get_hint(recipient: Bytes32) -> Result<Bytes32> {
        Ok(chia_sdk_driver::StreamingPuzzleInfo::get_hint(recipient))
    }

    pub fn get_launch_hints(&self) -> Result<Vec<Bytes>> {
        Ok(chia_sdk_driver::StreamingPuzzleInfo::get_launch_hints(
            &self.clone().into(),
        ))
    }

    pub fn inner_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(chia_sdk_driver::StreamingPuzzleInfo::inner_puzzle_hash(&self.clone().into()).into())
    }

    pub fn from_memos(memos: Vec<Bytes>) -> Result<Option<Self>> {
        Ok(chia_sdk_driver::StreamingPuzzleInfo::from_memos(&memos)?.map(Into::into))
    }
}

#[derive(Clone)]
pub struct StreamedCat {
    pub coin: Coin,
    pub asset_id: Bytes32,
    pub proof: LineageProof,

    pub info: StreamingPuzzleInfo,
}

impl From<chia_sdk_driver::StreamedCat> for StreamedCat {
    fn from(value: chia_sdk_driver::StreamedCat) -> Self {
        Self {
            coin: value.coin,
            asset_id: value.asset_id,
            proof: LineageProof {
                parent_parent_coin_info: value.proof.parent_parent_coin_info,
                parent_inner_puzzle_hash: Some(value.proof.parent_inner_puzzle_hash),
                parent_amount: value.proof.parent_amount,
            },
            info: value.info.into(),
        }
    }
}

impl TryFrom<StreamedCat> for chia_sdk_driver::StreamedCat {
    type Error = Error;

    fn try_from(value: StreamedCat) -> Result<Self> {
        Ok(chia_sdk_driver::StreamedCat::new(
            value.coin,
            value.asset_id,
            chia_puzzle_types::LineageProof {
                parent_parent_coin_info: value.proof.parent_parent_coin_info,
                parent_inner_puzzle_hash: value
                    .proof
                    .parent_inner_puzzle_hash
                    .ok_or(Error::MissingParentInnerPuzzleHash)?,
                parent_amount: value.proof.parent_amount,
            },
            value.info.into(),
        ))
    }
}
