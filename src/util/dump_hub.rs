//! Broadcast channel for debug-dump lifecycle events (dashboard live view).

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 512;

#[derive(Clone, Debug)]
pub struct DumpHub {
    inner: Arc<DumpHubInner>,
}

#[derive(Debug)]
struct DumpHubInner {
    tx: broadcast::Sender<DumpEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DumpEvent {
    Created {
        id: String,
        model: String,
        endpoint: String,
        provider: String,
        is_streaming: bool,
        status: Option<u16>,
        files: Vec<String>,
    },
    Updated {
        id: String,
        model: String,
        endpoint: String,
        provider: String,
        is_streaming: bool,
        status: Option<u16>,
        files: Vec<String>,
    },
    Chunk {
        id: String,
        file: String,
        text: String,
    },
}

impl DumpHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            inner: Arc::new(DumpHubInner { tx }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DumpEvent> {
        self.inner.tx.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.inner.tx.receiver_count()
    }

    pub fn publish(&self, event: DumpEvent) {
        let _ = self.inner.tx.send(event);
    }
}

impl Default for DumpHub {
    fn default() -> Self {
        Self::new()
    }
}
