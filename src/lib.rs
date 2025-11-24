//! Renacer - Pure Rust system call tracer with source-aware correlation
//!
//! This library provides the core functionality for tracing system calls
//! in Rust binaries, with support for DWARF debug information, function profiling,
//! and comprehensive filtering.

pub mod adaptive_backend; // Sprint 40: Adaptive Backend Selection (Specification Section 5.2)
pub mod adaptive_sampler; // Sprint 40: Adaptive Sampling (Specification Section 7.3)
pub mod anomaly;
pub mod anti_patterns; // Sprint 41: Anti-pattern detection (God Process, Tight Loop, PCIe)
pub mod assertion_dsl; // Sprint 44: renacer.toml parser for build-time assertions
pub mod assertion_engine; // Sprint 44: Assertion evaluation engine (Toyota Way: Andon)
pub mod assertion_types; // Sprint 44: Build-time trace assertion types (Toyota Way: Andon)
pub mod autoencoder;
pub mod causal_graph; // Sprint 41: Causal graph construction for critical path analysis
pub mod chaos;
pub mod cli;
pub mod cluster; // Single-Shot Compile Tooling: TOML-based syscall clustering (Section 6.1)
pub mod critical_path; // Sprint 41: Critical path analysis (longest path in DAG)
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
pub mod lamport_clock; // Sprint 40: Lamport logical clocks for causal ordering (Toyota Way: Poka-Yoke)
pub mod lazy_span; // Sprint 36: Lazy span creation for performance
pub mod ml_anomaly;
pub mod otlp_exporter;
pub mod profiling;
pub mod ring_buffer; // Sprint 40: Lock-free ring buffer for span export (Toyota Way: Heijunka)
pub mod rle_compression; // Sprint 41: Run-length encoding for tight loop compression (Toyota Way: Muda)
pub mod semantic_equivalence; // Sprint 40: Semantic Equivalence (Specification Section 6.3)
pub mod sequence; // Single-Shot Compile Tooling: N-gram sequence mining (Section 6.1.1)
pub mod span_pool; // Sprint 36: Memory pool for span allocations
pub mod span_record; // Sprint 40: Parquet-compatible span schema
pub mod stack_unwind;
pub mod stats;
pub mod syscalls;
pub mod trace_context; // Sprint 33: W3C Trace Context propagation
pub mod tracer;
pub mod transpiler_map;
pub mod trueno_db_storage; // Sprint 40: Trueno-DB Parquet storage for golden thread traces
pub mod unified_trace; // Sprint 40: Unified Trace Model (Specification Section 3.1)
pub mod validation_engine; // Sprint 40: ValidationEngine for Batuta Integration (Specification Section 5.1)
