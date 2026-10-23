//! Webhook registrations, persisted to `<root>/.ostore/webhooks.json`, with
//! best-effort background delivery (3 retries: 1s / 5s / 15s, then drop the
//! delivery — the registration itself is never removed on failure).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::events::ObjectEvent;

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    /// Event kinds to deliver; `None` (absent) means all events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
}

impl Webhook {
    fn matches(&self, kind: &str) -> bool {
        match &self.events {
            None => true,
            Some(kinds) => kinds.iter().any(|k| k == kind),
        }
    }
}

pub struct WebhookStore {
    file: PathBuf,
    hooks: RwLock<Vec<Webhook>>,
    client: reqwest::Client,
}

impl WebhookStore {
    /// Load registrations from `<root>/.ostore/webhooks.json` (empty if the
    /// file is missing or unreadable).
    pub fn load(root: &Path) -> Self {
        let file = root.join(".ostore").join("webhooks.json");
        let hooks: Vec<Webhook> = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { file, hooks: RwLock::new(hooks), client }
    }

    fn read(&self) -> Vec<Webhook> {
        match self.hooks.read() {
            Ok(g) => g.clone(),
            Err(_) => Vec::new(),
        }
    }

    fn persist(&self) {
        let hooks: Vec<Webhook> = match self.hooks.read() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        let body = match serde_json::to_string_pretty(&hooks) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialize webhooks");
                return;
            }
        };
        let tmp = self.file.with_extension("json.tmp");
        match std::fs::write(&tmp, body) {
            Ok(()) => {
                if let Err(e) = std::fs::rename(&tmp, &self.file) {
                    tracing::warn!(error = %e, "failed to persist webhooks.json");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to write webhooks.json"),
        }
    }

    pub fn add(&self, url: String, events: Option<Vec<String>>) -> Webhook {
        let hook = Webhook { id: new_id(), url, events };
        if let Ok(mut g) = self.hooks.write() {
            g.push(hook.clone());
        }
        self.persist();
        hook
    }

    pub fn list(&self) -> Vec<Webhook> {
        self.read()
    }

    pub fn remove(&self, id: &str) -> bool {
        let removed = match self.hooks.write() {
            Ok(mut g) => {
                let before = g.len();
                g.retain(|h| h.id != id);
                g.len() != before
            }
            Err(_) => false,
        };
        if removed {
            self.persist();
        }
        removed
    }

    /// Queue background delivery of `ev` to every matching registration.
    /// Never blocks the caller.
    pub fn deliver(&self, ev: ObjectEvent) {
        let targets: Vec<Webhook> = self.read().into_iter().filter(|h| h.matches(&ev.kind)).collect();
        if targets.is_empty() {
            return;
        }
        let mut payload = match serde_json::to_value(&ev) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialize event for webhooks");
                return;
            }
        };
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("event".to_string(), serde_json::Value::String(ev.kind.clone()));
        }
        let client = self.client.clone();
        tokio::spawn(async move {
            for hook in targets {
                deliver_one(&client, &hook, &payload).await;
            }
        });
    }
}

async fn deliver_one(client: &reqwest::Client, hook: &Webhook, payload: &serde_json::Value) {
    const BACKOFF_SECS: [u64; 3] = [1, 5, 15];
    for attempt in 0..3 {
        match client.post(&hook.url).json(payload).send().await {
            Ok(resp) if resp.status().is_success() => return,
            Ok(resp) => {
                tracing::warn!(
                    url = %hook.url, status = %resp.status(), attempt,
                    "webhook delivery failed"
                );
            }
            Err(e) => {
                tracing::warn!(url = %hook.url, error = %e, attempt, "webhook delivery error");
            }
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_secs(BACKOFF_SECS[attempt])).await;
        }
    }
    tracing::warn!(url = %hook.url, "webhook delivery dropped after 3 attempts");
}

fn new_id() -> String {
    let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(n.to_be_bytes());
    h.update(nanos.to_be_bytes());
    h.update(std::process::id().to_be_bytes());
    hex::encode(h.finalize())[..16].to_string()
}
