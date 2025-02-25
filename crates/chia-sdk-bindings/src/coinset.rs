use std::sync::Arc;

use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_coinset::ChiaRpcClient;

use crate::CoinRecord;

#[derive(Clone)]
pub struct CoinsetClient(Arc<chia_sdk_coinset::CoinsetClient>);

impl CoinsetClient {
    pub fn new(base_url: String) -> Result<Self> {
        Ok(Self(Arc::new(chia_sdk_coinset::CoinsetClient::new(
            base_url,
        ))))
    }

    pub fn testnet11() -> Result<Self> {
        Ok(Self(Arc::new(chia_sdk_coinset::CoinsetClient::testnet11())))
    }

    pub fn mainnet() -> Result<Self> {
        Ok(Self(Arc::new(chia_sdk_coinset::CoinsetClient::mainnet())))
    }

    pub async fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<Vec<CoinRecord>> {
        Ok(self
            .0
            .get_coin_records_by_hint(hint, start_height, end_height, include_spent_coins)
            .await?
            .coin_records
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect())
    }
}
