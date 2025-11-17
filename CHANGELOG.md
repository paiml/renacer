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
- **8x Faster Than strace**: Benchmarked at 20ms vs strace's 163ms for `ls -laR /usr/bin`
- **Efficient Memory Operations**: Uses `process_vm_readv` for direct memory reads
- **Native Compilation**: Rust with LTO optimizations for maximum performance

#### Infrastructure
- **DWARF Support (Planned)**: `--source` flag infrastructure for future source correlation
- **Comprehensive Test Suite**:
  - 16 integration tests across 3 sprint phases
  - Unit tests for core modules
  - 100% pass rate on core functionality
- **Quality Tooling**: Integrated with paiml-mcp-agent-toolkit for TDG scoring

### Performance Benchmarks

```
Command: ls -laR /usr/bin | head -1000 (average of 5 runs)
- Baseline (no tracing):  13ms
- strace:                163ms (1253% overhead)
- renacer:                20ms  (153% overhead)

Result: renacer is 8.15x FASTER than strace
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

#### Sprint 9-10: Advanced Filtering, Statistics & Timing
- **Syscall Filtering**: `-e trace=EXPR` flag for filtering syscalls
  - Individual syscalls: `-e trace=open,read,write`
  - Syscall classes: `-e trace=file`, `-e trace=network`, `-e trace=process`, `-e trace=memory`
  - Mixed mode: `-e trace=file,socket,brk`
- **Filter Module**: Robust parsing and evaluation of filter expressions
- **Statistics Mode**: `-c` flag for syscall summary (strace-compatible)
  - Per-syscall call counts and error counts
  - Percentage distribution with timing data
  - Summary table with totals
  - Compatible with filtering
- **Per-Syscall Timing**: `-T` flag for syscall duration tracking
  - Displays time in `<seconds>` format after each syscall
  - Integrated with statistics mode (% time, seconds, usecs/call columns)
  - Zero overhead when disabled
- **Zero Overhead**: Filtering/statistics/timing at display time, no performance impact when disabled
- **14 Integration Tests**: Comprehensive coverage of filtering, statistics, and timing functionality

### Planned for 0.2.0
- ✅ DWARF .debug_line parsing using addr2line crate (COMPLETED in v0.1.0)
- ✅ `--source` flag infrastructure (COMPLETED in v0.1.0)
- ✅ Basic syscall filtering (COMPLETED post-v0.1.0)
- ✅ `-c` statistics mode (COMPLETED post-v0.1.0)
- ✅ `-T` timing mode (COMPLETED post-v0.1.0)
- Stack unwinding to attribute syscalls to user code frames
- Source-aware output showing file:line for each syscall (requires stack unwinding)
- Function name attribution from DWARF .debug_info (requires stack unwinding)
- `-f` follow forks
- `-p PID` attach to running process
- `--format json` JSON output

---

[0.1.0]: https://github.com/paiml/renacer/releases/tag/v0.1.0
