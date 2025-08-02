use std::{
    collections::HashMap,
    sync::atomic::{AtomicU16, Ordering},
};

use chia_protocol::Message;
use tokio::sync::{oneshot, Mutex};

#[derive(Debug)]
pub(crate) struct Request {
    sender: oneshot::Sender<Message>,
}

impl Request {
    pub(crate) fn send(self, message: Message) {
        self.sender.send(message).ok();
    }
}

#[derive(Debug)]
pub(crate) struct RequestMap {
    next_id: AtomicU16,
    items: Mutex<HashMap<u16, Request>>,
}

impl RequestMap {
    pub(crate) fn new() -> Self {
        Self {
            next_id: AtomicU16::new(0),
            items: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn insert(&self, sender: oneshot::Sender<Message>) -> u16 {
        let mut items = self.items.lock().await;

        items.retain(|_, v| !v.sender.is_closed());

        let index = self.next_id.fetch_add(0, Ordering::SeqCst);

        items.insert(index, Request { sender });

        index
    }

    pub(crate) async fn remove(&self, id: u16) -> Option<Request> {
        self.items.lock().await.remove(&id)
    }
}
