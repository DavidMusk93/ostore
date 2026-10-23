//! In-memory object event bus: a broadcast channel for live subscribers plus
//! a bounded ring buffer replayed to new SSE connections.

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

pub const CREATED: &str = "object.created";
pub const UPDATED: &str = "object.updated";
pub const DELETED: &str = "object.deleted";
pub const MOVED: &str = "object.moved";
pub const MKDIR: &str = "object.mkdir";

/// All known event kinds, for help text and webhook defaults.
pub const ALL_KINDS: [&str; 5] = [CREATED, UPDATED, DELETED, MOVED, MKDIR];

/// Number of recent events replayed to new SSE subscribers.
pub const RING_CAP: usize = 100;

/// One object mutation event.
#[derive(Clone, Debug, Serialize)]
pub struct ObjectEvent {
    /// Event kind, e.g. "object.created". Serialized by consumers under the
    /// key appropriate to their transport (SSE `event:` line / JSON "event").
    #[serde(skip)]
    pub kind: String,
    /// Root-relative object path (`to` path for moves).
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Source path for object.moved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub ts: DateTime<Utc>,
}

pub struct EventBus {
    tx: broadcast::Sender<ObjectEvent>,
    ring: Mutex<VecDeque<ObjectEvent>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx, ring: Mutex::new(VecDeque::with_capacity(RING_CAP)) }
    }

    /// Append to the replay ring and broadcast to live subscribers.
    /// No subscribers is not an error.
    pub fn publish(&self, ev: ObjectEvent) {
        if let Ok(mut ring) = self.ring.lock() {
            if ring.len() == RING_CAP {
                ring.pop_front();
            }
            ring.push_back(ev.clone());
        }
        let _ = self.tx.send(ev);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ObjectEvent> {
        self.tx.subscribe()
    }

    /// Oldest-first copy of the replay ring. Also used by tests.
    pub fn snapshot(&self) -> Vec<ObjectEvent> {
        match self.ring.lock() {
            Ok(ring) => ring.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}
