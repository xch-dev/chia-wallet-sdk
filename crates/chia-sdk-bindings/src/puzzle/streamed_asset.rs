use bindy::Result;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::LineageProof;
use chia_sdk_driver::{StreamedAsset, StreamingPuzzleInfo};

pub trait StreamedAssetExt {
    fn new(
        coin: Coin,
        asset_id: Option<Bytes32>,
        proof: Option<LineageProof>,
        info: StreamingPuzzleInfo,
    ) -> Self;
    fn xch(coin: Coin, info: StreamingPuzzleInfo) -> Result<StreamedAsset>;
    fn cat(
        coin: Coin,
        asset_id: Bytes32,
        proof: LineageProof,
        info: StreamingPuzzleInfo,
    ) -> Result<StreamedAsset>;
}

impl StreamedAssetExt for StreamedAsset {
    fn new(
        coin: Coin,
        asset_id: Option<Bytes32>,
        proof: Option<LineageProof>,
        info: StreamingPuzzleInfo,
    ) -> Self {
        StreamedAsset {
            coin,
            asset_id,
            proof,
            info,
        }
    }

    fn xch(coin: Coin, info: StreamingPuzzleInfo) -> Result<StreamedAsset> {
        Ok(StreamedAsset::xch(coin, info))
    }

    fn cat(
        coin: Coin,
        asset_id: Bytes32,
        proof: LineageProof,
        info: StreamingPuzzleInfo,
    ) -> Result<StreamedAsset> {
        Ok(StreamedAsset::cat(coin, asset_id, proof, info))
    }
}

pub trait StreamingPuzzleInfoExt: Sized {
    fn amount_to_be_paid(&self, my_coin_amount: u64, payment_time: u64) -> Result<u64>;
    fn get_hint(recipient: Bytes32) -> Result<Bytes32>;
    fn get_launch_hints(&self) -> Result<Vec<Bytes>>;
    fn inner_puzzle_hash(&self) -> Result<Bytes32>;
    fn from_memos(memos: Vec<Bytes>) -> Result<Option<Self>>;
}

impl StreamingPuzzleInfoExt for StreamingPuzzleInfo {
    fn amount_to_be_paid(&self, my_coin_amount: u64, payment_time: u64) -> Result<u64> {
        // LAST_PAYMENT_TIME + (to_pay * (END_TIME - LAST_PAYMENT_TIME) / my_amount) = payment_time
        // to_pay = my_amount * (payment_time - LAST_PAYMENT_TIME) / (END_TIME - LAST_PAYMENT_TIME)
        Ok(my_coin_amount * (payment_time - self.last_payment_time)
            / (self.end_time - self.last_payment_time))
    }

    fn get_hint(recipient: Bytes32) -> Result<Bytes32> {
        Ok(Self::get_hint(recipient))
    }

    fn get_launch_hints(&self) -> Result<Vec<Bytes>> {
        Ok(self.get_launch_hints())
    }

    fn inner_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.inner_puzzle_hash().into())
    }

    fn from_memos(memos: Vec<Bytes>) -> Result<Option<Self>> {
        Ok(Self::from_memos(&memos)?)
    }
}

#[derive(Clone)]
pub struct StreamedAssetParsingResult {
    pub streamed_asset: Option<StreamedAsset>,
    pub last_spend_was_clawback: bool,
    pub last_payment_amount_if_clawback: u64,
}
