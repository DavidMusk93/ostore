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
use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::events::ALL_KINDS;

const BUCKETS: [f64; 12] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 10.0, 60.0, 300.0];

#[derive(Clone, Copy)]
struct CpuSample {
    timestamp: Instant,
    sys_total: u64,
    sys_idle: u64,
    proc_ticks: u64,
    sys_ratio: f64,
    proc_ratio: f64,
}


#[derive(Default, Clone, Debug)]
pub struct NodeSyncStats {
    pub synced_files: u64,
    pub skipped_files: u64,
    pub failed_files: u64,
    pub bytes_transferred: u64,
    pub last_duration_ms: u64,
    pub last_speed_bps: u64,
}

#[derive(Default, Clone, Debug)]
pub struct NodeReplicationStats {
    pub success_count: u64,
    pub failure_count: u64,
    pub bytes_replicated: u64,
    pub last_latency_ms: u64,
}

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
    /// Live events suppressed by a subscriber's `?prefix=` filter.
    pub sse_filtered: AtomicU64,
    /// `POST /api/batch` requests accepted (validation passed).
    pub batch_ok: AtomicU64,
    /// `POST /api/batch` requests rejected with 400 (nothing written).
    pub batch_rejected: AtomicU64,
    /// Objects written / failed inside accepted batches.
    pub batch_objects_written: AtomicU64,
    pub batch_objects_failed: AtomicU64,
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
    pub s3_puts: AtomicU64,
    pub s3_deletes: AtomicU64,
    pub s3_lists: AtomicU64,
    pub s3_denied: AtomicU64,
    pub s3_bytes: AtomicU64,
    cpu_sample: Mutex<Option<CpuSample>>,
    pub sync_in_progress: AtomicI64,
    pub sync_total_synced: AtomicU64,
    pub sync_total_skipped: AtomicU64,
    pub sync_total_failed: AtomicU64,
    pub sync_total_bytes: AtomicU64,
    pub sync_total_files_checked: AtomicU64,
    pub sync_last_duration_ms: AtomicU64,
    pub sync_last_synced: AtomicU64,
    pub sync_last_bytes: AtomicU64,
    pub sync_last_speed_bps: AtomicU64,
    pub sync_last_scan_ms: AtomicU64,
    pub sync_last_probe_ms: AtomicU64,
    pub sync_last_transfer_ms: AtomicU64,
    pub node_sync_stats: Mutex<BTreeMap<String, NodeSyncStats>>,
    pub replication_success: AtomicU64,
    pub replication_failure: AtomicU64,
    pub replication_bytes: AtomicU64,
    pub replication_last_latency_ms: AtomicU64,
    pub node_replication_stats: Mutex<BTreeMap<String, NodeReplicationStats>>,
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
            sse_filtered: AtomicU64::new(0),
            batch_ok: AtomicU64::new(0),
            batch_rejected: AtomicU64::new(0),
            batch_objects_written: AtomicU64::new(0),
            batch_objects_failed: AtomicU64::new(0),
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
            s3_puts: AtomicU64::new(0),
            s3_deletes: AtomicU64::new(0),
            s3_lists: AtomicU64::new(0),
            s3_denied: AtomicU64::new(0),
            s3_bytes: AtomicU64::new(0),
            cpu_sample: Mutex::new(None),
            sync_in_progress: AtomicI64::new(0),
            sync_total_synced: AtomicU64::new(0),
            sync_total_skipped: AtomicU64::new(0),
            sync_total_failed: AtomicU64::new(0),
            sync_total_bytes: AtomicU64::new(0),
            sync_total_files_checked: AtomicU64::new(0),
            sync_last_duration_ms: AtomicU64::new(0),
            sync_last_synced: AtomicU64::new(0),
            sync_last_bytes: AtomicU64::new(0),
            sync_last_speed_bps: AtomicU64::new(0),
            sync_last_scan_ms: AtomicU64::new(0),
            sync_last_probe_ms: AtomicU64::new(0),
            sync_last_transfer_ms: AtomicU64::new(0),
            node_sync_stats: Mutex::new(BTreeMap::new()),
            replication_success: AtomicU64::new(0),
            replication_failure: AtomicU64::new(0),
            replication_bytes: AtomicU64::new(0),
            replication_last_latency_ms: AtomicU64::new(0),
            node_replication_stats: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn record_sync_completion(
        &self,
        checked: usize,
        synced: usize,
        skipped: usize,
        failed: usize,
        bytes_transferred: u64,
        duration_ms: u64,
        speed_bps: f64,
        scan_ms: u64,
        probe_ms: u64,
        transfer_ms: u64,
        node_results: &[crate::cluster::NodeSyncResult],
    ) {
        self.sync_total_synced.fetch_add(synced as u64, Ordering::Relaxed);
        self.sync_total_skipped.fetch_add(skipped as u64, Ordering::Relaxed);
        self.sync_total_failed.fetch_add(failed as u64, Ordering::Relaxed);
        self.sync_total_bytes.fetch_add(bytes_transferred, Ordering::Relaxed);
        self.sync_total_files_checked.fetch_add(checked as u64, Ordering::Relaxed);

        self.sync_last_duration_ms.store(duration_ms, Ordering::Relaxed);
        self.sync_last_synced.store(synced as u64, Ordering::Relaxed);
        self.sync_last_bytes.store(bytes_transferred, Ordering::Relaxed);
        self.sync_last_speed_bps.store(speed_bps as u64, Ordering::Relaxed);
        self.sync_last_scan_ms.store(scan_ms, Ordering::Relaxed);
        self.sync_last_probe_ms.store(probe_ms, Ordering::Relaxed);
        self.sync_last_transfer_ms.store(transfer_ms, Ordering::Relaxed);

        if let Ok(mut lock) = self.node_sync_stats.lock() {
            for nr in node_results {
                let entry = lock.entry(nr.node_id.clone()).or_default();
                entry.synced_files += nr.synced as u64;
                entry.skipped_files += nr.skipped as u64;
                entry.failed_files += nr.errors.len() as u64;
                entry.bytes_transferred += nr.bytes_transferred;
                entry.last_duration_ms = nr.duration_ms;
                entry.last_speed_bps = nr.speed_bytes_per_sec as u64;
            }
        }
    }

    pub fn record_replication(&self, node_id: &str, success: bool, bytes: u64, latency_ms: u64) {
        if success {
            self.replication_success.fetch_add(1, Ordering::Relaxed);
            self.replication_bytes.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.replication_failure.fetch_add(1, Ordering::Relaxed);
        }
        self.replication_last_latency_ms.store(latency_ms, Ordering::Relaxed);

        if let Ok(mut lock) = self.node_replication_stats.lock() {
            let entry = lock.entry(node_id.to_string()).or_default();
            if success {
                entry.success_count += 1;
                entry.bytes_replicated += bytes;
            } else {
                entry.failure_count += 1;
            }
            entry.last_latency_ms = latency_ms;
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

    pub fn sample_cpu(&self) -> (f64, f64) {
        let now = Instant::now();
        let (sys_total, sys_idle) = read_system_cpu_ticks().unwrap_or((0, 0));
        let (_, _, proc_ticks) = read_process_stat().unwrap_or((0.0, 0.0, 0));

        let mut lock = self.cpu_sample.lock().unwrap();
        if let Some(prev) = *lock {
            if now.duration_since(prev.timestamp) < Duration::from_millis(500) {
                return (prev.proc_ratio, prev.sys_ratio);
            }
            let delta_sys_total = sys_total.saturating_sub(prev.sys_total);
            let delta_sys_idle = sys_idle.saturating_sub(prev.sys_idle);
            let delta_proc_ticks = proc_ticks.saturating_sub(prev.proc_ticks);

            let sys_ratio = if delta_sys_total > 0 {
                let non_idle = delta_sys_total.saturating_sub(delta_sys_idle);
                ((non_idle as f64) / (delta_sys_total as f64)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let proc_ratio = if delta_sys_total > 0 {
                ((delta_proc_ticks as f64) / (delta_sys_total as f64)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let new_sample = CpuSample {
                timestamp: now,
                sys_total,
                sys_idle,
                proc_ticks,
                sys_ratio,
                proc_ratio,
            };
            *lock = Some(new_sample);
            (proc_ratio, sys_ratio)
        } else {
            let initial = CpuSample {
                timestamp: now,
                sys_total,
                sys_idle,
                proc_ticks,
                sys_ratio: 0.0,
                proc_ratio: 0.0,
            };
            *lock = Some(initial);
            (0.0, 0.0)
        }
    }

    pub fn current_system_cpu_ratio(&self) -> f64 {
        self.sample_cpu().1
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
    pub async fn render(&self, st: &AppState) -> String {
        let mut out = String::with_capacity(8 * 1024);
        let o = &mut out;

        gauge(o, "ostore_up", "Always 1 while the daemon serves requests.", 1);
        let _ = writeln!(o, "# HELP ostore_build_info Build metadata.\n# TYPE ostore_build_info gauge");
        let _ = writeln!(o, "ostore_build_info{{version=\"{}\"}} 1", env!("CARGO_PKG_VERSION"));
        gauge_f(o, "ostore_uptime_seconds", "Seconds since process start.", self.started.elapsed().as_secs_f64());

        // Process & System CPU & Memory
        let (proc_cpu_ratio, sys_cpu_ratio) = self.sample_cpu();
        gauge_f(o, "ostore_process_cpu_usage_ratio", "Current CPU usage ratio of ostore process (0.0 to 1.0).", proc_cpu_ratio);
        gauge_f(o, "ostore_system_cpu_usage_ratio", "Current host total CPU usage ratio (0.0 to 1.0).", sys_cpu_ratio);

        if let Some((utime_sec, stime_sec, _)) = read_process_stat() {
            gauge_f(o, "ostore_process_cpu_user_seconds_total", "Cumulative user CPU seconds spent by ostore.", utime_sec);
            gauge_f(o, "ostore_process_cpu_system_seconds_total", "Cumulative system CPU seconds spent by ostore.", stime_sec);
            gauge_f(o, "ostore_process_cpu_seconds_total", "Total CPU seconds spent by ostore.", utime_sec + stime_sec);
        }

        if let Some((rss, vms)) = read_process_mem() {
            gauge(o, "ostore_process_resident_memory_bytes", "Resident memory (RSS) in bytes used by ostore.", rss as i64);
            gauge(o, "ostore_process_virtual_memory_bytes", "Virtual memory size (VMS) in bytes used by ostore.", vms as i64);
        }

        if let Some((total, avail, used)) = read_system_mem() {
            gauge(o, "ostore_system_memory_total_bytes", "Total host physical memory in bytes.", total as i64);
            gauge(o, "ostore_system_memory_available_bytes", "Available host memory in bytes.", avail as i64);
            gauge(o, "ostore_system_memory_used_bytes", "Used host memory in bytes.", used as i64);
        }

        // Cluster & Sync Telemetry
        gauge(o, "ostore_sync_in_progress", "Whether cluster fleet synchronization is currently running (1 or 0).", self.sync_in_progress.load(Ordering::Relaxed));
        counter(o, "ostore_sync_total_synced", "Total files successfully synchronized across fleet.", self.sync_total_synced.load(Ordering::Relaxed));
        counter(o, "ostore_sync_total_skipped", "Total files skipped (already matching hash) during fleet sync.", self.sync_total_skipped.load(Ordering::Relaxed));
        counter(o, "ostore_sync_total_failed", "Total files failed during fleet sync.", self.sync_total_failed.load(Ordering::Relaxed));
        counter(o, "ostore_sync_bytes_transferred_total", "Total payload bytes transferred across the fleet during synchronization.", self.sync_total_bytes.load(Ordering::Relaxed));
        counter(o, "ostore_sync_files_checked_total", "Total objects scanned and evaluated across fleet sync operations.", self.sync_total_files_checked.load(Ordering::Relaxed));
        gauge_f(o, "ostore_sync_last_duration_seconds", "Duration of the last fleet sync in seconds.", (self.sync_last_duration_ms.load(Ordering::Relaxed) as f64) / 1000.0);
        gauge(o, "ostore_sync_last_synced_files", "Number of files synced in the last fleet sync run.", self.sync_last_synced.load(Ordering::Relaxed) as i64);
        gauge(o, "ostore_sync_last_bytes_transferred", "Payload bytes transferred during the most recent fleet sync.", self.sync_last_bytes.load(Ordering::Relaxed) as i64);
        gauge_f(o, "ostore_sync_last_speed_bytes_per_second", "Effective network transfer throughput during the last sync in bytes/sec.", self.sync_last_speed_bps.load(Ordering::Relaxed) as f64);

        // Sync Phase Breakdown (Operation Costs)
        let _ = writeln!(o, "# HELP ostore_sync_phase_duration_seconds Operational latency cost breakdown by synchronization phase in seconds.");
        let _ = writeln!(o, "# TYPE ostore_sync_phase_duration_seconds gauge");
        let _ = writeln!(o, "ostore_sync_phase_duration_seconds{{phase=\"local_scan\"}} {:.4}", (self.sync_last_scan_ms.load(Ordering::Relaxed) as f64) / 1000.0);
        let _ = writeln!(o, "ostore_sync_phase_duration_seconds{{phase=\"remote_probe\"}} {:.4}", (self.sync_last_probe_ms.load(Ordering::Relaxed) as f64) / 1000.0);
        let _ = writeln!(o, "ostore_sync_phase_duration_seconds{{phase=\"data_transfer\"}} {:.4}", (self.sync_last_transfer_ms.load(Ordering::Relaxed) as f64) / 1000.0);

        // Per-node Sync Breakdown
        if let Ok(m) = self.node_sync_stats.lock() {
            if !m.is_empty() {
                let _ = writeln!(o, "# HELP ostore_sync_node_files_total Sync file distribution by follower node and status outcome.");
                let _ = writeln!(o, "# TYPE ostore_sync_node_files_total counter");
                for (nid, st) in m.iter() {
                    let _ = writeln!(o, "ostore_sync_node_files_total{{node=\"{}\",status=\"synced\"}} {}", esc(nid), st.synced_files);
                    let _ = writeln!(o, "ostore_sync_node_files_total{{node=\"{}\",status=\"skipped\"}} {}", esc(nid), st.skipped_files);
                    let _ = writeln!(o, "ostore_sync_node_files_total{{node=\"{}\",status=\"failed\"}} {}", esc(nid), st.failed_files);
                }

                let _ = writeln!(o, "# HELP ostore_sync_node_bytes_transferred_total Volume of data in bytes transferred to each follower node.");
                let _ = writeln!(o, "# TYPE ostore_sync_node_bytes_transferred_total counter");
                for (nid, st) in m.iter() {
                    let _ = writeln!(o, "ostore_sync_node_bytes_transferred_total{{node=\"{}\"}} {}", esc(nid), st.bytes_transferred);
                }

                let _ = writeln!(o, "# HELP ostore_sync_node_duration_seconds Time spent synchronizing to each follower node in seconds.");
                let _ = writeln!(o, "# TYPE ostore_sync_node_duration_seconds gauge");
                for (nid, st) in m.iter() {
                    let _ = writeln!(o, "ostore_sync_node_duration_seconds{{node=\"{}\"}} {:.4}", esc(nid), (st.last_duration_ms as f64) / 1000.0);
                }

                let _ = writeln!(o, "# HELP ostore_sync_node_speed_bytes_per_second Effective transfer speed to each follower node.");
                let _ = writeln!(o, "# TYPE ostore_sync_node_speed_bytes_per_second gauge");
                for (nid, st) in m.iter() {
                    let _ = writeln!(o, "ostore_sync_node_speed_bytes_per_second{{node=\"{}\"}} {:.1}", esc(nid), st.last_speed_bps as f64);
                }
            }
        }

        // Live Write-time Replication Telemetry
        counter(o, "ostore_replication_success_total", "Successful real-time writes replicated to follower nodes.", self.replication_success.load(Ordering::Relaxed));
        counter(o, "ostore_replication_failure_total", "Failed real-time writes to follower nodes.", self.replication_failure.load(Ordering::Relaxed));
        counter(o, "ostore_replication_bytes_total", "Total payload bytes replicated on write across follower nodes.", self.replication_bytes.load(Ordering::Relaxed));
        gauge_f(o, "ostore_replication_last_latency_seconds", "Latency in seconds of the last write-time object replication.", (self.replication_last_latency_ms.load(Ordering::Relaxed) as f64) / 1000.0);

        if let Ok(m) = self.node_replication_stats.lock() {
            if !m.is_empty() {
                let _ = writeln!(o, "# HELP ostore_replication_node_events_total Automatic write-time replication calls by target node.");
                let _ = writeln!(o, "# TYPE ostore_replication_node_events_total counter");
                for (nid, st) in m.iter() {
                    let _ = writeln!(o, "ostore_replication_node_events_total{{node=\"{}\",status=\"success\"}} {}", esc(nid), st.success_count);
                    let _ = writeln!(o, "ostore_replication_node_events_total{{node=\"{}\",status=\"failure\"}} {}", esc(nid), st.failure_count);
                }
                let _ = writeln!(o, "# HELP ostore_replication_node_bytes_total Total bytes replicated to each node on write.");
                let _ = writeln!(o, "# TYPE ostore_replication_node_bytes_total counter");
                for (nid, st) in m.iter() {
                    let _ = writeln!(o, "ostore_replication_node_bytes_total{{node=\"{}\"}} {}", esc(nid), st.bytes_replicated);
                }
            }
        }

        // Checksum Cache Hit/Miss Telemetry (Hash Computation Cost)
        let _ = writeln!(o, "# HELP ostore_checksum_cache_requests_total In-memory dual-checksum cache requests and hit ratio.");
        let _ = writeln!(o, "# TYPE ostore_checksum_cache_requests_total counter");
        let _ = writeln!(o, "ostore_checksum_cache_requests_total{{result=\"hit\"}} {}", st.checksum_cache.hits.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_checksum_cache_requests_total{{result=\"miss\"}} {}", st.checksum_cache.misses.load(Ordering::Relaxed));

        if let Some(cluster) = &st.cluster {
            let statuses = cluster.get_node_statuses().await;
            let _ = writeln!(o, "# HELP ostore_cluster_node_healthy Health status of cluster node (1 = healthy, 0 = down).");
            let _ = writeln!(o, "# TYPE ostore_cluster_node_healthy gauge");
            for node in &statuses {
                let _ = writeln!(o, "ostore_cluster_node_healthy{{node=\"{}\"}} {}", esc(&node.id), if node.healthy { 1 } else { 0 });
            }

            let _ = writeln!(o, "# HELP ostore_cluster_node_rtt_seconds Round-trip ping time to cluster node in seconds.");
            let _ = writeln!(o, "# TYPE ostore_cluster_node_rtt_seconds gauge");
            for node in &statuses {
                let rtt_secs = node.rtt_ms.map(|ms| ms as f64 / 1000.0).unwrap_or(0.0);
                let _ = writeln!(o, "ostore_cluster_node_rtt_seconds{{node=\"{}\"}} {}", esc(&node.id), rtt_secs);
            }
        }

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
        let _ = writeln!(o, "ostore_s3_requests_total{{method=\"PUT\"}} {}", self.s3_puts.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_s3_requests_total{{method=\"DELETE\"}} {}", self.s3_deletes.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_s3_requests_total{{method=\"LIST\"}} {}", self.s3_lists.load(Ordering::Relaxed));
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
        counter(o, "ostore_sse_filtered_total", "Live events not sent to a subscriber because of its ?prefix= filter.", self.sse_filtered.load(Ordering::Relaxed));

        let _ = writeln!(o, "# HELP ostore_batch_requests_total POST /api/batch requests by outcome.");
        let _ = writeln!(o, "# TYPE ostore_batch_requests_total counter");
        let _ = writeln!(o, "ostore_batch_requests_total{{result=\"ok\"}} {}", self.batch_ok.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_batch_requests_total{{result=\"rejected\"}} {}", self.batch_rejected.load(Ordering::Relaxed));
        let _ = writeln!(o, "# HELP ostore_batch_objects_total Objects in accepted batches by outcome.");
        let _ = writeln!(o, "# TYPE ostore_batch_objects_total counter");
        let _ = writeln!(o, "ostore_batch_objects_total{{result=\"written\"}} {}", self.batch_objects_written.load(Ordering::Relaxed));
        let _ = writeln!(o, "ostore_batch_objects_total{{result=\"failed\"}} {}", self.batch_objects_failed.load(Ordering::Relaxed));

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

pub fn disk_space(root: &Path) -> Option<(u64, u64)> {
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

pub fn read_process_stat() -> Option<(f64, f64, u64)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rparen = stat.rfind(')')?;
    let rest = &stat[rparen + 1..];
    let mut parts = rest.split_whitespace();
    let utime_str = parts.nth(11)?;
    let stime_str = parts.next()?;
    let utime: u64 = utime_str.parse().ok()?;
    let stime: u64 = stime_str.parse().ok()?;
    let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    let tck = if clk_tck > 0 { clk_tck as f64 } else { 100.0 };
    Some(((utime as f64) / tck, (stime as f64) / tck, utime + stime))
}

pub fn read_process_mem() -> Option<(u64, u64)> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let mut parts = statm.split_whitespace();
    let size_pages: u64 = parts.next()?.parse().ok()?;
    let res_pages: u64 = parts.next()?.parse().ok()?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let ps = if page_size > 0 { page_size as u64 } else { 4096 };
    Some((res_pages * ps, size_pages * ps))
}

pub fn read_system_mem() -> Option<(u64, u64, u64)> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest.trim().split_whitespace().next()?.parse().ok()?;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest.trim().split_whitespace().next()?.parse().ok()?;
        }
    }
    if total_kb == 0 {
        return None;
    }
    let total = total_kb * 1024;
    let avail = avail_kb * 1024;
    let used = total.saturating_sub(avail);
    Some((total, avail, used))
}

pub fn read_system_cpu_ticks() -> Option<(u64, u64)> {
    let content = std::fs::read_to_string("/proc/stat").ok()?;
    let line = content.lines().next()?;
    if !line.starts_with("cpu ") {
        return None;
    }
    let mut parts = line.split_whitespace();
    parts.next(); // skip "cpu"
    let user: u64 = parts.next()?.parse().ok()?;
    let nice: u64 = parts.next()?.parse().ok()?;
    let system: u64 = parts.next()?.parse().ok()?;
    let idle: u64 = parts.next()?.parse().ok()?;
    let iowait: u64 = parts.next()?.parse().ok()?;
    let irq: u64 = parts.next()?.parse().ok()?;
    let softirq: u64 = parts.next()?.parse().ok()?;
    let steal: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let total = user + nice + system + idle + iowait + irq + softirq + steal;
    let idle_ticks = idle + iowait;
    Some((total, idle_ticks))
}

/// GET /api/metrics
pub async fn handler(State(st): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"))],
        st.metrics.render(&st).await,
    )
}
