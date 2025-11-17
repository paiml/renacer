# Renacer

**Pure Rust system call tracer with source-aware correlation for Rust binaries**

Renacer (Spanish: "to be reborn") is a next-generation binary inspection and tracing framework built following Toyota Way principles and EXTREME TDD methodology.

## Project Status

**Current Version:** 0.3.0-dev (Sprint 16 in progress)
**Status:** Production-Ready (v0.2.0) + Advanced Filtering (Sprint 15-16)
**TDG Score:** 94.5/100 (A grade)
**Test Coverage:** 201 tests (all passing)
**Specification:** [docs/specifications/deep-strace-rust-wasm-binary-spec.md](docs/specifications/deep-strace-rust-wasm-binary-spec.md)

## Features

### Core Tracing (Sprint 1-10)
- ✅ **Full syscall tracing** - All 335 Linux syscalls supported
- ✅ **DWARF debug info** - Source file and line number correlation
- ✅ **Statistics mode** (-c flag) - Call counts, error rates, timing
- ✅ **JSON output** (--format json) - Machine-readable trace export
- ✅ **Advanced filtering** (-e trace=SPEC) - File, network, process, memory classes
- ✅ **Negation operator** (Sprint 15) - Exclude syscalls with ! prefix
- ✅ **Regex patterns** (Sprint 16) - Pattern matching with /regex/ syntax
- ✅ **PID attachment** (-p PID) - Attach to running processes
- ✅ **Timing mode** (-T) - Microsecond-precision syscall durations

### Function Profiling (Sprint 13-14)
- ✅ **I/O Bottleneck Detection** - Automatic detection of slow I/O (>1ms)
- ✅ **Call Graph Tracking** - Parent→child function relationships via stack unwinding
- ✅ **Hot Path Analysis** - Top 10 most expensive functions with percentage breakdown
- ✅ **Flamegraph Export** - Compatible with flamegraph.pl, inferno, speedscope

### Quality Infrastructure (v0.2.0)
- ✅ **Property-based testing** - 670+ test cases via proptest
- ✅ **Pre-commit hooks** - 5 quality gates (format, clippy, tests, audit, bash)
- ✅ **Dependency policy** - cargo-deny configuration for security
- ✅ **Zero warnings** - Clippy strict mode enforced
- ✅ **Trueno integration** - SIMD-accelerated statistics via trueno v0.1.0

## Quick Start

```bash
# Install
cargo install --git https://github.com/paiml/renacer

# Basic tracing
renacer -- ls -la

# With source correlation (requires debug symbols)
renacer --source -- cargo test

# Function profiling with flamegraph
renacer --function-time --source -- ./my-binary > profile.txt
cat profile.txt | flamegraph.pl > flamegraph.svg

# JSON output for scripting
renacer --format json -- echo "test" > trace.json

# CSV output for spreadsheet analysis (Sprint 17)
renacer --format csv -- echo "test" > trace.csv
renacer --format csv -T -- ls > trace-with-timing.csv
renacer --format csv --source -- ./my-binary > trace-with-source.csv
renacer --format csv -c -- cargo build > stats.csv

# Filter syscalls
renacer -e trace=file -- cat file.txt       # File operations only
renacer -e trace=open,read,write -- ls      # Specific syscalls
renacer -e trace=!close -- ls               # All syscalls except close (Sprint 15)
renacer -e trace=file,!close -- cat file    # File ops except close (Sprint 15)

# Regex patterns (Sprint 16)
renacer -e 'trace=/^open.*/' -- ls          # All syscalls starting with "open"
renacer -e 'trace=/.*at$/' -- cat file      # All syscalls ending with "at"
renacer -e 'trace=/read|write/' -- app      # Syscalls matching read OR write
renacer -e 'trace=/^open.*/,!/openat/' -- ls  # open* except openat

# Statistics summary
renacer -c -T -- cargo build

# Attach to running process
renacer -p 1234
```

## Examples

### Basic Syscall Tracing
```bash
$ renacer -- echo "Hello"
openat(AT_FDCWD, "/etc/ld.so.cache", O_RDONLY|O_CLOEXEC) = 3
write(1, "Hello\n", 6) = 6
exit_group(0) = ?
```

### With Source Correlation
```bash
$ renacer --source -- ./my-program
read(3, buf, 1024) = 42          [src/main.rs:15 in my_function]
write(1, "result", 6) = 6        [src/main.rs:20 in my_function]
```

### Function Profiling
```bash
$ renacer --function-time --source -- cargo test

Function Profiling Summary:
========================
Total functions profiled: 5
Total syscalls: 142

Top 10 Hot Paths (by total time):
  1. cargo::build_script  - 45.2% (1.2s, 67 syscalls) ⚠️ SLOW I/O
  2. rustc::compile       - 32.1% (850ms, 45 syscalls)
  3. std::fs::read_dir    - 12.4% (330ms, 18 syscalls)
  ...

Call Graph:
  cargo::build_script
    └─ rustc::compile (67 calls)
       └─ std::fs::read_dir (12 calls)
```

## Performance

Benchmarks vs strace (Sprint 11-12):
- **Overhead:** 5-9% vs 8-12% (strace)
- **Memory:** ~2MB vs ~5MB (strace)
- **Syscalls:** 335 supported vs 335 (strace)
- **Features:** Source correlation + function profiling (unique to Renacer)

## Quality Standards

Following [paiml-mcp-agent-toolkit](https://github.com/paiml/paiml-mcp-agent-toolkit) EXTREME TDD:

- **Test Coverage:** 91.21% overall, 100% on critical modules
- **Mutation Score:** 80%+ (via cargo-mutants)
- **TDG Score:** 94.2/100 (A grade)
- **Zero Tolerance:** All 142 tests pass, zero warnings

## Development

### Setup
```bash
git clone https://github.com/paiml/renacer
cd renacer
cargo build
```

### Pre-commit Hook
The pre-commit hook automatically runs 5 quality gates (<10s):
```bash
chmod +x .git/hooks/pre-commit

# Triggered on every commit:
# 1. cargo fmt --check
# 2. cargo clippy -- -D warnings
# 3. bashrs lint (bash/Makefile quality)
# 4. cargo test --test property_based_comprehensive
# 5. cargo audit
```

### Testing
```bash
# All tests (142 unit + integration)
cargo test

# Property-based tests only (670+ cases)
cargo test --test property_based_comprehensive

# With coverage
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# Mutation testing
cargo mutants
```

### Quality Checks
```bash
# TDG analysis
pmat analyze tdg src/

# Dependency audit
cargo audit

# Deny check (licenses, bans, sources)
cargo deny check
```

## Architecture

### Modules
- `cli` - Command-line argument parsing (clap)
- `tracer` - Core ptrace syscall tracing
- `syscalls` - Syscall name resolution (335 syscalls)
- `dwarf` - DWARF debug info parsing (addr2line, gimli)
- `filter` - Syscall filtering (classes + individual syscalls)
- `stats` - Statistics tracking (Trueno SIMD)
- `json_output` - JSON export format
- `function_profiler` - Function-level profiling with I/O detection
- `stack_unwind` - Stack unwinding for call graphs
- `profiling` - Self-profiling infrastructure

### Dependencies
- `nix` - Ptrace system calls
- `addr2line`, `gimli`, `object` - DWARF parsing
- `clap` - CLI parsing
- `serde`, `serde_json` - JSON serialization
- `trueno` - SIMD-accelerated statistics
- `proptest` - Property-based testing

## Roadmap

See [CHANGELOG.md](CHANGELOG.md) for version history.

### v0.2.0 ✅ (Current - 2025-11-17)
- Function-level profiling complete
- Property-based test suite
- Pre-commit quality gates
- Trueno SIMD integration

### v0.3.0 (Planned)
- Multi-threaded tracing
- eBPF backend option
- Advanced filtering expressions
- Performance dashboard

### v1.0.0 (Planned)
- Production hardening
- Cross-platform support (ARM64)
- Plugin architecture
- Web UI for trace analysis

## License

MIT - See [LICENSE](LICENSE) file.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Follow EXTREME TDD (tests first!)
4. Ensure all quality gates pass
5. Submit pull request

See [docs/specifications/deep-strace-rust-wasm-binary-spec.md](docs/specifications/deep-strace-rust-wasm-binary-spec.md) for complete specification.

## Credits

Built with:
- Toyota Way quality principles
- EXTREME TDD methodology
- [paiml-mcp-agent-toolkit](https://github.com/paiml/paiml-mcp-agent-toolkit) workflows
- [Trueno](https://github.com/paiml/trueno) SIMD library

Developed by [Pragmatic AI Labs](https://paiml.com)
