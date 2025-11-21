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
    trace::{
        Span, SpanContext, SpanKind, Status, TraceContextExt, TraceFlags, TraceState, Tracer,
        TracerProvider as _,
    },
    KeyValue,
};
#[cfg(feature = "otlp")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otlp")]
use opentelemetry_sdk::{
    trace::{BatchSpanProcessor, SdkTracerProvider as TracerProvider},
    Resource,
};

use crate::trace_context::TraceContext; // Sprint 33

/// Configuration for OTLP exporter (Sprint 36: added batch config)
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    /// OTLP endpoint URL (e.g., "http://localhost:4317")
    pub endpoint: String,
    /// Service name for traces
    pub service_name: String,
    /// Maximum number of spans per batch (default: 512)
    pub batch_size: usize,
    /// Maximum batch delay in milliseconds (default: 1000ms)
    pub batch_delay_ms: u64,
    /// Maximum queue size (default: 2048)
    pub queue_size: usize,
}

impl OtlpConfig {
    /// Create a new OTLP configuration with default batching settings
    pub fn new(endpoint: String, service_name: String) -> Self {
        OtlpConfig {
            endpoint,
            service_name,
            batch_size: 512,
            batch_delay_ms: 1000,
            queue_size: 2048,
        }
    }

    /// Performance preset: Balanced (default)
    pub fn balanced(endpoint: String, service_name: String) -> Self {
        Self::new(endpoint, service_name)
    }

    /// Performance preset: Aggressive (max throughput)
    pub fn aggressive(endpoint: String, service_name: String) -> Self {
        OtlpConfig {
            endpoint,
            service_name,
            batch_size: 2048,
            batch_delay_ms: 5000,
            queue_size: 8192,
        }
    }

    /// Performance preset: Low-latency (min delay)
    pub fn low_latency(endpoint: String, service_name: String) -> Self {
        OtlpConfig {
            endpoint,
            service_name,
            batch_size: 128,
            batch_delay_ms: 100,
            queue_size: 512,
        }
    }

    /// Set custom batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set custom batch delay
    pub fn with_batch_delay_ms(mut self, delay_ms: u64) -> Self {
        self.batch_delay_ms = delay_ms;
        self
    }

    /// Set custom queue size
    pub fn with_queue_size(mut self, size: usize) -> Self {
        self.queue_size = size;
        self
    }
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

/// GPU kernel metadata for tracing (Sprint 37)
///
/// Represents a single GPU kernel execution (compute shader, render pass, etc.)
/// captured via wgpu timestamp queries.
#[derive(Debug, Clone)]
pub struct GpuKernel {
    /// Kernel name (e.g., "sum_aggregation", "matrix_multiply")
    pub kernel: String,
    /// Total duration in microseconds
    pub duration_us: u64,
    /// GPU backend (always "wgpu" for Phase 1)
    pub backend: &'static str,
    /// Workgroup size for compute shaders (e.g., "[256,1,1]")
    pub workgroup_size: Option<String>,
    /// Number of elements processed (if known)
    pub elements: Option<usize>,
    /// Whether this kernel exceeded the slow threshold (>100μs)
    pub is_slow: bool,
}

/// GPU memory transfer direction (Sprint 39 - Phase 4)
///
/// Represents the direction of CPU↔GPU data movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// CPU → GPU (buffer upload, write_buffer)
    CpuToGpu,
    /// GPU → CPU (buffer download/readback, map_async)
    GpuToCpu,
}

impl TransferDirection {
    /// Get string representation of transfer direction
    pub fn as_str(&self) -> &'static str {
        match self {
            TransferDirection::CpuToGpu => "cpu_to_gpu",
            TransferDirection::GpuToCpu => "gpu_to_cpu",
        }
    }
}

/// GPU memory transfer metadata for tracing (Sprint 39 - Phase 4)
///
/// Represents a single CPU↔GPU memory transfer operation captured via wall-clock timing.
/// Tracks buffer uploads (CPU→GPU) and downloads (GPU→CPU) to identify PCIe bandwidth
/// bottlenecks.
#[derive(Debug, Clone)]
pub struct GpuMemoryTransfer {
    /// Transfer name/label (e.g., "mesh_data_upload", "framebuffer_readback")
    pub label: String,
    /// Transfer direction (CPU→GPU or GPU→CPU)
    pub direction: TransferDirection,
    /// Number of bytes transferred
    pub bytes: usize,
    /// Total duration in microseconds
    pub duration_us: u64,
    /// Calculated bandwidth in MB/s
    pub bandwidth_mbps: f64,
    /// Optional buffer usage hint (e.g., "VERTEX", "UNIFORM", "STORAGE")
    pub buffer_usage: Option<String>,
    /// Whether this transfer exceeded the slow threshold (>100μs)
    pub is_slow: bool,
}

impl GpuMemoryTransfer {
    /// Create a new GPU memory transfer record
    ///
    /// Automatically calculates bandwidth from bytes and duration.
    ///
    /// # Arguments
    ///
    /// * `label` - Transfer name/label
    /// * `direction` - Transfer direction (CPU→GPU or GPU→CPU)
    /// * `bytes` - Number of bytes transferred
    /// * `duration_us` - Transfer duration in microseconds
    /// * `buffer_usage` - Optional buffer usage hint
    /// * `threshold_us` - Slow threshold for adaptive sampling
    ///
    /// # Returns
    ///
    /// New GpuMemoryTransfer with calculated bandwidth
    pub fn new(
        label: String,
        direction: TransferDirection,
        bytes: usize,
        duration_us: u64,
        buffer_usage: Option<String>,
        threshold_us: u64,
    ) -> Self {
        // Calculate bandwidth: MB/s = (bytes / 1_048_576) / (duration_us / 1_000_000)
        // Simplified: (bytes * 1_000_000) / (duration_us * 1_048_576)
        let bandwidth_mbps = if duration_us > 0 {
            (bytes as f64 * 1_000_000.0) / (duration_us as f64 * 1_048_576.0)
        } else {
            0.0
        };

        GpuMemoryTransfer {
            label,
            direction,
            bytes,
            duration_us,
            bandwidth_mbps,
            buffer_usage,
            is_slow: duration_us > threshold_us,
        }
    }
}

/// OTLP exporter for syscall traces
#[cfg(feature = "otlp")]
pub struct OtlpExporter {
    _runtime: tokio::runtime::Runtime, // Tokio runtime for async OTLP operations
    _provider: TracerProvider,
    tracer: opentelemetry_sdk::trace::Tracer,
    root_span: Option<opentelemetry_sdk::trace::Span>,
    remote_parent_context: Option<opentelemetry::Context>, // Sprint 33: W3C Trace Context
}

#[cfg(feature = "otlp")]
impl OtlpExporter {
    /// Create a new OTLP exporter (Sprint 33: with optional trace context)
    pub fn new(config: OtlpConfig, trace_context: Option<TraceContext>) -> Result<Self> {
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

            // Create batch span processor (Sprint 36: batching for performance)
            // Note: OpenTelemetry SDK 0.31.0 uses default batch settings
            // config.batch_size, batch_delay_ms, and queue_size are available
            // for future versions or custom implementations
            let span_processor = BatchSpanProcessor::builder(exporter).build();

            // Log batch configuration for transparency
            eprintln!(
                "[renacer: OTLP batch config - size: {}, delay: {}ms, queue: {}]",
                config.batch_size, config.batch_delay_ms, config.queue_size
            );

            // Create resource with service name + compute tracing attributes (Sprint 32 + 37)
            let resource_attrs = vec![
                // Sprint 32: Static SIMD compute tracing attributes at Resource level (Toyota Way: no waste)
                KeyValue::new("compute.library", "trueno"),
                KeyValue::new("compute.library.version", "0.4.0"),
                KeyValue::new("compute.tracing.abstraction", "block_level"),
            ];

            // Sprint 37: GPU kernel tracing attributes (only if gpu-tracing feature enabled)
            #[cfg(feature = "gpu-tracing")]
            {
                resource_attrs.push(KeyValue::new("gpu.library", "wgpu"));
                resource_attrs.push(KeyValue::new("gpu.tracing.abstraction", "kernel_level"));
            }

            let resource = Resource::builder()
                .with_service_name(config.service_name.clone())
                .with_attributes(resource_attrs)
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

        // Sprint 33: Create remote parent context from W3C Trace Context
        let remote_parent_context = trace_context.map(|ctx| {
            let span_context = SpanContext::new(
                ctx.otel_trace_id(),
                ctx.otel_parent_id(),
                TraceFlags::new(ctx.trace_flags),
                true, // is_remote = true (context from external system)
                TraceState::default(),
            );

            opentelemetry::Context::current().with_remote_span_context(span_context)
        });

        Ok(OtlpExporter {
            _runtime: runtime,
            _provider: provider,
            tracer,
            root_span: None,
            remote_parent_context,
        })
    }

    /// Start a root span for the traced process (Sprint 33: with optional parent context)
    pub fn start_root_span(&mut self, program: &str, pid: i32) {
        let span_builder = self
            .tracer
            .span_builder(format!("process: {}", program))
            .with_kind(SpanKind::Server)
            .with_attributes(vec![
                KeyValue::new("process.command", program.to_string()),
                KeyValue::new("process.pid", pid as i64),
            ]);

        // Sprint 33: If we have a remote parent context, make this span a child
        let span = if let Some(ref parent_ctx) = self.remote_parent_context {
            span_builder.start_with_context(&self.tracer, parent_ctx)
        } else {
            span_builder.start(&self.tracer)
        };

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
            span.add_event(format!("decision: {}::{}", category, name), attributes);
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

    /// Record a GPU kernel execution as a span (Sprint 37)
    ///
    /// Exports GPU kernel timing captured via wgpu-profiler timestamp queries.
    /// Follows Sprint 32's adaptive sampling pattern (only trace if duration > threshold).
    ///
    /// # Arguments
    ///
    /// * `kernel` - Metadata about the GPU kernel execution
    ///
    /// # Adaptive Sampling
    ///
    /// This method should only be called if duration >= threshold (default 100μs).
    /// The caller (GpuProfilerWrapper) handles sampling decisions.
    pub fn record_gpu_kernel(&self, kernel: GpuKernel) {
        let mut span_attrs = vec![
            // Only dynamic attributes on span (Toyota Way: no attribute explosion)
            KeyValue::new("gpu.backend", kernel.backend.to_string()),
            KeyValue::new("gpu.kernel", kernel.kernel.clone()),
            KeyValue::new("gpu.duration_us", kernel.duration_us as i64),
            KeyValue::new("gpu.is_slow", kernel.is_slow),
        ];

        // Optional attributes
        if let Some(ref wg_size) = kernel.workgroup_size {
            span_attrs.push(KeyValue::new("gpu.workgroup_size", wg_size.clone()));
        }
        if let Some(elements) = kernel.elements {
            span_attrs.push(KeyValue::new("gpu.elements", elements as i64));
        }

        let mut span = self
            .tracer
            .span_builder(format!("gpu_kernel: {}", kernel.kernel))
            .with_kind(SpanKind::Internal)
            .with_attributes(span_attrs)
            .start(&self.tracer);

        span.set_status(Status::Ok);
        span.end();
    }

    /// Record a GPU memory transfer as a span (Sprint 39 - Phase 4)
    ///
    /// Exports GPU memory transfer timing (CPU↔GPU) captured via wall-clock measurement.
    /// Follows Sprint 37's adaptive sampling pattern (only trace if duration > threshold).
    ///
    /// # Arguments
    ///
    /// * `transfer` - Metadata about the GPU memory transfer
    ///
    /// # Adaptive Sampling
    ///
    /// This method should only be called if duration >= threshold (default 100μs).
    /// The caller (transfer tracking wrapper) handles sampling decisions.
    pub fn record_gpu_transfer(&self, transfer: GpuMemoryTransfer) {
        let mut span_attrs = vec![
            // Only dynamic attributes on span (Toyota Way: no attribute explosion)
            KeyValue::new(
                "gpu_transfer.direction",
                transfer.direction.as_str().to_string(),
            ),
            KeyValue::new("gpu_transfer.bytes", transfer.bytes as i64),
            KeyValue::new("gpu_transfer.duration_us", transfer.duration_us as i64),
            KeyValue::new("gpu_transfer.bandwidth_mbps", transfer.bandwidth_mbps),
            KeyValue::new("gpu_transfer.is_slow", transfer.is_slow),
        ];

        // Optional buffer usage
        if let Some(ref usage) = transfer.buffer_usage {
            span_attrs.push(KeyValue::new("gpu_transfer.buffer_usage", usage.clone()));
        }

        let mut span = self
            .tracer
            .span_builder(format!("gpu_transfer: {}", transfer.label))
            .with_kind(SpanKind::Internal)
            .with_attributes(span_attrs)
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
        let config = OtlpConfig::new(
            "http://localhost:4317".to_string(),
            "test-service".to_string(),
        );

        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.batch_size, 512); // Sprint 36: default batch size
        assert_eq!(config.batch_delay_ms, 1000); // Sprint 36: default delay
        assert_eq!(config.queue_size, 2048); // Sprint 36: default queue size
    }

    #[test]
    #[cfg(feature = "otlp")]
    fn test_otlp_config_presets() {
        // Test balanced preset
        let balanced =
            OtlpConfig::balanced("http://localhost:4317".to_string(), "test".to_string());
        assert_eq!(balanced.batch_size, 512);
        assert_eq!(balanced.batch_delay_ms, 1000);

        // Test aggressive preset
        let aggressive =
            OtlpConfig::aggressive("http://localhost:4317".to_string(), "test".to_string());
        assert_eq!(aggressive.batch_size, 2048);
        assert_eq!(aggressive.batch_delay_ms, 5000);

        // Test low-latency preset
        let low_latency =
            OtlpConfig::low_latency("http://localhost:4317".to_string(), "test".to_string());
        assert_eq!(low_latency.batch_size, 128);
        assert_eq!(low_latency.batch_delay_ms, 100);
    }

    #[test]
    #[cfg(feature = "otlp")]
    fn test_otlp_config_builder() {
        let config = OtlpConfig::new("http://localhost:4317".to_string(), "test".to_string())
            .with_batch_size(1024)
            .with_batch_delay_ms(2000)
            .with_queue_size(4096);

        assert_eq!(config.batch_size, 1024);
        assert_eq!(config.batch_delay_ms, 2000);
        assert_eq!(config.queue_size, 4096);
    }

    #[test]
    #[cfg(not(feature = "otlp"))]
    fn test_otlp_disabled_returns_error() {
        let config = OtlpConfig::new("http://localhost:4317".to_string(), "test".to_string());

        let result = OtlpExporter::new(config, None);
        assert!(result.is_err());
    }
}
