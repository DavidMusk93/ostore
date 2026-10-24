//! GET /api/events — Server-Sent Events stream of object events.
//!
//! Cursor semantics (first match wins):
//!   ?since=<id>         resume after event <id>
//!   Last-Event-ID: <id> same, sent automatically by a reconnecting browser
//!   ?replay=<n>         no cursor: replay the last n events (default 100)
//! A cursor that can't be honored (fell out of the ring, previous process)
//! yields one `stream.reset` frame — the client must re-list. A subscriber
//! that lags behind the live broadcast gets the same frame.
//!
//! Frame format:
//!   id: <id>
//!   event: <type>
//!   data: {"id","path","type","size"?,"from"?,"ts"}

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use std::sync::atomic::Ordering;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::AppState;
use crate::events::{DEFAULT_REPLAY, ObjectEvent, RING_CAP, Replay, STREAM_RESET};
use crate::metrics::Metrics;

#[derive(Deserialize, Default)]
pub struct EventsQuery {
    since: Option<u64>,
    replay: Option<usize>,
}

fn to_sse(ev: &ObjectEvent) -> Event {
    let data = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
    Event::default().id(ev.id.to_string()).event(ev.kind.clone()).data(data)
}

fn reset(metrics: &Metrics, reason: &str) -> Event {
    metrics.sse_resets.fetch_add(1, Ordering::Relaxed);
    Event::default()
        .event(STREAM_RESET)
        .data(serde_json::json!({ "reason": reason }).to_string())
}

pub async fn sse(
    State(st): State<AppState>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let since = q.since.or_else(|| {
        headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
    });
    let replay_n = q.replay.unwrap_or(DEFAULT_REPLAY).min(RING_CAP);
    let (rx, replay) = st.events.subscribe(since, replay_n);

    let head: Vec<Result<Event, Infallible>> = match replay {
        Replay::Events(evs) => evs.iter().map(|ev| Ok(to_sse(ev))).collect(),
        Replay::Reset => vec![Ok(reset(&st.metrics, "cursor"))],
    };

    let metrics = st.metrics.clone();
    let live = BroadcastStream::new(rx).filter_map(move |r| {
        let metrics = metrics.clone();
        async move {
            match r {
                Ok(ev) => Some(Ok(to_sse(&ev))),
                Err(BroadcastStreamRecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE subscriber lagged, sending stream.reset");
                    Some(Ok(reset(&metrics, "lagged")))
                }
            }
        }
    });

    // The guard lives inside the stream, so the subscriber gauge drops when
    // the client disconnects or shutdown ends the stream.
    let metrics = st.metrics.clone();
    metrics.sse_subscribers.fetch_add(1, Ordering::Relaxed);
    let guard = SubscriberGuard(metrics);
    let stream = stream::iter(head)
        .chain(live)
        .take_until(st.shutdown.clone().cancelled_owned())
        .map(move |item| {
            let _ = &guard;
            item
        });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

struct SubscriberGuard(std::sync::Arc<Metrics>);

impl Drop for SubscriberGuard {
    fn drop(&mut self) {
        self.0.sse_subscribers.fetch_sub(1, Ordering::Relaxed);
    }
}
