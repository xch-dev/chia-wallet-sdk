use std::{net::SocketAddr, sync::Arc};

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
}

impl Drop for Simulator {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use chia_bls::Signature;
    use chia_client::PeerEvent;
    use chia_protocol::{
        CoinSpend, CoinState, Program, PuzzleSolutionResponse, SpendBundle, TransactionAck,
    };
    use chia_sdk_types::conditions::CreateCoin;
    use clvm_traits::{FromNodePtr, ToClvm};
    use clvm_utils::tree_hash;
    use clvmr::Allocator;

    #[tokio::test]
    async fn test_coin_lineage_many_blocks() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let mut coin = sim.mint_coin(puzzle_hash, 1000).await;

        for _ in 0..1000 {
            let solution = [CreateCoin::new(puzzle_hash, coin.amount - 1)].to_clvm(&mut a)?;

            let coin_spend = CoinSpend::new(
                coin,
                puzzle_reveal.clone(),
                Program::from_node_ptr(&a, solution)?,
            );

            let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

            let transaction_id = spend_bundle.name();
            let ack = peer.send_transaction(spend_bundle.clone()).await.unwrap();
            assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

            coin = Coin {
                parent_coin_info: coin.coin_id(),
                puzzle_hash,
                amount: coin.amount - 1,
            };

            let ack = peer.send_transaction(spend_bundle).await.unwrap();
            assert_eq!(ack.txid, transaction_id);
            assert_eq!(ack.status, 3);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_spend_unknown_coin() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let solution = [CreateCoin::new(puzzle_hash, 1000)].to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(
            Coin {
                parent_coin_info: Bytes32::new([42; 32]),
                puzzle_hash,
                amount: 1000,
            },
            puzzle_reveal,
            Program::from_node_ptr(&a, solution)?,
        );

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack.txid, transaction_id);
        assert_eq!(ack.status, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_coin_subscriptions() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let coin = sim.mint_coin(puzzle_hash, 1000).await;
        let mut cs = sim.coin_state(coin.coin_id()).await.unwrap();

        // Subscribe and request initial state.
        let results = peer
            .register_for_coin_updates(vec![coin.coin_id()], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        // The initial state should still be the same.
        let results = peer
            .register_for_coin_updates(vec![coin.coin_id()], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        // Spend the coin.
        let solution = [CreateCoin::new(puzzle_hash, coin.amount - 1)].to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(coin, puzzle_reveal, Program::from_node_ptr(&a, solution)?);

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        // We should have gotten a new peak and an update.
        // But the coin is spent now.
        cs.spent_height = Some(0);

        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PeerEvent::NewPeakWallet(..)));

        let event = receiver.recv().await.unwrap();
        match event {
            PeerEvent::CoinStateUpdate(update) => {
                assert_eq!(update.items, vec![cs]);
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_puzzle_subscriptions() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let coin = sim.mint_coin(puzzle_hash, 1000).await;
        let mut cs = sim.coin_state(coin.coin_id()).await.unwrap();

        // Subscribe and request initial state.
        let results = peer
            .register_for_ph_updates(vec![coin.puzzle_hash], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        // The initial state should still be the same.
        let results = peer
            .register_for_ph_updates(vec![coin.puzzle_hash], 0)
            .await
            .unwrap();

        assert_eq!(results, vec![cs]);

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        // Spend the coin.
        let solution = [CreateCoin::new(puzzle_hash, coin.amount - 1)].to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(coin, puzzle_reveal, Program::from_node_ptr(&a, solution)?);

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        // We should have gotten a new peak and an update.
        // But the coin is spent now.
        cs.spent_height = Some(0);
        let new_cs = CoinState {
            coin: Coin {
                parent_coin_info: cs.coin.coin_id(),
                puzzle_hash,
                amount: coin.amount - 1,
            },
            created_height: Some(0),
            spent_height: None,
        };

        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PeerEvent::NewPeakWallet(..)));

        let event = receiver.recv().await.unwrap();
        match event {
            PeerEvent::CoinStateUpdate(update) => {
                let items = update.items.into_iter().collect::<HashSet<_>>();
                let expected = vec![cs, new_cs].into_iter().collect::<HashSet<_>>();
                assert_eq!(items, expected);
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_hint_subscriptions() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let coin = sim.mint_coin(puzzle_hash, 1000).await;

        let hint = Bytes32::new([34; 32]);

        // Subscribe and request initial state.
        let results = peer.register_for_ph_updates(vec![hint], 0).await.unwrap();
        assert!(results.is_empty());

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        // Spend the coin.
        let solution = [CreateCoin::with_custom_hint(
            puzzle_hash,
            coin.amount - 1,
            hint,
        )]
        .to_clvm(&mut a)?;

        let coin_spend = CoinSpend::new(coin, puzzle_reveal, Program::from_node_ptr(&a, solution)?);

        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        // We should have gotten a new peak and an update.
        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PeerEvent::NewPeakWallet(..)));

        let event = receiver.recv().await.unwrap();
        match event {
            PeerEvent::CoinStateUpdate(update) => {
                let new_cs = CoinState {
                    coin: Coin {
                        parent_coin_info: coin.coin_id(),
                        puzzle_hash,
                        amount: coin.amount - 1,
                    },
                    created_height: Some(0),
                    spent_height: None,
                };

                assert_eq!(update.items, vec![new_cs]);
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_puzzle_solution() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let coin = sim.mint_coin(puzzle_hash, 1000).await;

        let solution = [CreateCoin::new(puzzle_hash, coin.amount - 1)].to_clvm(&mut a)?;

        let solution = Program::from_node_ptr(&a, solution)?;
        let coin_spend = CoinSpend::new(coin, puzzle_reveal.clone(), solution.clone());
        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        let response = peer
            .request_puzzle_and_solution(coin.coin_id(), 0)
            .await
            .unwrap();

        assert_eq!(
            response,
            PuzzleSolutionResponse::new(coin.coin_id(), 0, puzzle_reveal, solution)
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_request_children() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let mut a = Allocator::new();

        let puzzle = a.one();
        let puzzle_hash = tree_hash(&a, puzzle).into();
        let puzzle_reveal = Program::from_node_ptr(&a, puzzle)?;

        let coin = sim.mint_coin(puzzle_hash, 1000).await;

        let solution = [CreateCoin::new(puzzle_hash, coin.amount - 1)].to_clvm(&mut a)?;
        let solution = Program::from_node_ptr(&a, solution)?;
        let coin_spend = CoinSpend::new(coin, puzzle_reveal.clone(), solution.clone());
        let spend_bundle = SpendBundle::new(vec![coin_spend], Signature::default());

        let transaction_id = spend_bundle.name();
        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack, TransactionAck::new(transaction_id, 1, None));

        let mut receiver = peer.receiver().resubscribe();

        while receiver.try_recv().is_ok() {}

        let response = peer.request_children(coin.coin_id()).await.unwrap();

        let new_cs = CoinState {
            coin: Coin {
                parent_coin_info: coin.coin_id(),
                puzzle_hash,
                amount: coin.amount - 1,
            },
            created_height: Some(0),
            spent_height: None,
        };

        assert_eq!(response, vec![new_cs]);

        Ok(())
    }
}
