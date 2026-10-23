//! ostore — a minimal agent-friendly object-storage daemon.
//!
//! Exposes object-storage semantics (upload/list/stat/download/delete/mkdir/
//! move) over HTTP for one bound server directory, plus an event system
//! (SSE + webhooks) so observers can watch object mutations.

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod paths;
pub mod range;
pub mod routes;
pub mod webhooks;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;

use crate::events::{EventBus, ObjectEvent};
use crate::webhooks::WebhookStore;

pub use routes::build_router;

/// Shared application state, cheap to clone.
#[derive(Clone)]
pub struct AppState {
    /// Bound root directory (absolute or as configured).
    pub root: PathBuf,
    /// Bearer token required for all /api requests.
    pub token: Arc<str>,
    /// In-memory event bus (broadcast + replay ring).
    pub events: Arc<EventBus>,
    /// Persistent webhook registrations + delivery.
    pub webhooks: Arc<WebhookStore>,
}

impl AppState {
    /// Create state, ensuring `<root>` and `<root>/.ostore/` exist and
    /// loading persisted webhook registrations.
    pub fn new(root: PathBuf, token: String) -> io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join(".ostore"))?;
        Ok(Self {
            webhooks: Arc::new(WebhookStore::load(&root)),
            root,
            token: token.into(),
            events: Arc::new(EventBus::new()),
        })
    }

    /// Publish an object event to SSE subscribers and webhook targets.
    pub fn publish(&self, kind: &str, path: &str, size: Option<u64>, from: Option<String>) {
        let ev = ObjectEvent {
            kind: kind.to_string(),
            path: path.to_string(),
            size,
            from,
            ts: Utc::now(),
        };
        self.events.publish(ev.clone());
        self.webhooks.deliver(ev);
    }
}
