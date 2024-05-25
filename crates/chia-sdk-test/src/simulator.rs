use std::{net::SocketAddr, sync::Arc};

use bip39::Mnemonic;
use chia_bls::SecretKey;
use chia_client::Peer;
use chia_protocol::{Bytes32, Coin, CoinState};
use hex_literal::hex;
use peer_map::PeerMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use simulator_data::SimulatorData;
use simulator_error::SimulatorError;
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::connect_async;
use ws_connection::ws_connection;

mod peer_map;
mod simulator_data;
mod simulator_error;
mod ws_connection;

pub struct Simulator {
    rng: Mutex<ChaCha8Rng>,
    addr: SocketAddr,
    data: Arc<Mutex<SimulatorData>>,
    join_handle: JoinHandle<()>,
}

impl Simulator {
    pub const AGG_SIG_ME: Bytes32 = Bytes32::new(hex!(
        "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb"
    ));

    pub async fn new() -> Result<Self, SimulatorError> {
        log::info!("starting simulator");

        let addr = "127.0.0.1:0";
        let peer_map = PeerMap::default();
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr().unwrap();
        let data = Arc::new(Mutex::new(SimulatorData::default()));

        let data_clone = data.clone();

        let join_handle = tokio::spawn(async move {
            let data = data_clone;

            while let Ok((stream, addr)) = listener.accept().await {
                let stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(stream) => stream,
                    Err(error) => {
                        log::error!("error accepting websocket connection: {}", error);
                        continue;
                    }
                };
                tokio::spawn(ws_connection(peer_map.clone(), stream, addr, data.clone()));
            }
        });

        Ok(Self {
            rng: Mutex::new(ChaCha8Rng::seed_from_u64(0)),
            addr,
            join_handle,
            data,
        })
    }

    pub async fn connect(&self) -> Result<Peer, SimulatorError> {
        log::info!("connecting new peer to simulator");
        let (ws, _) = connect_async(format!("ws://{}", self.addr)).await?;
        Ok(Peer::new(ws))
    }

    pub async fn reset(&self) -> Result<(), SimulatorError> {
        let mut data = self.data.lock().await;
        *data = SimulatorData::default();
        Ok(())
    }

    pub async fn mint_coin(&self, puzzle_hash: Bytes32, amount: u64) -> Coin {
        let mut data = self.data.lock().await;

        let coin = Coin::new(
            Bytes32::new(self.rng.lock().await.gen()),
            puzzle_hash,
            amount,
        );

        data.create_coin(coin);

        coin
    }

    pub async fn coin_state(&self, coin_id: Bytes32) -> Option<CoinState> {
        let data = self.data.lock().await;
        data.coin_state(coin_id)
    }

    pub async fn secret_key(&self) -> Result<SecretKey, bip39::Error> {
        let entropy: [u8; 32] = self.rng.lock().await.gen();
        let mnemonic = Mnemonic::from_entropy(&entropy)?;
        let seed = mnemonic.to_seed("");
        Ok(SecretKey::from_seed(&seed))
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::{PublicKey, Signature};
    use chia_protocol::{CoinSpend, SpendBundle};
    use chia_sdk_types::conditions::{AggSigMe, CreateCoin};

    use crate::{test_transaction, to_program, to_puzzle};

    use super::*;

    #[tokio::test]
    async fn test_coin_state() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;

        let coin = sim.mint_coin(Bytes32::default(), 1000).await;
        let coin_state = sim
            .coin_state(coin.coin_id())
            .await
            .expect("missing coin state");

        assert_eq!(coin_state.coin, coin);
        assert_eq!(coin_state.created_height, Some(0));
        assert_eq!(coin_state.spent_height, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_empty_transaction() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let empty_bundle = SpendBundle::new(Vec::new(), Signature::default());
        let transaction_id = empty_bundle.name();

        let ack = peer.send_transaction(empty_bundle).await?;
        assert_eq!(ack.status, 3);
        assert_eq!(ack.txid, transaction_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_simple_transaction() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_unknown_coin() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = Coin::new(Bytes32::default(), puzzle_hash, 0);

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_bad_signature() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let public_key = sim.secret_key().await?.public_key();

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([AggSigMe {
                    public_key,
                    message: Vec::new().into(),
                }])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_infinity_signature() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([AggSigMe {
                    public_key: PublicKey::default(),
                    message: Vec::new().into(),
                }])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_valid_signature() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        test_transaction(
            &peer,
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([AggSigMe {
                    public_key: pk,
                    message: b"Hello, world!".to_vec().into(),
                }])?,
            )],
            &[sk],
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_aggregated_signature() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk1 = sim.secret_key().await?;
        let pk1 = sk1.public_key();

        let sk2 = sim.secret_key().await?;
        let pk2 = sk2.public_key();

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        test_transaction(
            &peer,
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([
                    AggSigMe {
                        public_key: pk1,
                        message: b"Hello, world!".to_vec().into(),
                    },
                    AggSigMe {
                        public_key: pk2,
                        message: b"Goodbye, world!".to_vec().into(),
                    },
                ])?,
            )],
            &[sk1, sk2],
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_excessive_output() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([CreateCoin::new(puzzle_hash, 1)])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_lineage() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let mut coin = sim.mint_coin(puzzle_hash, 1000).await;

        for _ in 0..1000 {
            let spend_bundle = SpendBundle::new(
                vec![CoinSpend::new(
                    coin,
                    puzzle_reveal.clone(),
                    to_program([CreateCoin::new(puzzle_hash, coin.amount - 1)])?,
                )],
                Signature::default(),
            );

            let ack = peer.send_transaction(spend_bundle).await?;
            assert_eq!(ack.status, 1);

            coin = Coin::new(coin.coin_id(), puzzle_hash, coin.amount - 1);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_request_children_unknown() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let children = peer.request_children(Bytes32::default()).await?;
        assert!(children.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_request_empty_children() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 0).await;

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(coin, puzzle_reveal, to_program(())?)],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let children = peer.request_children(coin.coin_id()).await?;
        assert!(children.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_request_children() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;

        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let spend_bundle = SpendBundle::new(
            vec![CoinSpend::new(
                coin,
                puzzle_reveal,
                to_program([
                    CreateCoin::new(puzzle_hash, 1),
                    CreateCoin::new(puzzle_hash, 2),
                ])?,
            )],
            Signature::default(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.status, 1);

        let children = peer.request_children(coin.coin_id()).await?;
        assert_eq!(children.len(), 2);

        let found_1 = children.iter().find(|cs| cs.coin.amount == 1).copied();
        let found_2 = children.iter().find(|cs| cs.coin.amount == 2).copied();

        let expected_1 = CoinState::new(Coin::new(coin.coin_id(), puzzle_hash, 1), None, Some(0));
        let expected_2 = CoinState::new(Coin::new(coin.coin_id(), puzzle_hash, 2), None, Some(0));

        assert_eq!(found_1, Some(expected_1));
        assert_eq!(found_2, Some(expected_2));

        Ok(())
    }
}
