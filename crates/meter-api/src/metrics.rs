//! Minimal Prometheus metrics for the engine HTTP surface — request counters and a latency histogram,
//! rendered at `GET /metrics`. Held in [`AppState`](crate::AppState) (no global recorder), so it is
//! fully testable.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Upper bounds (seconds, inclusive) for the request-latency histogram, sized to the engine's hot-path
/// SLO targets in `docs/SLO.md` (single-digit-millisecond operations) with headroom into seconds.
const LATENCY_BUCKETS_SECONDS: [f64; 12] = [
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Process-wide HTTP request counters and a request-latency histogram.
#[derive(Debug)]
pub struct RequestMetrics {
    total: AtomicU64,
    client_errors: AtomicU64,
    server_errors: AtomicU64,
    /// Per-bucket observation counts; index `i` holds requests whose latency falls in
    /// `(LATENCY_BUCKETS_SECONDS[i - 1], LATENCY_BUCKETS_SECONDS[i]]`. Rendered cumulatively.
    latency_buckets: [AtomicU64; LATENCY_BUCKETS_SECONDS.len()],
    /// Requests slower than the last finite bucket (the `+Inf` tail).
    latency_overflow: AtomicU64,
    /// Sum of observed latencies in microseconds (avoids float atomics; rendered back to seconds).
    latency_sum_micros: AtomicU64,
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self {
            total: AtomicU64::new(0),
            client_errors: AtomicU64::new(0),
            server_errors: AtomicU64::new(0),
            latency_buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            latency_overflow: AtomicU64::new(0),
            latency_sum_micros: AtomicU64::new(0),
        }
    }
}

impl RequestMetrics {
    /// Record one completed request by its HTTP status code.
    pub fn record(&self, status: u16) {
        self.total.fetch_add(1, Ordering::Relaxed);
        match status {
            400..=499 => {
                self.client_errors.fetch_add(1, Ordering::Relaxed);
            }
            500..=599 => {
                self.server_errors.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Record one completed request's latency into the histogram.
    pub fn record_latency(&self, latency: Duration) {
        let seconds = latency.as_secs_f64();
        let micros = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        self.latency_sum_micros.fetch_add(micros, Ordering::Relaxed);
        match LATENCY_BUCKETS_SECONDS
            .iter()
            .position(|&bound| seconds <= bound)
        {
            // `position` indexes into the same array, so `get` always succeeds; the `if let` keeps the
            // hot path panic-free without an `unwrap`.
            Some(index) => {
                if let Some(bucket) = self.latency_buckets.get(index) {
                    bucket.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => {
                self.latency_overflow.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Render the counters and latency histogram in Prometheus text exposition format.
    #[must_use]
    pub fn render(&self) -> String {
        let total = self.total.load(Ordering::Relaxed);
        let client = self.client_errors.load(Ordering::Relaxed);
        let server = self.server_errors.load(Ordering::Relaxed);

        let mut out = format!(
            "# HELP meter_http_requests_total Total HTTP requests served.\n\
             # TYPE meter_http_requests_total counter\n\
             meter_http_requests_total {total}\n\
             # HELP meter_http_request_errors_total HTTP responses with a 4xx or 5xx status.\n\
             # TYPE meter_http_request_errors_total counter\n\
             meter_http_request_errors_total{{class=\"client\"}} {client}\n\
             meter_http_request_errors_total{{class=\"server\"}} {server}\n\
             # HELP meter_http_request_duration_seconds HTTP request latency in seconds.\n\
             # TYPE meter_http_request_duration_seconds histogram\n"
        );

        let mut cumulative = 0_u64;
        for (bound, bucket) in LATENCY_BUCKETS_SECONDS
            .iter()
            .zip(self.latency_buckets.iter())
        {
            cumulative += bucket.load(Ordering::Relaxed);
            out.push_str(&format!(
                "meter_http_request_duration_seconds_bucket{{le=\"{bound}\"}} {cumulative}\n"
            ));
        }
        let observed = cumulative + self.latency_overflow.load(Ordering::Relaxed);
        let sum_seconds = self.latency_sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0;
        out.push_str(&format!(
            "meter_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {observed}\n\
             meter_http_request_duration_seconds_sum {sum_seconds}\n\
             meter_http_request_duration_seconds_count {observed}\n"
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, RequestMetrics};

    #[test]
    fn histogram_buckets_are_cumulative_and_count_every_observation() {
        let metrics = RequestMetrics::default();
        metrics.record_latency(Duration::from_micros(500)); // falls in the 0.001 bucket
        metrics.record_latency(Duration::from_millis(20)); // falls in the 0.025 bucket
        metrics.record_latency(Duration::from_secs(30)); // beyond the last bucket (+Inf tail)

        let body = metrics.render();
        assert!(
            body.contains("meter_http_request_duration_seconds_bucket{le=\"0.001\"} 1"),
            "{body}"
        );
        // Cumulative: the 500µs and the 20ms observations.
        assert!(
            body.contains("meter_http_request_duration_seconds_bucket{le=\"0.025\"} 2"),
            "{body}"
        );
        // +Inf and count include the 30s tail.
        assert!(
            body.contains("meter_http_request_duration_seconds_bucket{le=\"+Inf\"} 3"),
            "{body}"
        );
        assert!(
            body.contains("meter_http_request_duration_seconds_count 3"),
            "{body}"
        );
    }

    #[test]
    fn counters_still_render() {
        let metrics = RequestMetrics::default();
        metrics.record(200);
        metrics.record(404);
        metrics.record(500);
        let body = metrics.render();
        assert!(body.contains("meter_http_requests_total 3"), "{body}");
        assert!(
            body.contains("meter_http_request_errors_total{class=\"client\"} 1"),
            "{body}"
        );
        assert!(
            body.contains("meter_http_request_errors_total{class=\"server\"} 1"),
            "{body}"
        );
    }
}
