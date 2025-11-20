# Trueno Tracing Integration Specification for Renacer

**Version:** 1.0
**Date:** 2025-11-20
**Status:** Specification Draft
**Sprint Target:** 32-33 (Span Context Propagation + Custom Sampling)

## Executive Summary

This specification defines **observability integration** between **Trueno** (SIMD/GPU compute library) and **Renacer** (syscall tracer with OTLP export) to provide **end-to-end visibility** into both syscall-level behavior and compute-level operations. This extends beyond Sprint 19-20's statistical integration to add **distributed tracing** for Trueno compute operations.

**Business Value:**
- **Performance Visibility**: See which SIMD backend is being used (AVX2/SSE2/NEON/GPU/Scalar)
- **Bottleneck Identification**: Identify slow statistical computations in Renacer
- **Cross-Layer Tracing**: Single trace showing syscalls → statistics → compute operations
- **Sister Project Synergy**: Deep integration between Trueno and Renacer ecosystems
- **Production Debugging**: Understand performance characteristics in production

**Key Differentiator:** First syscall tracer with **SIMD-level observability** via OpenTelemetry.

---

## Table of Contents

1. [Current State (v0.5.0)](#1-current-state-v050)
2. [Goals and Requirements](#2-goals-and-requirements)
3. [Architecture Overview](#3-architecture-overview)
4. [Phase 1: Trueno Compute Spans](#4-phase-1-trueno-compute-spans)
5. [Phase 2: Backend Detection Traces](#5-phase-2-backend-detection-traces)
6. [Phase 3: Sampling and Performance](#6-phase-3-sampling-and-performance)
7. [Implementation Plan](#7-implementation-plan)
8. [Testing Strategy](#8-testing-strategy)
9. [Performance Impact](#9-performance-impact)
10. [Migration and Compatibility](#10-migration-and-compatibility)

---

## 1. Current State (v0.5.0)

### 1.1 Existing Integrations

**Renacer ← Trueno (Statistics):**
- Location: `src/stats.rs`, `src/anomaly.rs`
- Operations: `Vector::sum()`, `Vector::mean()`, `Vector::stddev()`, `Vector::percentile()`
- Performance: 3-10x faster statistical computations
- **Missing**: No observability into Trueno operations

**Renacer → OpenTelemetry (Sprint 30-31):**
- OTLP exporter: `src/otlp_exporter.rs`
- Syscall spans: Every syscall becomes a span
- Decision traces: Transpiler decisions as span events
- Backends: Jaeger, Grafana Tempo, Elastic APM
- **Missing**: No visibility into compute operations

### 1.2 Gap Analysis

**What We Can See:**
- ✅ Individual syscalls (name, duration, result, source location)
- ✅ Transpiler decisions (category, name, result)
- ✅ Statistical summaries (mean, stddev, percentiles)

**What We Cannot See:**
- ❌ Which SIMD backend Trueno selected (AVX2 vs SSE2 vs Scalar?)
- ❌ How long Trueno operations took (mean, stddev computations)
- ❌ Whether GPU was used for large datasets
- ❌ Trueno operation failures or fallbacks
- ❌ Memory allocations for Vector/Matrix objects

**Problem Example:**
```bash
# User runs Renacer with statistics
$ renacer -c --stats-extended --otlp-endpoint http://localhost:4317 -- cargo build

# Jaeger shows:
# - Root span: "process: cargo"
# - Child spans: 1000+ syscalls
# - Summary: Statistics computed in 50ms

# Questions user cannot answer:
# - Why did statistics take 50ms? (Expected ~10ms with SIMD)
# - Did Trueno use AVX2 or fall back to scalar?
# - Which percentile calculation was slowest?
# - Did any Trueno operation fail?
```

---

## 2. Goals and Requirements

### 2.1 Primary Goals

1. **Compute Operation Visibility**: Export Trueno operations as OpenTelemetry spans
2. **Backend Attribution**: Tag spans with SIMD backend used (AVX2/SSE2/NEON/GPU/Scalar)
3. **Performance Analysis**: Measure duration of each Trueno operation
4. **Error Tracking**: Capture Trueno errors/fallbacks in span status
5. **Zero Overhead**: Tracing optional, no impact when disabled

### 2.2 Non-Goals

- **Not** adding tracing to Trueno library itself (upstream change)
- **Not** replacing Trueno's internal benchmarking
- **Not** tracing every element-wise operation (too granular)

### 2.3 Success Criteria

- ✅ Trueno operations appear as spans in Jaeger/Tempo
- ✅ Backend selection visible in span attributes
- ✅ <5% performance overhead when tracing enabled
- ✅ 20+ integration tests covering all Trueno operations
- ✅ Backward compatible (works without OTLP)

---

## 3. Architecture Overview

### 3.1 Tracing Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Observability Backend (Jaeger, Tempo, etc.)                │
└─────────────────────────────────────────────────────────────┘
                          ▲
                          │ OTLP Protocol
                          │
┌─────────────────────────────────────────────────────────────┐
│  Renacer OTLP Exporter (src/otlp_exporter.rs)               │
│  - Export syscall spans                                     │
│  - Export decision event spans                              │
│  - NEW: Export compute operation spans                      │
└─────────────────────────────────────────────────────────────┘
                          ▲
                          │ record_compute_operation()
                          │
┌─────────────────────────────────────────────────────────────┐
│  Compute Tracer Wrapper (NEW: src/compute_tracer.rs)        │
│  - Wrap Trueno Vector operations                            │
│  - Measure duration                                         │
│  - Detect backend used                                      │
│  - Export as spans                                          │
└─────────────────────────────────────────────────────────────┘
                          ▲
                          │ Vector::sum(), mean(), etc.
                          │
┌─────────────────────────────────────────────────────────────┐
│  Trueno Library (external crate v0.4.0)                     │
│  - SIMD-accelerated vector operations                       │
│  - Auto-backend selection                                   │
│  - No built-in tracing                                      │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Span Hierarchy

**Complete Trace Structure:**
```
Root Span: "process: ./program"
├─ Span: "syscall: openat"
├─ Span: "syscall: read"
├─ Span: "syscall: write"
├─ ...
├─ Span: "compute: calculate_statistics"
│   ├─ Span: "trueno: Vector::mean"
│   │   └─ Attributes: backend=AVX2, elements=10000, duration_us=12
│   ├─ Span: "trueno: Vector::stddev"
│   │   └─ Attributes: backend=AVX2, elements=10000, duration_us=24
│   └─ Span: "trueno: Vector::percentile"
│       └─ Attributes: backend=Scalar, elements=10000, duration_us=150, percentile=95.0
└─ Span: "compute: detect_anomalies"
    ├─ Span: "trueno: Vector::mean"
    └─ Span: "trueno: Vector::zscore"
```

### 3.3 Span Attributes

**Trueno Operation Spans:**
```json
{
  "span.name": "trueno: Vector::mean",
  "span.kind": "INTERNAL",
  "attributes": {
    "compute.operation": "mean",
    "compute.library": "trueno",
    "compute.backend": "AVX2",
    "compute.elements": 10000,
    "compute.data_type": "f32",
    "compute.duration_us": 12,
    "compute.result": "success",
    "compute.fallback": false
  },
  "status": "OK"
}
```

**Error/Fallback Example:**
```json
{
  "span.name": "trueno: Vector::percentile",
  "attributes": {
    "compute.operation": "percentile",
    "compute.backend": "Scalar",
    "compute.elements": 10000,
    "compute.fallback": true,
    "compute.fallback_reason": "SSE2 not available on this CPU"
  },
  "status": "ERROR",
  "status.message": "SIMD unavailable, used scalar fallback"
}
```

---

## 4. Phase 1: Trueno Compute Spans

### 4.1 New Module: Compute Tracer

**File:** `src/compute_tracer.rs` (new module)

**Purpose:** Wrap Trueno operations with OpenTelemetry span export

```rust
//! Compute operation tracing for Trueno integration
//!
//! Sprint 32: Provides observability wrapper around Trueno SIMD operations
//! to export compute-level spans to OpenTelemetry backends.

use trueno::Vector;
use std::time::Instant;
use crate::otlp_exporter::OtlpExporter;

/// Wrapper for traced Trueno operations
pub struct ComputeTracer {
    /// OTLP exporter (optional - feature-gated)
    otlp_exporter: Option<OtlpExporter>,
}

impl ComputeTracer {
    pub fn new(otlp_exporter: Option<OtlpExporter>) -> Self {
        Self { otlp_exporter }
    }

    /// Traced Vector::sum() operation
    pub fn traced_sum(&self, vector: &Vector<f32>, operation_name: &str) -> Result<f32, trueno::TruenoError> {
        let start = Instant::now();
        let backend = detect_backend(vector);

        // Execute Trueno operation
        let result = vector.sum();

        let duration_us = start.elapsed().as_micros() as u64;

        // Export span if OTLP enabled
        if let Some(exporter) = &self.otlp_exporter {
            exporter.record_compute_operation(ComputeOperation {
                name: "Vector::sum",
                parent_operation: operation_name,
                backend,
                elements: vector.len(),
                duration_us,
                result: result.is_ok(),
                error: result.as_ref().err().map(|e| format!("{:?}", e)),
            });
        }

        result
    }

    /// Traced Vector::mean() operation
    pub fn traced_mean(&self, vector: &Vector<f32>, operation_name: &str) -> Result<f32, trueno::TruenoError> {
        let start = Instant::now();
        let backend = detect_backend(vector);

        let result = vector.mean();
        let duration_us = start.elapsed().as_micros() as u64;

        if let Some(exporter) = &self.otlp_exporter {
            exporter.record_compute_operation(ComputeOperation {
                name: "Vector::mean",
                parent_operation: operation_name,
                backend,
                elements: vector.len(),
                duration_us,
                result: result.is_ok(),
                error: result.as_ref().err().map(|e| format!("{:?}", e)),
            });
        }

        result
    }

    /// Traced Vector::stddev() operation
    pub fn traced_stddev(&self, vector: &Vector<f32>, operation_name: &str) -> Result<f32, trueno::TruenoError> {
        let start = Instant::now();
        let backend = detect_backend(vector);

        let result = vector.stddev();
        let duration_us = start.elapsed().as_micros() as u64;

        if let Some(exporter) = &self.otlp_exporter {
            exporter.record_compute_operation(ComputeOperation {
                name: "Vector::stddev",
                parent_operation: operation_name,
                backend,
                elements: vector.len(),
                duration_us,
                result: result.is_ok(),
                error: result.as_ref().err().map(|e| format!("{:?}", e)),
            });
        }

        result
    }

    /// Generic traced operation wrapper
    pub fn trace_operation<F, R>(
        &self,
        operation_name: &str,
        parent_operation: &str,
        vector: &Vector<f32>,
        operation: F,
    ) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let backend = detect_backend(vector);

        let result = operation();
        let duration_us = start.elapsed().as_micros() as u64;

        if let Some(exporter) = &self.otlp_exporter {
            exporter.record_compute_operation(ComputeOperation {
                name: operation_name,
                parent_operation,
                backend,
                elements: vector.len(),
                duration_us,
                result: true, // Generic wrapper assumes success
                error: None,
            });
        }

        result
    }
}

/// Detected SIMD backend (heuristic-based)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdBackend {
    AVX2,
    SSE2,
    NEON,
    GPU,
    Scalar,
    Unknown,
}

impl SimdBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            SimdBackend::AVX2 => "AVX2",
            SimdBackend::SSE2 => "SSE2",
            SimdBackend::NEON => "NEON",
            SimdBackend::GPU => "GPU",
            SimdBackend::Scalar => "Scalar",
            SimdBackend::Unknown => "Unknown",
        }
    }
}

/// Detect which SIMD backend Trueno is likely using
fn detect_backend(vector: &Vector<f32>) -> SimdBackend {
    // Heuristic detection based on CPU features and vector size

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return SimdBackend::AVX2;
        }
        if is_x86_feature_detected!("sse2") {
            return SimdBackend::SSE2;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // ARM NEON is baseline on aarch64
        return SimdBackend::NEON;
    }

    // Check for GPU threshold (Trueno uses GPU for >10K elements)
    if vector.len() > 10_000 {
        // Note: This is speculative - would need Trueno API to confirm
        return SimdBackend::GPU;
    }

    SimdBackend::Scalar
}

/// Compute operation metadata for span export
pub struct ComputeOperation {
    pub name: &'static str,
    pub parent_operation: &'static str,
    pub backend: SimdBackend,
    pub elements: usize,
    pub duration_us: u64,
    pub result: bool,
    pub error: Option<String>,
}
```

### 4.2 OTLP Exporter Extension

**File:** `src/otlp_exporter.rs` (extend existing)

**Add Method:**
```rust
impl OtlpExporter {
    /// Record a compute operation (Trueno SIMD operation) as a span
    ///
    /// Sprint 32: Extends OTLP export to include compute-level operations
    pub fn record_compute_operation(&self, operation: ComputeOperation) {
        #[cfg(feature = "otlp")]
        {
            use opentelemetry::trace::{Span, SpanKind, Status, Tracer};
            use std::time::SystemTime;

            if let Some(tracer) = &self.tracer {
                let mut span = tracer
                    .span_builder(format!("trueno: {}", operation.name))
                    .with_kind(SpanKind::Internal)
                    .with_start_time(SystemTime::now())
                    .start(tracer);

                // Set compute-specific attributes
                span.set_attribute(KeyValue::new("compute.operation", operation.name));
                span.set_attribute(KeyValue::new("compute.library", "trueno"));
                span.set_attribute(KeyValue::new("compute.backend", operation.backend.as_str()));
                span.set_attribute(KeyValue::new("compute.elements", operation.elements as i64));
                span.set_attribute(KeyValue::new("compute.duration_us", operation.duration_us as i64));
                span.set_attribute(KeyValue::new("compute.parent", operation.parent_operation));

                // Set status based on result
                if operation.result {
                    span.set_status(Status::Ok);
                } else {
                    span.set_status(Status::error(operation.error.unwrap_or_else(|| "Unknown error".to_string())));
                }

                span.end();
            }
        }
    }
}
```

### 4.3 Integration with Stats Module

**File:** `src/stats.rs` (modify existing)

**Before (no tracing):**
```rust
pub fn calculate_totals_with_trueno(&self) -> StatTotals {
    let counts: Vec<f32> = self.stats.values().map(|s| s.count as f32).collect();
    let total_calls = trueno::Vector::from_slice(&counts).sum().unwrap_or(0.0) as u64;
    // ...
}
```

**After (with tracing):**
```rust
use crate::compute_tracer::ComputeTracer;

impl StatsTracker {
    pub fn calculate_totals_with_trueno(&self, compute_tracer: &ComputeTracer) -> StatTotals {
        let counts: Vec<f32> = self.stats.values().map(|s| s.count as f32).collect();
        let counts_vec = trueno::Vector::from_slice(&counts);

        // Traced sum operation
        let total_calls = compute_tracer
            .traced_sum(&counts_vec, "calculate_totals")
            .unwrap_or(0.0) as u64;

        // ... rest of implementation
    }

    pub fn calculate_extended_stats(&self, compute_tracer: &ComputeTracer) -> ExtendedStats {
        let durations: Vec<f32> = /* ... */;
        let v = trueno::Vector::from_slice(&durations);

        ExtendedStats {
            mean: compute_tracer.traced_mean(&v, "calculate_extended_stats").unwrap_or(0.0),
            stddev: compute_tracer.traced_stddev(&v, "calculate_extended_stats").unwrap_or(0.0),
            // ... rest of stats
        }
    }
}
```

---

## 5. Phase 2: Backend Detection Traces

### 5.1 Enhanced Backend Detection

**Problem:** Current `detect_backend()` is heuristic-based and may be inaccurate.

**Solution:** Add instrumentation to Trueno (upstream contribution) OR use performance characteristics to infer backend.

**Performance-Based Detection:**
```rust
/// Detect backend based on performance characteristics
fn detect_backend_from_performance(
    elements: usize,
    duration_us: u64,
    operation: &str,
) -> SimdBackend {
    // Expected performance (based on Trueno benchmarks)
    let expected_scalar_us = match operation {
        "sum" => elements as u64 / 100,  // ~100 elements/μs scalar
        "mean" => elements as u64 / 100,
        "stddev" => elements as u64 / 50,  // ~50 elements/μs scalar
        _ => elements as u64 / 100,
    };

    let speedup = expected_scalar_us as f64 / duration_us as f64;

    // Classify based on speedup
    if speedup > 3.0 {
        SimdBackend::AVX2  // 3-4x faster
    } else if speedup > 2.5 {
        SimdBackend::SSE2  // 2.5-3x faster
    } else if speedup > 1.5 {
        SimdBackend::NEON  // 1.5-2.5x faster
    } else {
        SimdBackend::Scalar  // <1.5x (likely scalar)
    }
}
```

### 5.2 Backend Span Events

**Add span events for backend selection:**
```rust
// In ComputeTracer::traced_sum()
if let Some(exporter) = &self.otlp_exporter {
    // Add event for backend selection
    exporter.add_span_event("backend_selected", &[
        ("backend", backend.as_str()),
        ("cpu_features", get_cpu_features()),
        ("vector_size", &vector.len().to_string()),
    ]);
}
```

---

## 6. Phase 3: Sampling and Performance

### 6.1 Compute Operation Sampling

**Problem:** Exporting every Trueno operation may be too expensive.

**Solution:** Implement sampling for compute operations.

**Sampling Strategies:**

1. **Probability-Based Sampling:**
```rust
pub struct ComputeTracerConfig {
    /// Sample rate (0.0 - 1.0)
    pub sample_rate: f64,
}

impl ComputeTracer {
    pub fn should_sample(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.config.sample_rate
    }

    pub fn traced_sum(&self, vector: &Vector<f32>, operation_name: &str) -> Result<f32, trueno::TruenoError> {
        if !self.should_sample() {
            return vector.sum();  // Skip tracing
        }

        // ... traced implementation
    }
}
```

2. **Adaptive Sampling (Sprint 33 feature):**
```rust
pub struct AdaptiveSampler {
    /// Sample slow operations (>threshold) at 100%
    slow_threshold_us: u64,
    /// Sample fast operations at reduced rate
    fast_sample_rate: f64,
}

impl AdaptiveSampler {
    pub fn should_sample(&self, duration_us: u64) -> bool {
        if duration_us > self.slow_threshold_us {
            true  // Always sample slow operations
        } else {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen::<f64>() < self.fast_sample_rate
        }
    }
}
```

### 6.2 Performance Budget

**Overhead Analysis:**

| Scenario | Operations/sec | Sampling Rate | Overhead |
|----------|----------------|---------------|----------|
| No tracing | N/A | 0% | 0% |
| Full tracing | 10,000 | 100% | ~15% |
| 10% sampling | 10,000 | 10% | ~2% |
| 1% sampling | 10,000 | 1% | <1% |
| Adaptive (slow only) | 10,000 | ~5% | ~1% |

**Target:** <5% overhead with adaptive sampling

---

## 7. Implementation Plan

### 7.1 Sprint 32: Core Compute Tracing

**RED Phase (Tests First):**

**File:** `tests/sprint32_compute_tracing_tests.rs`

```rust
#[test]
fn test_compute_spans_exported_to_otlp() {
    // Test that Trueno operations appear as spans in OTLP export
}

#[test]
fn test_compute_span_attributes() {
    // Test span attributes (backend, elements, duration)
}

#[test]
fn test_compute_tracing_optional() {
    // Test that compute tracing works without OTLP
}

#[test]
fn test_backend_detection_avx2() {
    // Test backend detection on AVX2 system
}

#[test]
fn test_compute_error_handling() {
    // Test error status when Trueno operation fails
}
```

**GREEN Phase (Implementation):**
1. Create `src/compute_tracer.rs` (250 lines)
2. Extend `src/otlp_exporter.rs` with `record_compute_operation()` (50 lines)
3. Modify `src/stats.rs` to use `ComputeTracer` (50 lines)
4. Modify `src/anomaly.rs` to use `ComputeTracer` (30 lines)

**REFACTOR Phase:**
1. Extract `detect_backend()` into separate module
2. Add unit tests for backend detection
3. Optimize span creation overhead

### 7.2 Sprint 33: Sampling and Optimization

**Features:**
1. Probability-based sampling
2. Adaptive sampling (slow operations)
3. Per-operation sampling configuration
4. Performance benchmarks

**Tests:** 15+ integration tests

---

## 8. Testing Strategy

### 8.1 Integration Tests

**File:** `tests/sprint32_compute_tracing_tests.rs`

**Test Matrix:**

| Test Case | OTLP Enabled | Compute Ops | Expected Spans |
|-----------|--------------|-------------|----------------|
| Basic compute tracing | ✅ | sum, mean, stddev | 3 spans |
| No OTLP | ❌ | sum, mean | 0 spans (no crash) |
| Statistics mode | ✅ | Extended stats | 7+ spans |
| Anomaly detection | ✅ | Real-time anomalies | 10+ spans |
| Backend detection | ✅ | Large vector (10K) | AVX2/SSE2 tags |

**Total Tests:** 20+ integration tests

### 8.2 Unit Tests

**File:** `src/compute_tracer.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_tracer_creation() {
        let tracer = ComputeTracer::new(None);
        assert!(tracer.otlp_exporter.is_none());
    }

    #[test]
    fn test_backend_detection_avx2() {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                let v = Vector::from_slice(&[1.0; 100]);
                let backend = detect_backend(&v);
                assert_eq!(backend, SimdBackend::AVX2);
            }
        }
    }

    #[test]
    fn test_traced_sum_without_otlp() {
        let tracer = ComputeTracer::new(None);
        let v = Vector::from_slice(&[1.0, 2.0, 3.0]);
        let result = tracer.traced_sum(&v, "test").unwrap();
        assert_eq!(result, 6.0);
    }
}
```

---

## 9. Performance Impact

### 9.1 Overhead Measurement

**Benchmark:** `benches/compute_tracing_overhead.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_trueno_sum_no_tracing(c: &mut Criterion) {
    let data: Vec<f32> = (0..10000).map(|i| i as f32).collect();
    let v = Vector::from_slice(&data);

    c.bench_function("trueno_sum_no_tracing", |b| {
        b.iter(|| black_box(&v).sum().unwrap());
    });
}

fn bench_trueno_sum_with_tracing(c: &mut Criterion) {
    let data: Vec<f32> = (0..10000).map(|i| i as f32).collect();
    let v = Vector::from_slice(&data);
    let tracer = ComputeTracer::new(Some(/* OTLP exporter */));

    c.bench_function("trueno_sum_with_tracing", |b| {
        b.iter(|| tracer.traced_sum(black_box(&v), "benchmark").unwrap());
    });
}

criterion_group!(benches, bench_trueno_sum_no_tracing, bench_trueno_sum_with_tracing);
criterion_main!(benches);
```

**Expected Results:**
- No tracing: 12 μs
- With tracing (100%): 14 μs (~15% overhead)
- With sampling (10%): 12.2 μs (~2% overhead)

### 9.2 Optimization Strategies

1. **Lazy Span Creation:** Only create spans if OTLP enabled
2. **Batch Export:** Group compute spans before export
3. **Attribute Pooling:** Reuse attribute objects
4. **Inline Backend Detection:** Cache CPU features

---

## 10. Migration and Compatibility

### 10.1 Backward Compatibility

**Guaranteed:**
- ✅ All existing Renacer features work unchanged
- ✅ OTLP export works without compute tracing
- ✅ Compute tracing optional (opt-in via config)
- ✅ No breaking changes to Trueno API

**Migration Path:**
```rust
// Old code (Sprint 19-20)
let mean = vector.mean().unwrap_or(0.0);

// New code (Sprint 32) - automatic if ComputeTracer injected
let mean = compute_tracer.traced_mean(&vector, "operation_name").unwrap_or(0.0);

// Fallback (if OTLP disabled)
let mean = vector.mean().unwrap_or(0.0);  // Still works
```

### 10.2 Feature Flags

**Cargo.toml:**
```toml
[features]
default = ["otlp"]

# Enable OTLP export (Sprint 30)
otlp = ["dep:opentelemetry", "dep:opentelemetry_sdk", "dep:opentelemetry-otlp", "dep:tokio"]

# Enable compute tracing (Sprint 32)
# Note: Requires 'otlp' feature
compute-tracing = ["otlp"]
```

### 10.3 CLI Flags

**New Flags (Sprint 32):**
```bash
--trace-compute              # Enable Trueno compute operation tracing
--trace-compute-sample 0.1   # Sample 10% of compute operations
--trace-compute-threshold 100  # Only trace operations >100μs
```

**Examples:**
```bash
# Basic compute tracing
renacer --otlp-endpoint http://localhost:4317 --trace-compute -c --stats-extended -- cargo build

# Adaptive sampling (only slow operations)
renacer --otlp-endpoint http://localhost:4317 --trace-compute --trace-compute-threshold 50 -c -- ./app

# 10% sampling
renacer --otlp-endpoint http://localhost:4317 --trace-compute --trace-compute-sample 0.1 -c -- ./app
```

---

## 11. Example Output

### 11.1 Jaeger UI View

**Trace: "process: cargo build"**
```
├─ Span: syscall: openat (duration: 15μs)
├─ Span: syscall: read (duration: 42μs)
├─ Span: syscall: write (duration: 8μs)
├─ ... [1000+ syscall spans] ...
└─ Span: compute: calculate_statistics (duration: 50ms)
    ├─ Span: trueno: Vector::sum (duration: 12μs)
    │   ├─ Attributes:
    │   │   - compute.backend: AVX2
    │   │   - compute.elements: 10000
    │   │   - compute.operation: sum
    │   └─ Status: OK
    ├─ Span: trueno: Vector::mean (duration: 12μs)
    │   ├─ Attributes:
    │   │   - compute.backend: AVX2
    │   │   - compute.elements: 10000
    │   └─ Status: OK
    ├─ Span: trueno: Vector::stddev (duration: 24μs)
    │   ├─ Attributes:
    │   │   - compute.backend: AVX2
    │   │   - compute.elements: 10000
    │   └─ Status: OK
    └─ Span: trueno: Vector::percentile (duration: 150μs)
        ├─ Attributes:
        │   - compute.backend: Scalar
        │   - compute.elements: 10000
        │   - compute.percentile: 95.0
        │   - compute.fallback: true
        │   - compute.fallback_reason: "percentile not SIMD-optimized"
        └─ Status: OK
```

**Insights:**
- ✅ AVX2 backend used for sum, mean, stddev (3-4x speedup)
- ⚠️ Percentile fell back to scalar (150μs instead of expected ~40μs with SIMD)
- 💡 Optimization opportunity: Add SIMD-optimized percentile to Trueno

---

## 12. Future Enhancements

### 12.1 Trueno Upstream Integration

**Proposal:** Add `#[instrument]` macro to Trueno library

```rust
// In Trueno library (upstream contribution)
use tracing::instrument;

impl Vector<f32> {
    #[instrument(level = "trace", skip(self), fields(backend, elements = self.len()))]
    pub fn sum(&self) -> Result<f32, TruenoError> {
        // ... implementation
        tracing::event!(Level::TRACE, backend = ?selected_backend);
        // ...
    }
}
```

**Benefit:** Renacer can use `tracing-opentelemetry` bridge for zero-overhead instrumentation.

### 12.2 GPU Tracing

**Sprint 34+ (Future):**
- Trace GPU kernel launches
- Measure CPU→GPU transfer time
- Identify GPU bottlenecks

### 12.3 Matrix Operation Tracing

**Sprint 35+ (Future):**
- Trace matrix multiplications
- Trace convolution operations
- Correlation matrix analysis tracing

---

## Appendix A: OTLP Exporter API Extension

### Complete API

```rust
impl OtlpExporter {
    /// Sprint 30: Record syscall as span
    pub fn record_syscall(&self, syscall: &SyscallEntry) { /* ... */ }

    /// Sprint 31: Record transpiler decision as span event
    pub fn record_decision(&self, decision: &Decision) { /* ... */ }

    /// Sprint 32: Record compute operation as span
    pub fn record_compute_operation(&self, operation: ComputeOperation) { /* ... */ }

    /// Sprint 32: Add span event (generic)
    pub fn add_span_event(&self, name: &str, attributes: &[(&str, &str)]) { /* ... */ }
}
```

---

## Appendix B: Trueno Operations Coverage

### Phase 1 Coverage (Sprint 32)

**Implemented:**
- ✅ `Vector::sum()` - Traced
- ✅ `Vector::mean()` - Traced
- ✅ `Vector::stddev()` - Traced
- ✅ `Vector::min()` - Traced
- ✅ `Vector::max()` - Traced

**Not Implemented (Future):**
- ⏳ `Vector::dot()` - Sprint 33
- ⏳ `Vector::correlation()` - Sprint 34
- ⏳ `Matrix::matmul()` - Sprint 35
- ⏳ GPU operations - Sprint 36

---

## Document Control

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2025-11-20 | Claude Code | Initial specification for Sprint 32-33 |

**Status:** ✅ Ready for Review
**Approval Required:** Product Owner (Noah Gift)
**Next Review:** Post-Sprint 32 Retrospective
**Related Specs:**
- `trueno-integration-spec.md` (Statistics integration)
- `deep-strace-rust-wasm-binary-spec.md` (Core Renacer spec)
- `ruchy-tracing-support.md` (Transpiler decision tracing)
