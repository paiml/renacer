//! Renacer - Pure Rust system call tracer with source-aware correlation
//!
//! This library provides the core functionality for tracing system calls
//! in Rust binaries, with support for DWARF debug information, function profiling,
//! and comprehensive filtering.

pub mod anomaly;
pub mod chaos;
pub mod cli;
pub mod csv_output;
pub mod dwarf;
pub mod filter;
pub mod function_profiler;
pub mod hpu;
pub mod html_output;
pub mod json_output;
pub mod ml_anomaly;
pub mod profiling;
pub mod stack_unwind;
pub mod stats;
pub mod syscalls;
pub mod tracer;
pub mod transpiler_map;
