# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**renacer** is a pure Rust system call tracer with source-aware correlation for Rust binaries. It provides `strace`-like functionality with enhanced debugging capabilities including source code mapping and semantic analysis.

## Build and Test Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Release build

# Test
cargo test                     # Run all tests
cargo test --lib               # Unit tests only

# Run
cargo run -- <binary>          # Trace a binary
cargo run -- --help            # Show options

# Lint
cargo clippy -- -D warnings    # Clippy with strict warnings
cargo fmt --check              # Check formatting
```

## Architecture

### Core Components

- **Tracer**: ptrace-based syscall interception
- **Decoder**: System call argument parsing and formatting
- **Correlator**: Source code location mapping via DWARF debug info
- **Reporter**: Output formatting (text, JSON, flamegraph)

### Key Features

- Pure Rust implementation (no C dependencies for core tracing)
- Source-aware correlation for Rust binaries
- JSON output for tooling integration
- Semantic validation of transpiled code

## Integration with Sovereign AI Stack

renacer is used by batuta for semantic validation during transpilation:

```rust
use renacer::{Tracer, TracerConfig};

// Trace original and transpiled binaries
let original_trace = Tracer::new(config).trace("./original")?;
let transpiled_trace = Tracer::new(config).trace("./transpiled")?;

// Compare syscall sequences for semantic equivalence
assert!(original_trace.semantically_equivalent(&transpiled_trace));
```

## Dependencies

- `nix`: Linux ptrace interface
- `libc`: System call numbers and structures
- `clap`: CLI argument parsing
- `serde`/`serde_json`: Trace serialization

## Stack Documentation Search

**IMPORTANT: Proactively use the batuta RAG oracle when:**
- Looking up patterns from other stack components (trueno SIMD, aprender ML, realizar inference)
- Finding how transpilers use renacer for semantic validation (depyler, bashrs, decy)
- Understanding ptrace/syscall patterns across the stack
- Researching anomaly detection and ML-based analysis approaches

```bash
# Index all stack documentation (run once, persists to ~/.cache/batuta/rag/)
batuta oracle --rag-index

# Search across the entire stack
batuta oracle --rag "your question here"

# Renacer-specific examples
batuta oracle --rag "syscall tracing semantic validation"
batuta oracle --rag "DWARF debug info source correlation"
batuta oracle --rag "ptrace process tracing patterns"
batuta oracle --rag "anomaly detection KMeans clustering"
batuta oracle --rag "OpenTelemetry OTLP export integration"
```

The RAG index (341+ docs) includes CLAUDE.md, README.md, and source files from all stack components plus Python ground truth corpora for cross-language pattern matching.
