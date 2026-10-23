//! GET /api/events — Server-Sent Events stream of object events.
//!
//! On connect, the last [`RING_CAP`] events are replayed from the in-memory
//! ring buffer, followed by live events from the broadcast channel.
//! Frame format:
//!   event: <type>
//!   data: {"path","size"?,"from"?,"ts"}

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use futures::stream::{self, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::AppState;
use crate::events::ObjectEvent;

fn to_sse(ev: &ObjectEvent) -> Event {
    let data = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
    Event::default().event(ev.kind.clone()).data(data)
}

pub async fn sse(State(st): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe before snapshotting so nothing is lost between the two; an
    // event published in between may be delivered twice, which consumers
    // should tolerate (events are idempotent notifications).
    let rx = st.events.subscribe();
    let replay: Vec<Result<Event, Infallible>> =
        st.events.snapshot().iter().map(|ev| Ok(to_sse(ev))).collect();
    let live = BroadcastStream::new(rx).filter_map(|r| async move {
        match r {
            Ok(ev) => Some(Ok(to_sse(&ev))),
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "SSE subscriber lagged, dropping events");
                None
            }
        }
    });
    Sse::new(stream::iter(replay).chain(live))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}
