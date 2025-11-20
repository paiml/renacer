//! OpenTelemetry OTLP Exporter for Renacer (Sprint 30)
//!
//! Exports syscall traces as OpenTelemetry spans via OTLP protocol.
//!
//! # Architecture
//!
//! - Each traced process gets a root span
//! - Each syscall becomes a child span with attributes
//! - Spans are exported to an OTLP endpoint (Jaeger, Tempo, etc.)
//!
//! # Example
//!
//! ```bash
//! renacer --otlp-endpoint http://localhost:4317 --otlp-service-name my-app -- ./program
//! ```

#[cfg(feature = "otlp")]
use anyhow::Result;
#[cfg(feature = "otlp")]
use opentelemetry::{
    trace::{Span, SpanKind, Status, Tracer, TracerProvider as _},
    KeyValue,
};
#[cfg(feature = "otlp")]
use opentelemetry_sdk::{
    trace::{BatchSpanProcessor, SdkTracerProvider as TracerProvider},
    Resource,
};
#[cfg(feature = "otlp")]
use opentelemetry_otlp::WithExportConfig;

/// Configuration for OTLP exporter
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    /// OTLP endpoint URL (e.g., "http://localhost:4317")
    pub endpoint: String,
    /// Service name for traces
    pub service_name: String,
}

/// Compute block metadata for tracing (Sprint 32)
///
/// Represents a block of statistical computation containing multiple
/// Trueno SIMD operations (e.g., mean, stddev, percentiles).
#[derive(Debug, Clone)]
pub struct ComputeBlock {
    /// Operation name (e.g., "calculate_statistics", "detect_anomalies")
    pub operation: &'static str,
    /// Total duration of the block in microseconds
    pub duration_us: u64,
    /// Number of elements processed
    pub elements: usize,
    /// Whether this block exceeded the slow threshold (>100μs)
    pub is_slow: bool,
}

/// OTLP exporter for syscall traces
#[cfg(feature = "otlp")]
pub struct OtlpExporter {
    _runtime: tokio::runtime::Runtime, // Tokio runtime for async OTLP operations
    _provider: TracerProvider,
    tracer: opentelemetry_sdk::trace::Tracer,
    root_span: Option<opentelemetry_sdk::trace::Span>,
}

#[cfg(feature = "otlp")]
impl OtlpExporter {
    /// Create a new OTLP exporter
    pub fn new(config: OtlpConfig) -> Result<Self> {
        // Create a Tokio runtime for OTLP async operations
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| anyhow::anyhow!("Failed to create Tokio runtime: {}", e))?;

        // Build OTLP exporter within the runtime context
        let (provider, tracer) = runtime.block_on(async {
            // Create OTLP exporter
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(&config.endpoint)
                .build()?;

            // Create batch span processor
            let span_processor = BatchSpanProcessor::builder(exporter).build();

            // Create resource with service name + compute tracing attributes (Sprint 32)
            let resource = Resource::builder()
                .with_service_name(config.service_name.clone())
                .with_attributes(vec![
                    // Static compute tracing attributes at Resource level (Toyota Way: no waste)
                    KeyValue::new("compute.library", "trueno"),
                    KeyValue::new("compute.library.version", "0.4.0"),
                    KeyValue::new("compute.tracing.abstraction", "block_level"),
                ])
                .build();

            // Create tracer provider
            let provider = TracerProvider::builder()
                .with_span_processor(span_processor)
                .with_resource(resource)
                .build();

            // Get tracer
            let tracer = provider.tracer("renacer");

            Ok::<_, anyhow::Error>((provider, tracer))
        })?;

        Ok(OtlpExporter {
            _runtime: runtime,
            _provider: provider,
            tracer,
            root_span: None,
        })
    }

    /// Start a root span for the traced process
    pub fn start_root_span(&mut self, program: &str, pid: i32) {
        let span = self
            .tracer
            .span_builder(format!("process: {}", program))
            .with_kind(SpanKind::Server)
            .with_attributes(vec![
                KeyValue::new("process.command", program.to_string()),
                KeyValue::new("process.pid", pid as i64),
            ])
            .start(&self.tracer);

        self.root_span = Some(span);
    }

    /// Record a syscall as a span
    pub fn record_syscall(
        &self,
        name: &str,
        duration_us: Option<u64>,
        result: i64,
        source_file: Option<&str>,
        source_line: Option<u32>,
    ) {
        let mut span = self
            .tracer
            .span_builder(format!("syscall: {}", name))
            .with_kind(SpanKind::Internal)
            .with_attributes(vec![
                KeyValue::new("syscall.name", name.to_string()),
                KeyValue::new("syscall.result", result),
            ])
            .start(&self.tracer);

        // Add duration if available
        if let Some(duration) = duration_us {
            span.set_attribute(KeyValue::new("syscall.duration_us", duration as i64));
        }

        // Add source location if available
        if let Some(file) = source_file {
            span.set_attribute(KeyValue::new("code.filepath", file.to_string()));
        }
        if let Some(line) = source_line {
            span.set_attribute(KeyValue::new("code.lineno", line as i64));
        }

        // Mark as error if syscall failed
        if result < 0 {
            span.set_status(Status::Error {
                description: format!("syscall failed with code: {}", result).into(),
            });
        }

        span.end();
    }

    /// Record a transpiler decision as a span event (Sprint 31)
    pub fn record_decision(
        &mut self,
        category: &str,
        name: &str,
        result: Option<&str>,
        timestamp_us: u64,
    ) {
        if let Some(ref mut span) = self.root_span {
            // Create attributes for the decision event
            let mut attributes = vec![
                KeyValue::new("decision.category", category.to_string()),
                KeyValue::new("decision.name", name.to_string()),
                KeyValue::new("decision.timestamp_us", timestamp_us as i64),
            ];

            // Add result if available
            if let Some(res) = result {
                attributes.push(KeyValue::new("decision.result", res.to_string()));
            }

            // Add event to the root span
            span.add_event(
                format!("decision: {}::{}", category, name),
                attributes,
            );
        }
    }

    /// Record a compute block (multiple Trueno operations) as a span (Sprint 32)
    ///
    /// This exports a block of statistical computations (e.g., calculate_statistics)
    /// as a single span. Following Toyota Way principles, we trace at the block level
    /// rather than individual SIMD operations to avoid overhead (Muda) and false
    /// observability (Genchi Genbutsu).
    ///
    /// # Arguments
    ///
    /// * `block` - Metadata about the compute block (operation, duration, elements)
    ///
    /// # Adaptive Sampling
    ///
    /// This method should only be called if duration >= threshold (default 100μs).
    /// The caller (trace_compute_block! macro) handles sampling decisions.
    pub fn record_compute_block(&self, block: ComputeBlock) {
        let mut span = self
            .tracer
            .span_builder(format!("compute_block: {}", block.operation))
            .with_kind(SpanKind::Internal)
            .with_attributes(vec![
                // Only dynamic attributes on span (Toyota Way: no attribute explosion)
                KeyValue::new("compute.operation", block.operation.to_string()),
                KeyValue::new("compute.duration_us", block.duration_us as i64),
                KeyValue::new("compute.elements", block.elements as i64),
                KeyValue::new("compute.is_slow", block.is_slow),
            ])
            .start(&self.tracer);

        span.set_status(Status::Ok);
        span.end();
    }

    /// Finish the root span
    pub fn end_root_span(&mut self, exit_code: i32) {
        if let Some(mut span) = self.root_span.take() {
            span.set_attribute(KeyValue::new("process.exit_code", exit_code as i64));

            if exit_code != 0 {
                span.set_status(Status::Error {
                    description: format!("process exited with code: {}", exit_code).into(),
                });
            }

            span.end();
        }
    }

    /// Shutdown the exporter and flush remaining spans
    pub fn shutdown(&mut self) {
        // Span processor automatically flushes on drop
        // But we explicitly end the root span if it exists
        if self.root_span.is_some() {
            self.end_root_span(0);
        }
    }
}

#[cfg(feature = "otlp")]
impl Drop for OtlpExporter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// Stub implementation when OTLP feature is disabled
#[cfg(not(feature = "otlp"))]
pub struct OtlpExporter;

#[cfg(not(feature = "otlp"))]
impl OtlpExporter {
    pub fn new(_config: OtlpConfig) -> Result<Self> {
        anyhow::bail!("OTLP support not compiled in. Enable the 'otlp' feature.");
    }

    pub fn start_root_span(&mut self, _program: &str, _pid: i32) {}

    pub fn record_syscall(
        &self,
        _name: &str,
        _duration_us: Option<u64>,
        _result: i64,
        _source_file: Option<&str>,
        _source_line: Option<u32>,
    ) {
    }

    pub fn record_decision(
        &mut self,
        _category: &str,
        _name: &str,
        _result: Option<&str>,
        _timestamp_us: u64,
    ) {
    }

    pub fn end_root_span(&mut self, _exit_code: i32) {}

    pub fn shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "otlp")]
    fn test_otlp_config_creation() {
        let config = OtlpConfig {
            endpoint: "http://localhost:4317".to_string(),
            service_name: "test-service".to_string(),
        };

        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "test-service");
    }

    #[test]
    #[cfg(not(feature = "otlp"))]
    fn test_otlp_disabled_returns_error() {
        let config = OtlpConfig {
            endpoint: "http://localhost:4317".to_string(),
            service_name: "test".to_string(),
        };

        let result = OtlpExporter::new(config);
        assert!(result.is_err());
    }
}
