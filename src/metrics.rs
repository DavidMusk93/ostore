//! Prometheus metrics, hand-rolled on atomics (no exporter dependency).
//!
//! Served as text exposition format 0.0.4 by `GET /api/metrics`. HTTP
//! latency is time-to-response-headers: streamed bodies (downloads, SSE) are
//! accounted by the byte counters and subscriber gauges instead.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::events::ALL_KINDS;

const BUCKETS: [f64; 12] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 10.0, 60.0, 300.0];

#[derive(Default)]
struct Histogram {
    counts: [u64; BUCKETS.len()],
    count: u64,
    sum: f64,
}

impl Histogram {
    fn observe(&mut self, secs: f64) {
        for (i, b) in BUCKETS.iter().enumerate() {
            if secs <= *b {
                self.counts[i] += 1;
            }
        }
        self.count += 1;
        self.sum += secs;
    }
}

/// Process-wide counters and gauges. Cheap to update from any task.
pub struct Metrics {
    started: Instant,
    /// (route, method, status) -> count
    requests: Mutex<BTreeMap<(String, String, u16), u64>>,
    /// (route, method) -> latency histogram
    latency: Mutex<BTreeMap<(String, String), Histogram>>,
    pub http_in_flight: AtomicI64,
    pub upload_bytes: AtomicU64,
    pub download_bytes: AtomicU64,
    pub uploads_in_flight: AtomicI64,
    pub objects_written: AtomicU64,
    pub upload_failures: AtomicU64,
    events: [AtomicU64; ALL_KINDS.len()],
    pub sse_subscribers: AtomicI64,
    pub sse_resets: AtomicU64,
    pub webhook_ok: AtomicU64,
    pub webhook_retries: AtomicU64,
    pub webhook_dropped: AtomicU64,
    pub webhook_in_flight: AtomicI64,
    pub tmp_swept: AtomicU64,
    pub thumbs_hit: AtomicU64,
    pub thumbs_generated: AtomicU64,
    pub thumbs_failed: AtomicU64,
    pub s3_gets: AtomicU64,
    pub s3_heads: AtomicU64,
    pub s3_denied: AtomicU64,
    pub s3_bytes: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            requests: Mutex::new(BTreeMap::new()),
            latency: Mutex::new(BTreeMap::new()),
            http_in_flight: AtomicI64::new(0),
            upload_bytes: AtomicU64::new(0),
            download_bytes: AtomicU64::new(0),
            uploads_in_flight: AtomicI64::new(0),
            objects_written: AtomicU64::new(0),
            upload_failures: AtomicU64::new(0),
            events: Default::default(),
            sse_subscribers: AtomicI64::new(0),
            sse_resets: AtomicU64::new(0),
            webhook_ok: AtomicU64::new(0),
            webhook_retries: AtomicU64::new(0),
            webhook_dropped: AtomicU64::new(0),
            webhook_in_flight: AtomicI64::new(0),
            tmp_swept: AtomicU64::new(0),
            thumbs_hit: AtomicU64::new(0),
            thumbs_generated: AtomicU64::new(0),
            thumbs_failed: AtomicU64::new(0),
            s3_gets: AtomicU64::new(0),
            s3_heads: AtomicU64::new(0),
            s3_denied: AtomicU64::new(0),
            s3_bytes: AtomicU64::new(0),
        }
    }

    /// Milliseconds since process start, used to seed per-request ids.
    pub fn started_ms(&self) -> u64 {
        self.started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
    }

    pub fn event_published(&self, kind: &str) {
        if let Some(i) = ALL_KINDS.iter().position(|k| *k == kind) {
            self.events[i].fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_request(&self, route: &str, method: &str, status: u16, secs: f64) {
        if let Ok(mut m) = self.requests.lock() {
            *m.entry((route.to_string(), method.to_string(), status)).or_insert(0) += 1;
        }
        if let Ok(mut m) = self.latency.lock() {
            m.entry((route.to_string(), method.to_string())).or_default().observe(secs);
        }
    }

    /// Render the Prometheus text exposition.
    pub fn render(&self, st: &AppState) -> String {
        let mut out = String::with_capacity(8 * 1024);
        let o = &mut out;

        gauge(o, "ostore_up", "Always 1 while the daemon serves requests.", 1);
        let _ = writeln!(o, "# HELP ostore_build_info Build metadata.\n# TYPE ostore_build_info gauge");
        let _ = writeln!(o, "ostore_build_info{{version=\"{}\"}} 1", env!("CARGO_PKG_VERSION"));
        gauge_f(o, "ostore_uptime_seconds", "Seconds since process start.", self.started.elapsed().as_secs_f64());

        let _ = writeln!(o, "# HELP ostore_http_requests_total HTTP requests by matched route, method and status.");
        let _ = writeln!(o, "# TYPE ostore_http_requests_total counter");
        if let Ok(m) = self.requests.lock() {
            for ((route, method, status), n) in m.iter() {
                let _ = writeln!(
                    o,
                    "ostore_http_requests_total{{route=\"{}\",method=\"{}\",status=\"{}\"}} {}",
                    esc(route), method, status, n
                );
            }
        }
        let _ = writeln!(o, "# HELP ostore_http_request_duration_seconds Time to response headers.");
        let _ = writeln!(o, "# TYPE ostore_http_request_duration_seconds histogram");
        if let Ok(m) = self.latency.lock() {
            for ((route, method), h) in m.iter() {
                let labels = format!("route=\"{}\",method=\"{}\"", esc(route), method);
                for (i, b) in BUCKETS.iter().enumerate() {
                    let _ = writeln!(o, "ostore_http_request_duration_seconds_bucket{{{labels},le=\"{b}\"}} {}", h.counts[i]);
                }
                let _ = writeln!(o, "ostore_http_request_duration_seconds_bucket{{{labels},le=\"+Inf\"}} {}", h.count);
                let _ = writeln!(o, "ostore_http_request_duration_seconds_sum{{{labels}}} {}", h.sum);
                let _ = writeln!(o, "ostore_http_request_duration_seconds_count{{{labels}}} {}", h.count);
            }
        }
        gauge(o, "ostore_http_requests_in_flight", "Requests currently being handled (until headers).", self.http_in_flight.load(Ordering::Relaxed));

        counter(o, "ostore_upload_bytes_total", "Bytes written to disk by uploads.", self.upload_bytes.load(Ordering::Relaxed));
        counter(o, "ostore_download_bytes_total", "Bytes streamed to clients by downloads.", self.download_bytes.load(Ordering::Relaxed));
        gauge(o, "ostore_uploads_in_flight", "Objects currently being written.", self.uploads_in_flight.load(Ordering::Relaxed));
        counter(o, "ostore_objects_written_total", "Objects committed (atomic rename done).", self.objects_written.load(Ordering::Relaxed));
        counter(o, "ostore_upload_failures_total", "Uploads aborted before commit.", self.upload_failures.load(Ordering::Relaxed));
        counter(o, "ostore_tmp_files_swept_total", "Stale temp files removed at startup.", self.tmp_swept.load(Ordering::Relaxed));

        let _ = writeln!(o, "# HELP ostore_s3_requests_total S3-surface requests by method and outcome.");
        let _ = writeln!(o, "# TYPE ostore_s3_requests_total counter");
        let _ = writeln!(o, "ostore_s3_requests_total{{method=\"GET\"}} {}", self.s3_gets.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_s3_requests_total{{method=\"HEAD\"}} {}", self.s3_heads.load(Ordering::Relaxed));
        counter(o, "ostore_s3_denied_total", "S3-surface requests rejected by signature verification.", self.s3_denied.load(Ordering::Relaxed));
        counter(o, "ostore_s3_bytes_total", "Bytes streamed by the S3 surface.", self.s3_bytes.load(Ordering::Relaxed));

        let _ = writeln!(o, "# HELP ostore_thumbnails_total Thumbnail requests by outcome.");
        let _ = writeln!(o, "# TYPE ostore_thumbnails_total counter");
        let _ = writeln!(o, "ostore_thumbnails_total{{result=\"hit\"}} {}", self.thumbs_hit.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_thumbnails_total{{result=\"generated\"}} {}", self.thumbs_generated.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_thumbnails_total{{result=\"failed\"}} {}", self.thumbs_failed.load(Ordering::Relaxed));

        let _ = writeln!(o, "# HELP ostore_events_published_total Object events published, by kind.");
        let _ = writeln!(o, "# TYPE ostore_events_published_total counter");
        for (i, k) in ALL_KINDS.iter().enumerate() {
            let _ = writeln!(o, "ostore_events_published_total{{kind=\"{k}\"}} {}", self.events[i].load(Ordering::Relaxed));
        }
        gauge(o, "ostore_event_last_id", "Sequence id of the newest event.", st.events.last_id() as i64);
        gauge(o, "ostore_event_ring_len", "Events retained for replay/resume.", st.events.ring_len() as i64);
        gauge(o, "ostore_sse_subscribers", "Open SSE connections.", self.sse_subscribers.load(Ordering::Relaxed));
        counter(o, "ostore_sse_resets_total", "stream.reset frames sent (cursor gap or lagging subscriber).", self.sse_resets.load(Ordering::Relaxed));

        let _ = writeln!(o, "# HELP ostore_webhook_deliveries_total Webhook delivery outcomes.");
        let _ = writeln!(o, "# TYPE ostore_webhook_deliveries_total counter");
        let _ = writeln!(o, "ostore_webhook_deliveries_total{{result=\"ok\"}} {}", self.webhook_ok.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_webhook_deliveries_total{{result=\"retry\"}} {}", self.webhook_retries.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_webhook_deliveries_total{{result=\"dropped\"}} {}", self.webhook_dropped.load(Ordering::Relaxed));
        gauge(o, "ostore_webhook_deliveries_in_flight", "Webhook deliveries pending (incl. backoff).", self.webhook_in_flight.load(Ordering::Relaxed));
        gauge(o, "ostore_webhooks_registered", "Registered webhooks.", st.webhooks.list().len() as i64);

        if let Some((total, avail)) = disk_space(&st.root) {
            gauge(o, "ostore_root_disk_total_bytes", "Filesystem size under the bound root.", total as i64);
            gauge(o, "ostore_root_disk_available_bytes", "Bytes available to ostore under the bound root.", avail as i64);
        }
        out
    }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn counter(o: &mut String, name: &str, help: &str, v: u64) {
    let _ = writeln!(o, "# HELP {name} {help}\n# TYPE {name} counter\n{name} {v}");
}

fn gauge(o: &mut String, name: &str, help: &str, v: i64) {
    let _ = writeln!(o, "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {v}");
}

fn gauge_f(o: &mut String, name: &str, help: &str, v: f64) {
    let _ = writeln!(o, "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {v}");
}

fn disk_space(root: &Path) -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(root.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `s` a writable statvfs.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let frsize = s.f_frsize as u64;
    Some((s.f_blocks as u64 * frsize, s.f_bavail as u64 * frsize))
}

/// Decrements a gauge when dropped — ties a gauge to a scope or a stream.
pub struct GaugeGuard<'a>(&'a AtomicI64);

impl<'a> GaugeGuard<'a> {
    pub fn new(g: &'a AtomicI64) -> Self {
        g.fetch_add(1, Ordering::Relaxed);
        Self(g)
    }
}

impl Drop for GaugeGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Request-accounting middleware (outermost layer, so 401s count too).
pub async fn track(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());
    let method = req.method().as_str().to_string();
    let start = Instant::now();
    let resp = {
        let _g = GaugeGuard::new(&st.metrics.http_in_flight);
        next.run(req).await
    };
    st.metrics.record_request(&route, &method, resp.status().as_u16(), start.elapsed().as_secs_f64());
    resp
}

/// GET /api/metrics
pub async fn handler(State(st): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"))],
        st.metrics.render(&st),
    )
}
