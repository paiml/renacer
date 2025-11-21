//! Renacer - Pure Rust system call tracer with source-aware correlation
//!
//! This library provides the core functionality for tracing system calls
//! in Rust binaries, with support for DWARF debug information, function profiling,
//! and comprehensive filtering.

pub mod adaptive_sampler; // Sprint 40: Adaptive Sampling (Specification Section 7.3)
pub mod anomaly;
pub mod autoencoder;
pub mod chaos;
pub mod cli;
pub mod csv_output;
pub mod cuda_tracer; // Sprint 38: CUDA kernel-level tracing via CUPTI
pub mod decision_trace;
pub mod dwarf;
pub mod filter;
pub mod function_profiler;
pub mod gpu_tracer; // Sprint 37: GPU kernel-level tracing for wgpu
pub mod hpu;
pub mod html_output;
pub mod isolation_forest;
pub mod json_output;
pub mod lazy_span; // Sprint 36: Lazy span creation for performance
pub mod ml_anomaly;
pub mod otlp_exporter;
pub mod profiling;
pub mod span_pool; // Sprint 36: Memory pool for span allocations
pub mod stack_unwind;
pub mod stats;
pub mod syscalls;
pub mod trace_context; // Sprint 33: W3C Trace Context propagation
pub mod tracer;
pub mod transpiler_map;
pub mod unified_trace; // Sprint 40: Unified Trace Model (Specification Section 3.1)
