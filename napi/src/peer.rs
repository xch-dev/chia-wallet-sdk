use std::net::SocketAddr;

use chia_wallet_sdk::{
    self as sdk, connect_peer, create_native_tls_connector, load_ssl_cert, Connector,
};
use napi::bindgen_prelude::*;

use crate::{
    traits::{IntoJs, IntoRust},
    CoinState,
};

#[napi]
pub struct Tls(Connector);

#[napi]
impl Tls {
    #[napi(constructor)]
    pub fn new(cert_path: String, key_path: String) -> Result<Self> {
        let cert = load_ssl_cert(&cert_path, &key_path)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        let tls = create_native_tls_connector(&cert)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Self(tls))
    }
}

#[napi]
pub struct Peer(sdk::Peer);

#[napi]
impl Peer {
    #[napi(ts_args_type = "uri: string, tls: Tls, networkId: string")]
    pub async fn connect(uri: String, tls: Reference<Tls>, network_id: String) -> Result<Self> {
        let (peer, mut receiver) = connect_peer(
            network_id,
            tls.0.clone(),
            uri.parse::<SocketAddr>()
                .map_err(|error| Error::from_reason(error.to_string()))?,
        )
        .await
        .map_err(|error| Error::from_reason(error.to_string()))?;

        tokio::spawn(async move { while let Some(_message) = receiver.recv().await {} });

        Ok(Self(peer))
    }

    #[napi]
    pub async fn request_children(&self, coin_id: Uint8Array) -> Result<Vec<CoinState>> {
        self.0
            .request_children(coin_id.into_rust()?)
            .await
            .map_err(|error| Error::from_reason(error.to_string()))?
            .coin_states
            .into_iter()
            .map(IntoJs::into_js)
            .collect()
    }

    #[napi]
    pub async fn close(&self) -> Result<()> {
        self.0
            .close()
            .await
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(())
    }
}
