use chia_client::PeerEvent;
use chia_protocol::CoinStateUpdate;
use tokio::sync::broadcast;

pub fn coin_state_updates(receiver: &mut broadcast::Receiver<PeerEvent>) -> Vec<CoinStateUpdate> {
    let mut items = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let PeerEvent::CoinStateUpdate(event) = event {
            items.push(event);
        }
    }
    items
}
