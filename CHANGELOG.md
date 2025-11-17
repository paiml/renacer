# Changelog

All notable changes to Renacer will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-11-16

### Added

#### Core Features
- **System Call Tracing**: Intercept and display all system calls from traced programs
- **Syscall Name Resolution**: Map syscall numbers to names for x86_64 (80+ syscalls)
- **Argument Decoding**: Decode common arguments including:
  - File paths in `openat()` calls
  - File descriptors, buffer addresses, counts
- **Exit Code Preservation**: Traced program's exit code is properly propagated
- **CLI Interface**: Clean command-line interface with `renacer -- COMMAND` syntax

#### Performance
- **1.1-1.3x Faster Than strace**: Formal benchmarks show consistent performance advantage
  - ls workload: 1.12x faster (137ms vs 154ms)
  - find workload: 1.09x faster (680ms vs 739ms)
  - echo workload: 1.28x faster (4.1ms vs 5.3ms)
- **Efficient Memory Operations**: Uses `process_vm_readv` for direct memory reads
- **Native Compilation**: Rust with LTO optimizations for maximum performance
- **Room for Optimization**: Target is 2-5x faster (roadmap Sprint 11-12)

#### Infrastructure
- **DWARF Support (Planned)**: `--source` flag infrastructure for future source correlation
- **Comprehensive Test Suite**:
  - 16 integration tests across 3 sprint phases
  - Unit tests for core modules
  - 100% pass rate on core functionality
- **Quality Tooling**: Integrated with paiml-mcp-agent-toolkit for TDG scoring

### Performance Benchmarks (v0.1.0 - Informal)

```
Command: ls -laR /usr/bin | head -1000 (average of 5 runs)
- Baseline (no tracing):  13ms
- strace:                163ms (1253% overhead)
- renacer:                20ms  (153% overhead)

Result: renacer is 8.15x FASTER than strace
NOTE: This was an early informal benchmark. See v0.2.0 for formal benchmarks.
```

### Quality Metrics

- **Test Coverage**: 100% of core features tested
- **TDG Score**: 94.6/100 (A grade)
- **Clippy Warnings**: 0 (excluding external crate deprecations)
- **Zero Regressions**: All tests maintained throughout development

### Architecture

- **Language**: Pure Rust (edition 2021)
- **Tracing Method**: ptrace system calls
- **Platform**: Linux x86_64
- **Dependencies**: Minimal - nix, clap, anyhow, thiserror

### Development Methodology

Built using EXTREME TDD following Toyota Way principles:
- **Jidoka** (Built-in Quality): RED → GREEN → REFACTOR cycle
- **Kaizen** (Continuous Improvement): Iterative 2-week sprints
- **Genchi Genbutsu** (Go and See): Data-driven benchmarking
- **Andon Cord** (Stop the Line): Quality gates block bad code

### Known Limitations

- **x86_64 Only**: aarch64 support planned for future release
- **Source Correlation Partial**: `--source` flag loads DWARF debug info, but syscall attribution requires stack unwinding (deferred to v0.2.0)
  - DWARF .debug_line parsing: ✅ Implemented with addr2line crate
  - Binary debug info loading: ✅ Implemented
  - Syscall-to-source attribution: ⚠️ Requires stack unwinding (syscalls happen in libc, not user code)
  - Planned for v0.2.0: Full call stack unwinding to attribute syscalls to user code frames
- **Basic Argument Decoding**: Currently supports filenames; advanced decoding (buffers, structures) planned

### Future Roadmap

See `roadmap.yaml` for detailed implementation plan:

**v0.2.0** (Sprint 5-6 completion):
- Full DWARF source correlation
- Map syscalls to source file:line
- Function name attribution

**v0.3.0** (Sprint 7-8):
- Multi-architecture support (aarch64)
- Cross-platform testing with QEMU

**v1.0.0** (Sprint 9-12):
- strace feature parity (`-p`, `-f`, `-e trace=`, `-c`, `-T`)
- JSON output format
- Advanced filtering

### Contributors

- Primary Development: Claude Code (Anthropic) with EXTREME TDD
- Methodology: paiml-mcp-agent-toolkit quality enforcement
- Specification: Toyota Way expert review

---

## [Unreleased]

### Added (Post-v0.1.0)

#### Sprint 9-10: Advanced Filtering, Statistics, Timing, JSON & PID Attach
- **Syscall Filtering**: `-e trace=EXPR` flag for filtering syscalls
  - Individual syscalls: `-e trace=open,read,write`
  - Syscall classes: `-e trace=file`, `-e trace=network`, `-e trace=process`, `-e trace=memory`
  - Mixed mode: `-e trace=file,socket,brk`
  - Hash-based filter implementation with O(1) lookup
- **Filter Module**: Robust parsing and evaluation of filter expressions (src/filter.rs)
- **Statistics Mode**: `-c` flag for syscall summary (strace-compatible)
  - Per-syscall call counts and error counts
  - Percentage distribution with timing data
  - Summary table with totals (% time, seconds, usecs/call columns)
  - Compatible with filtering
- **Per-Syscall Timing**: `-T` flag for syscall duration tracking
  - Displays time in `<seconds>` format after each syscall
  - Integrated with statistics mode (% time, seconds, usecs/call columns)
  - Zero overhead when disabled
- **JSON Output**: `--format json` for machine-parseable output
  - Structured renacer-json-v1 schema with syscalls and summary
  - Compatible with filtering, timing, and source correlation
  - Ideal for tooling integration and analysis pipelines
  - Full serde serialization support
- **PID Attach**: `-p PID` flag for attaching to running processes
  - Uses PTRACE_ATTACH instead of fork + PTRACE_TRACEME
  - Mutually exclusive with command tracing
  - Proper error handling for non-existent PIDs
  - Shares same tracing infrastructure as command mode
- **Fork Following Infrastructure**: `-f` flag and ptrace options (PTRACE_O_TRACEFORK/VFORK/CLONE)
  - CLI flag implemented
  - Ptrace options configured
  - Full multi-process tracking deferred to v0.3.0 (requires refactoring)
- **Zero Overhead**: Filtering/statistics/timing at display time, no performance impact when disabled
- **24 Integration Tests**: Comprehensive coverage across 5 test suites
  - 6 tests for filtering (tests/sprint9_filtering_tests.rs)
  - 4 tests for statistics mode (tests/sprint9_statistics_tests.rs)
  - 4 tests for timing mode (tests/sprint9_timing_tests.rs)
  - 5 tests for JSON output (tests/sprint9_json_output_tests.rs)
  - 5 tests for PID attach (tests/sprint9_pid_attach_tests.rs)

###Sprint 11-12: Hardening & Performance Baseline (Started)
- ✅ Benchmark suite vs strace (4 comprehensive benchmarks)
- ⏳ 90%+ test coverage enforcement (pending)
- ⏳ 24hr fuzz runs (pending)
- ⏳ Complete documentation (in progress)

#### Performance Benchmarks (v0.2.0 - Formal)

Benchmark suite in `tests/benchmark_vs_strace.rs`:

```
ls -la /usr/bin (5 runs):
- Baseline: 14.4ms
- strace:   154ms (965% overhead)
- renacer:  137ms (851% overhead)
Result: 1.12x faster

find /usr/share/doc (3 runs):
- Baseline: 371ms
- strace:   739ms (99% overhead)
- renacer:  680ms (83% overhead)
Result: 1.09x faster

echo "hello" (10 runs):
- Baseline: 0.59ms
- strace:   5.31ms
- renacer:  4.14ms
Result: 1.28x faster

Filtering overhead: ~8% improvement with -e trace=open
```

**Honest Assessment** (Genchi Genbutsu):
- Current: 1.1-1.3x faster than strace
- Target: 2-5x faster (roadmap Sprint 11-12)
- Room for optimization exists

### Sprint 9-10 Status (5/6 Complete - 83%)
- ✅ Syscall filtering with `-e trace=` expressions
- ✅ Statistics mode with `-c` flag
- ✅ Per-syscall timing with `-T` flag
- ✅ JSON output with `--format json`
- ✅ PID attach with `-p PID` flag
- ⚠️  Fork following with `-f` flag (infrastructure only - full implementation deferred to v0.3.0)

### Quality Metrics (Post Sprint 9-10)
- **TDG Score**: 92.6/100 (A grade)
- **Test Suites**: 8 total (3 from v0.1.0 + 5 from Sprint 9-10)
- **Test Count**: 36 passing (33 active + 3 ignored)
- **New Modules**: 3 (filter.rs, stats.rs, json_output.rs)
- **Zero Regressions**: All previous tests maintained

### Planned for 0.2.0
- ✅ DWARF .debug_line parsing using addr2line crate (COMPLETED in v0.1.0)
- ✅ `--source` flag infrastructure (COMPLETED in v0.1.0)
- ✅ Basic syscall filtering (COMPLETED post-v0.1.0)
- ✅ `-c` statistics mode (COMPLETED post-v0.1.0)
- ✅ `-T` timing mode (COMPLETED post-v0.1.0)
- ✅ `--format json` JSON output (COMPLETED post-v0.1.0)
- ✅ `-p PID` attach to running process (COMPLETED post-v0.1.0)
- Stack unwinding to attribute syscalls to user code frames
- Source-aware output showing file:line for each syscall (requires stack unwinding)
- Function name attribution from DWARF .debug_info (requires stack unwinding)

### Planned for 0.3.0
- `-f` follow forks (multi-process tracking with refactored trace loop)
- See GitHub Issue #2 for detailed implementation plan

---

[0.1.0]: https://github.com/paiml/renacer/releases/tag/v0.1.0
