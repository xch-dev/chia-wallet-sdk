use std::{collections::HashMap, sync::Arc};

use chia_protocol::Message;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, oneshot};

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

#[derive(Debug, Default)]
struct Items {
    requests: HashMap<u16, Request>,
    /// Where the search for the next free id starts. Ids are handed out round-robin rather
    /// than lowest-free, so an id freed by a timed-out request isn't immediately reused:
    /// a late response to that request then finds no entry instead of being delivered to
    /// an unrelated newer request.
    next_id: u16,
}

#[derive(Debug)]
pub(crate) struct RequestMap {
    items: Mutex<Items>,
    semaphore: Arc<Semaphore>,
}

impl RequestMap {
    pub(crate) fn new() -> Self {
        Self {
            items: Mutex::new(Items::default()),
            semaphore: Arc::new(Semaphore::new(u16::MAX as usize)),
        }
    }

    pub(crate) async fn insert(&self, sender: oneshot::Sender<Message>) -> u16 {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        let mut items = self.items.lock().await;

        items.requests.retain(|_, v| !v.sender.is_closed());

        let start = items.next_id;
        let index = (0..=u16::MAX)
            .map(|offset| start.wrapping_add(offset))
            .find(|i| !items.requests.contains_key(i))
            .expect("exceeded expected number of requests");

        items.requests.insert(
            index,
            Request {
                sender,
                _permit: permit,
            },
        );
        items.next_id = index.wrapping_add(1);
        index
    }

    pub(crate) async fn remove(&self, id: u16) -> Option<Request> {
        self.items.lock().await.requests.remove(&id)
    }

    #[cfg(test)]
    pub(crate) async fn len(&self) -> usize {
        self.items.lock().await.requests.len()
    }
}
