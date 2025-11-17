//! Renacer - Pure Rust system call tracer with source-aware correlation
//!
//! This library provides the core functionality for tracing system calls
//! in Rust binaries, with support for DWARF debug information, function profiling,
//! and comprehensive filtering.

pub mod cli;
pub mod dwarf;
pub mod filter;
pub mod function_profiler;
pub mod json_output;
pub mod profiling;
pub mod stack_unwind;
pub mod stats;
pub mod syscalls;
pub mod tracer;
