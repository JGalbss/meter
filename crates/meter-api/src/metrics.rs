//! Minimal Prometheus metrics for the engine HTTP surface — request and error counters, rendered at
//! `GET /metrics`. Held in [`AppState`](crate::AppState) (no global recorder), so it is fully
//! testable. Latency histograms can adopt the `metrics` ecosystem later without changing this surface.

use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide HTTP request counters.
#[derive(Debug, Default)]
pub struct RequestMetrics {
    total: AtomicU64,
    client_errors: AtomicU64,
    server_errors: AtomicU64,
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

    /// Render the counters in Prometheus text exposition format.
    #[must_use]
    pub fn render(&self) -> String {
        let total = self.total.load(Ordering::Relaxed);
        let client = self.client_errors.load(Ordering::Relaxed);
        let server = self.server_errors.load(Ordering::Relaxed);
        format!(
            "# HELP meter_http_requests_total Total HTTP requests served.\n\
             # TYPE meter_http_requests_total counter\n\
             meter_http_requests_total {total}\n\
             # HELP meter_http_request_errors_total HTTP responses with a 4xx or 5xx status.\n\
             # TYPE meter_http_request_errors_total counter\n\
             meter_http_request_errors_total{{class=\"client\"}} {client}\n\
             meter_http_request_errors_total{{class=\"server\"}} {server}\n"
        )
    }
}
