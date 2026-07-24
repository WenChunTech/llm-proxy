//! In-memory log ring buffer + broadcast channel for live dashboard streaming.

use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::broadcast;
use tracing_subscriber::fmt::MakeWriter;

const DEFAULT_CAPACITY: usize = 2_000;
const BROADCAST_CAPACITY: usize = 1_024;

/// Shared log fan-out used by the tracing writer and WebSocket clients.
#[derive(Clone, Debug)]
pub struct LogHub {
    inner: Arc<LogHubInner>,
}

#[derive(Debug)]
struct LogHubInner {
    buffer: Mutex<VecDeque<String>>,
    capacity: usize,
    tx: broadcast::Sender<String>,
    sequence: AtomicU64,
}

impl LogHub {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            inner: Arc::new(LogHubInner {
                buffer: Mutex::new(VecDeque::with_capacity(capacity)),
                capacity,
                tx,
                sequence: AtomicU64::new(0),
            }),
        }
    }

    pub fn push(&self, line: impl Into<String>) {
        let line = line.into();
        if line.is_empty() {
            return;
        }
        // Normalize to one logical line for the UI (keep trailing content).
        let line = strip_ansi(line.trim_end_matches(['\r', '\n']));
        if line.is_empty() {
            return;
        }

        if let Ok(mut buffer) = self.inner.buffer.lock() {
            if buffer.len() >= self.inner.capacity {
                buffer.pop_front();
            }
            buffer.push_back(line.clone());
        }
        self.inner.sequence.fetch_add(1, Ordering::Relaxed);
        let _ = self.inner.tx.send(line);
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.inner
            .buffer
            .lock()
            .map(|buffer| buffer.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.inner.tx.subscribe()
    }

    pub fn sequence(&self) -> u64 {
        self.inner.sequence.load(Ordering::Relaxed)
    }
}

impl Default for LogHub {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> MakeWriter<'a> for LogHub {
    type Writer = LogLineWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogLineWriter {
            hub: self.clone(),
            buf: Vec::new(),
        }
    }
}

/// Collects one formatted tracing event, then pushes it into the hub on drop.
pub struct LogLineWriter {
    hub: LogHub,
    buf: Vec<u8>,
}

impl Write for LogLineWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LogLineWriter {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.buf).into_owned();
        self.hub.push(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_evicts_oldest() {
        let hub = LogHub::with_capacity(2);
        hub.push("one");
        hub.push("two");
        hub.push("three");
        assert_eq!(hub.snapshot(), vec!["two".to_string(), "three".to_string()]);
    }

    #[test]
    fn subscribers_receive_new_lines() {
        let hub = LogHub::new();
        let mut rx = hub.subscribe();
        hub.push("hello");
        let line = rx.try_recv().expect("line");
        assert_eq!(line, "hello");
    }
}

fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if (0x40..=0x7e).contains(&b) {
                        break;
                    }
                }
                continue;
            }
            continue;
        }
        let ch = input[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
