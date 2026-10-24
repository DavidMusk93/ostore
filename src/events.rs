//! In-memory object event bus: a broadcast channel for live subscribers plus
//! a bounded ring buffer used for replay and `Last-Event-ID` resume.
//!
//! Every event carries a monotonically increasing `id`. Ids are seeded from
//! the boot time in microseconds, so they keep increasing across restarts and
//! a cursor from a previous process is detected as a gap (never silently
//! mistaken for "up to date").

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

/// Control frame telling an SSE subscriber its cursor can't be honored
/// (gap past the ring, or the subscriber lagged) and it must re-list.
pub const STREAM_RESET: &str = "stream.reset";

/// Number of recent events kept for replay / resume.
pub const RING_CAP: usize = 1024;

/// Events replayed to a fresh SSE subscriber that sent no cursor.
pub const DEFAULT_REPLAY: usize = 100;

/// One object mutation event.
#[derive(Clone, Debug, Serialize)]
pub struct ObjectEvent {
    /// Event kind, e.g. "object.created". Serialized by consumers under the
    /// key appropriate to their transport (SSE `event:` line / JSON "event").
    #[serde(skip)]
    pub kind: String,
    /// Monotonic sequence id, assigned by [`EventBus::publish`].
    pub id: u64,
    /// Root-relative object path (`to` path for moves).
    pub path: String,
    /// "file" or "dir".
    #[serde(rename = "type")]
    pub entry_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Source path for object.moved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub ts: DateTime<Utc>,
}

/// What a new subscriber should be sent before live events.
pub enum Replay {
    Events(Vec<ObjectEvent>),
    /// The requested cursor is outside the ring: client must resync.
    Reset,
}

struct Ring {
    next_id: u64,
    buf: VecDeque<ObjectEvent>,
}

pub struct EventBus {
    tx: broadcast::Sender<ObjectEvent>,
    ring: Mutex<Ring>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        let seed = Utc::now().timestamp_micros().max(1) as u64;
        Self { tx, ring: Mutex::new(Ring { next_id: seed, buf: VecDeque::with_capacity(RING_CAP) }) }
    }

    /// Assign the next id, append to the ring and broadcast. The broadcast
    /// happens under the ring lock so ring order == delivery order, which is
    /// what makes [`EventBus::subscribe`] gap- and duplicate-free.
    pub fn publish(&self, mut ev: ObjectEvent) -> ObjectEvent {
        let mut ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
        ev.id = ring.next_id;
        ring.next_id += 1;
        if ring.buf.len() == RING_CAP {
            ring.buf.pop_front();
        }
        ring.buf.push_back(ev.clone());
        let _ = self.tx.send(ev.clone());
        ev
    }

    /// Subscribe atomically with a replay computed from the cursor:
    /// - `since = Some(id)`: every event after `id`, or `Reset` if events
    ///   after `id` already fell out of the ring (or `id` is from the future).
    /// - `since = None`: the last `replay` events.
    pub fn subscribe(&self, since: Option<u64>, replay: usize) -> (broadcast::Receiver<ObjectEvent>, Replay) {
        let ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
        let rx = self.tx.subscribe();
        let last_id = ring.next_id - 1;
        let out = match since {
            Some(s) if s > last_id => Replay::Reset,
            Some(s) if s == last_id => Replay::Events(Vec::new()),
            Some(s) => match ring.buf.front() {
                Some(oldest) if s + 1 >= oldest.id => {
                    Replay::Events(ring.buf.iter().filter(|e| e.id > s).cloned().collect())
                }
                _ => Replay::Reset,
            },
            None => {
                let skip = ring.buf.len().saturating_sub(replay);
                Replay::Events(ring.buf.iter().skip(skip).cloned().collect())
            }
        };
        (rx, out)
    }

    /// Oldest-first copy of the replay ring. Also used by tests.
    pub fn snapshot(&self) -> Vec<ObjectEvent> {
        let ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
        ring.buf.iter().cloned().collect()
    }

    /// Id of the most recent event (seed - 1 when none yet).
    pub fn last_id(&self) -> u64 {
        self.ring.lock().unwrap_or_else(|p| p.into_inner()).next_id - 1
    }

    pub fn ring_len(&self) -> usize {
        self.ring.lock().unwrap_or_else(|p| p.into_inner()).buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(path: &str) -> ObjectEvent {
        ObjectEvent {
            kind: CREATED.to_string(),
            id: 0,
            path: path.to_string(),
            entry_type: "file",
            size: Some(1),
            from: None,
            ts: Utc::now(),
        }
    }

    fn ids(r: Replay) -> Option<Vec<u64>> {
        match r {
            Replay::Events(v) => Some(v.iter().map(|e| e.id).collect()),
            Replay::Reset => None,
        }
    }

    #[test]
    fn ids_are_monotonic_and_resume_is_exact() {
        let bus = EventBus::new();
        let a = bus.publish(ev("a")).id;
        let b = bus.publish(ev("b")).id;
        let c = bus.publish(ev("c")).id;
        assert!(a < b && b < c);
        assert_eq!(ids(bus.subscribe(Some(a), 0).1), Some(vec![b, c]));
        assert_eq!(ids(bus.subscribe(Some(c), 0).1), Some(vec![]));
        assert_eq!(ids(bus.subscribe(None, 2).1), Some(vec![b, c]));
        assert_eq!(ids(bus.subscribe(None, 0).1), Some(vec![]));
    }

    #[test]
    fn stale_or_future_cursor_resets() {
        let bus = EventBus::new();
        let first = bus.publish(ev("a")).id;
        // Cursor from a previous process (ids seeded from boot time).
        assert!(ids(bus.subscribe(Some(first - 10), 0).1).is_none());
        assert!(ids(bus.subscribe(Some(first + 10), 0).1).is_none());
        // Cursor right before the oldest retained event is still exact.
        assert_eq!(ids(bus.subscribe(Some(first - 1), 0).1), Some(vec![first]));
    }

    #[test]
    fn overflowed_ring_resets_old_cursor() {
        let bus = EventBus::new();
        let first = bus.publish(ev("x")).id;
        bus.publish(ev("y"));
        for i in 0..RING_CAP {
            bus.publish(ev(&format!("f{i}")));
        }
        assert!(ids(bus.subscribe(Some(first), 0).1).is_none());
        assert_eq!(bus.ring_len(), RING_CAP);
    }
}
