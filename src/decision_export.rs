//! Decision Trace OTLP Export (Sprint 49 - Ticket #19)
//!
//! Implements OTLP/gRPC export for decision traces to entrenar.
//!
//! # Overview
//!
//! This module provides:
//! - Configuration for OTLP export with retry logic
//! - Queue-based export with overflow handling
//! - Batch export with configurable size
//! - Auth token support
//!
//! # Example
//!
//! ```no_run
//! use renacer::decision_export::{DecisionExportConfig, DecisionExporter};
//! use renacer::decision_trace::DecisionTrace;
//!
//! let config = DecisionExportConfig::default();
//! let mut exporter = DecisionExporter::new(config).unwrap();
//!
//! // Queue decisions for export
//! let decision = DecisionTrace {
//!     timestamp_us: 1000,
//!     category: "TypeMapping".to_string(),
//!     name: "test".to_string(),
//!     input: serde_json::json!({}),
//!     result: None,
//!     source_location: None,
//!     decision_id: Some(1),
//! };
//! exporter.queue(decision);
//!
//! // Export batch
//! let batch = exporter.next_batch();
//! ```
//!
//! # Reference
//!
//! paiml/depyler docs/specifications/decision-traces-signal-spec.md Section 5.1, 10.2

use crate::decision_trace::DecisionTrace;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Configuration for retry behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
    /// Maximum queue size for offline resilience
    pub queue_size: usize,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff_ms: 100,
            max_backoff_ms: 30000,
            queue_size: 10000,
        }
    }
}

impl RetryConfig {
    /// Calculate backoff for a given attempt using exponential backoff
    ///
    /// # Arguments
    ///
    /// * `attempt` - The attempt number (0-indexed)
    ///
    /// # Returns
    ///
    /// Backoff duration in milliseconds, capped at max_backoff_ms
    pub fn backoff_ms(&self, attempt: u32) -> u64 {
        let backoff = self.initial_backoff_ms.saturating_mul(1 << attempt.min(16));
        backoff.min(self.max_backoff_ms)
    }
}

/// Configuration for decision trace export
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DecisionExportConfig {
    /// OTLP endpoint URL
    #[serde(default = "default_endpoint")]
    pub otlp_endpoint: String,

    /// Batch size for export
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Flush interval in milliseconds
    #[serde(default = "default_flush_interval")]
    pub flush_interval_ms: u64,

    /// Maximum queue size
    #[serde(default = "default_queue_size")]
    pub queue_size: usize,

    /// Optional auth token for secure export
    #[serde(default)]
    pub auth_token: Option<String>,

    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfig,
}

fn default_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_batch_size() -> usize {
    100
}

fn default_flush_interval() -> u64 {
    1000
}

fn default_queue_size() -> usize {
    10000
}

impl Default for DecisionExportConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: default_endpoint(),
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval(),
            queue_size: default_queue_size(),
            auth_token: None,
            retry: RetryConfig::default(),
        }
    }
}

impl DecisionExportConfig {
    /// Create config from environment variables
    ///
    /// Looks for:
    /// - RENACER_OTLP_ENDPOINT
    /// - RENACER_AUTH_TOKEN
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(endpoint) = std::env::var("RENACER_OTLP_ENDPOINT") {
            config.otlp_endpoint = endpoint;
        }

        if let Ok(token) = std::env::var("RENACER_AUTH_TOKEN") {
            config.auth_token = Some(token);
        }

        config
    }
}

/// Statistics for export monitoring
#[derive(Debug, Clone, Default)]
pub struct ExportStats {
    /// Total decisions queued
    pub decisions_queued: u64,
    /// Total decisions successfully exported
    pub decisions_exported: u64,
    /// Total decisions dropped (queue overflow)
    pub decisions_dropped: u64,
    /// Total batches sent
    pub batches_sent: u64,
    /// Total batches failed
    pub batches_failed: u64,
    /// Total retry attempts
    pub retry_attempts: u64,
}

/// Decision trace exporter
pub struct DecisionExporter {
    config: DecisionExportConfig,
    queue: VecDeque<DecisionTrace>,
    stats: ExportStats,
}

impl DecisionExporter {
    /// Create a new exporter from configuration
    pub fn new(config: DecisionExportConfig) -> Result<Self, String> {
        Ok(Self {
            config,
            queue: VecDeque::new(),
            stats: ExportStats::default(),
        })
    }

    /// Queue a decision for export
    ///
    /// If the queue is full, the oldest decision is dropped.
    pub fn queue(&mut self, decision: DecisionTrace) {
        // Check if we need to drop oldest
        if self.queue.len() >= self.config.queue_size {
            self.queue.pop_front();
            self.stats.decisions_dropped += 1;
        }

        self.queue.push_back(decision);
        self.stats.decisions_queued += 1;
    }

    /// Queue multiple decisions for export
    pub fn queue_all(&mut self, decisions: Vec<DecisionTrace>) {
        for decision in decisions {
            self.queue(decision);
        }
    }

    /// Get the current queue length
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get the next batch of decisions for export
    ///
    /// Removes decisions from the queue up to batch_size.
    pub fn next_batch(&mut self) -> Vec<DecisionTrace> {
        let batch_size = self.config.batch_size.min(self.queue.len());
        let mut batch = Vec::with_capacity(batch_size);

        for _ in 0..batch_size {
            if let Some(decision) = self.queue.pop_front() {
                batch.push(decision);
            }
        }

        batch
    }

    /// Get current statistics
    pub fn stats(&self) -> &ExportStats {
        &self.stats
    }

    /// Record a successful batch export
    pub fn record_batch_success(&mut self, count: usize) {
        self.stats.decisions_exported += count as u64;
        self.stats.batches_sent += 1;
    }

    /// Record a failed batch export
    pub fn record_batch_failure(&mut self) {
        self.stats.batches_failed += 1;
    }

    /// Record a retry attempt
    pub fn record_retry(&mut self) {
        self.stats.retry_attempts += 1;
    }

    /// Get the OTLP endpoint
    pub fn endpoint(&self) -> &str {
        &self.config.otlp_endpoint
    }

    /// Get the auth token if configured
    pub fn auth_token(&self) -> Option<&str> {
        self.config.auth_token.as_deref()
    }

    /// Get the flush interval as Duration
    pub fn flush_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.config.flush_interval_ms)
    }

    /// Get retry config
    pub fn retry_config(&self) -> &RetryConfig {
        &self.config.retry
    }
}

/// Print statistics for a msgpack file (CLI support)
pub fn print_stats(path: &std::path::Path) -> Result<(), String> {
    use crate::decision_trace::read_decisions_from_msgpack;
    use std::collections::HashMap;

    let decisions = read_decisions_from_msgpack(path)?;

    println!("Decision Trace Statistics for: {:?}", path);
    println!("========================================");
    println!("Total decisions: {}", decisions.len());
    println!();

    // Count by category
    let mut by_category: HashMap<String, usize> = HashMap::new();
    for decision in &decisions {
        *by_category.entry(decision.category.clone()).or_default() += 1;
    }

    println!("By category:");
    let mut categories: Vec<_> = by_category.into_iter().collect();
    categories.sort_by(|a, b| b.1.cmp(&a.1));
    for (category, count) in categories {
        println!("  {}: {}", category, count);
    }
    println!();

    // Time range
    if !decisions.is_empty() {
        let min_ts = decisions.iter().map(|d| d.timestamp_us).min().unwrap();
        let max_ts = decisions.iter().map(|d| d.timestamp_us).max().unwrap();
        let duration_ms = (max_ts - min_ts) / 1000;
        println!("Time range: {} ms", duration_ms);
        println!(
            "Rate: {:.1} decisions/sec",
            if duration_ms > 0 {
                (decisions.len() as f64) / (duration_ms as f64 / 1000.0)
            } else {
                0.0
            }
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_backoff_ms, 100);
    }

    #[test]
    fn test_retry_backoff() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_backoff_ms: 100,
            max_backoff_ms: 30000,
            queue_size: 10000,
        };

        assert_eq!(config.backoff_ms(0), 100);
        assert_eq!(config.backoff_ms(1), 200);
        assert_eq!(config.backoff_ms(2), 400);
    }

    #[test]
    fn test_export_config_default() {
        let config = DecisionExportConfig::default();
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert_eq!(config.batch_size, 100);
    }

    #[test]
    fn test_exporter_queue() {
        let config = DecisionExportConfig::default();
        let mut exporter = DecisionExporter::new(config).unwrap();

        let decision = DecisionTrace {
            timestamp_us: 1000,
            category: "Test".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({}),
            result: None,
            source_location: None,
            decision_id: Some(1),
        };

        exporter.queue(decision);
        assert_eq!(exporter.queue_len(), 1);
    }
}
