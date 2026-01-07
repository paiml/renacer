//! Metrics collection module for Renacer observability platform
//!
//! Sprint 56: Core Metrics + OTLP Export
//!
//! Provides Counter, Gauge, and Histogram metric types with:
//! - Atomic operations for thread-safety
//! - SIMD-accelerated histogram bucket search (via trueno)
//! - Label validation to prevent cardinality explosion
//! - OTLP-compatible export
//!
//! # Peer-Reviewed Foundations
//! - Prometheus (SoundCloud 2012): Counter/Gauge/Histogram types
//! - Linux perf_event (2008): Per-CPU ring buffers
//! - Dapper (Google 2010): Low-overhead instrumentation
//!
//! # Example
//! ```ignore
//! use renacer::metrics::{Counter, Gauge, Histogram, Registry};
//!
//! let registry = Registry::new();
//!
//! let counter = registry.counter("syscalls_total", &[("type", "read")]);
//! counter.inc();
//!
//! let gauge = registry.gauge("active_traces", &[]);
//! gauge.set(42);
//!
//! let histogram = registry.histogram("syscall_duration_seconds", &[]);
//! histogram.observe(0.025);
//! ```

mod counter;
mod gauge;
mod histogram;
mod labels;
mod registry;

pub use counter::Counter;
pub use gauge::Gauge;
pub use histogram::{Histogram, DEFAULT_BUCKETS, PROMETHEUS_BUCKETS};
pub use labels::{LabelPair, LabelValidator, Labels};
pub use registry::{Registry, RegistryError};

/// Metric descriptor with name and labels
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricDesc {
    pub name: String,
    pub labels: Labels,
}

impl MetricDesc {
    pub fn new(name: impl Into<String>, labels: Labels) -> Self {
        Self {
            name: name.into(),
            labels,
        }
    }

    /// Generate unique key for this metric (name + sorted labels)
    pub fn key(&self) -> String {
        let mut key = self.name.clone();
        let mut sorted_labels: Vec<_> = self.labels.iter().collect();
        sorted_labels.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_labels {
            key.push_str(&format!("|{}={}", k, v));
        }
        key
    }
}

/// Metric type enumeration for registry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

impl std::fmt::Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricType::Counter => write!(f, "counter"),
            MetricType::Gauge => write!(f, "gauge"),
            MetricType::Histogram => write!(f, "histogram"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_desc_key() {
        let labels: Labels = [("method", "GET"), ("status", "200")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let desc = MetricDesc::new("http_requests", labels);
        assert_eq!(desc.key(), "http_requests|method=GET|status=200");
    }

    #[test]
    fn test_metric_desc_key_empty_labels() {
        let desc = MetricDesc::new("simple_metric", Labels::new());
        assert_eq!(desc.key(), "simple_metric");
    }
}
