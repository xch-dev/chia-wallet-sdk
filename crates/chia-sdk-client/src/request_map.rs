use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chia_protocol::Message;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use crate::ClientError;

#[derive(Debug)]
pub(crate) struct Request {
    sender: oneshot::Sender<Message>,
    _permit: OwnedSemaphorePermit,
}

impl Request {
    pub(crate) fn send(self, message: Message) {
        self.sender.send(message).ok();
    }
}

#[derive(Debug)]
pub(crate) struct RequestMap {
    state: Mutex<State>,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug, Default)]
struct State {
    items: HashMap<u16, Request>,
    next_id: u16,
    closed: bool,
}

impl RequestMap {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            semaphore: Arc::new(Semaphore::new(u16::MAX as usize)),
        }
    }

    /// Reserves a request id for the sender. Ids are handed out round-robin, so that a late
    /// response to an abandoned request isn't delivered to a new request that reused its id.
    pub(crate) async fn insert(
        &self,
        sender: oneshot::Sender<Message>,
    ) -> Result<u16, ClientError> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ClientError::ConnectionClosed)?;

        let mut state = self.state.lock().expect("request map poisoned");

        if state.closed {
            return Err(ClientError::ConnectionClosed);
        }

        // The semaphore limits the number of outstanding requests to fewer than the number of
        // possible ids, so there is always a free id and this terminates.
        let mut id = state.next_id;
        while state.items.contains_key(&id) {
            id = id.wrapping_add(1);
        }
        state.next_id = id.wrapping_add(1);

        state.items.insert(
            id,
            Request {
                sender,
                _permit: permit,
            },
        );

        Ok(id)
    }

    pub(crate) fn remove(&self, id: u16) -> Option<Request> {
        self.state
            .lock()
            .expect("request map poisoned")
            .items
            .remove(&id)
    }

    /// Fails all pending requests and rejects any new ones.
    pub(crate) fn close(&self) {
        self.semaphore.close();

        let items = {
            let mut state = self.state.lock().expect("request map poisoned");
            state.closed = true;
            std::mem::take(&mut state.items)
        };

        // Dropping the senders wakes up every caller that's waiting on a response.
        drop(items);
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::ProtocolMessageTypes;

    use super::*;

    fn message(id: u16) -> Message {
        Message {
            msg_type: ProtocolMessageTypes::RespondPeers,
            id: Some(id),
            data: Vec::new().into(),
        }
    }

    #[tokio::test]
    async fn test_ids_are_not_immediately_reused() -> anyhow::Result<()> {
        let map = RequestMap::new();

        let (sender, _receiver) = oneshot::channel();
        let first = map.insert(sender).await?;
        assert!(map.remove(first).is_some());

        let (sender, _receiver) = oneshot::channel();
        let second = map.insert(sender).await?;
        assert_ne!(first, second);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_delivers_response() -> anyhow::Result<()> {
        let map = RequestMap::new();

        let (sender, receiver) = oneshot::channel();
        let id = map.insert(sender).await?;

        map.remove(id)
            .expect("request is pending")
            .send(message(id));
        assert_eq!(receiver.await?.id, Some(id));
        assert!(map.remove(id).is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_close_fails_pending_and_new_requests() -> anyhow::Result<()> {
        let map = RequestMap::new();

        let (sender, receiver) = oneshot::channel();
        map.insert(sender).await?;

        map.close();
        assert!(receiver.await.is_err());

        let (sender, _receiver) = oneshot::channel();
        assert!(matches!(
            map.insert(sender).await,
            Err(ClientError::ConnectionClosed)
        ));

        Ok(())
    }
}
