# Renacer

**Pure Rust system call tracer with source-aware correlation for Rust binaries**

Renacer (Spanish: "to be reborn") is a next-generation binary inspection and tracing framework built following Toyota Way principles and EXTREME TDD methodology.

## Project Status

**Current Sprint:** Sprint 1-2 - Minimal Viable Tracer
**Version:** 0.1.0 (Pre-release)
**Specification:** [docs/specifications/deep-strace-rust-wasm-binary-spec.md](docs/specifications/deep-strace-rust-wasm-binary-spec.md)

## Sprint 1-2 Goals

- [x] Cargo project initialization
- [ ] Minimal CLI accepting `-- COMMAND`
- [ ] Ptrace attach to child process (x86_64 only)
- [ ] Intercept `write` syscall only
- [ ] Print to stdout: `write(1, "Hello\n", 6) = 6`
- [ ] 90%+ test coverage
- [ ] Zero crashes on 100 test programs

## Quick Start

```bash
# Build
cargo build

# Run tests
cargo test

# Trace a simple program (Sprint 1-2 target)
cargo run -- -- echo "Hello, World"
```

## Quality Standards

Following [paiml-mcp-agent-toolkit](https://github.com/paiml/paiml-mcp-agent-toolkit) EXTREME TDD:

- **Test Coverage:** 90%+ line coverage, 85%+ branch coverage
- **Mutation Score:** 80%+ (via cargo-mutants)
- **Technical Debt Grade:** A+ (TDG score < 25)
- **Zero Tolerance:** All tests must pass, no warnings

## Development Workflow

```bash
# Run quality checks
pmat analyze tdg src/

# Check test coverage
cargo tarpaulin --all-features

# Mutation testing
pmat mutate --target src/

# Pre-commit checks
cargo test && cargo clippy -- -D warnings
```

## Architecture

See [docs/specifications/deep-strace-rust-wasm-binary-spec.md](docs/specifications/deep-strace-rust-wasm-binary-spec.md) for complete specification.

**1.0 MVP Focus:** Best-in-class `strace` replacement for Rust developers with DWARF-based source correlation.

**Post-1.0:** eBPF backend, WASM analysis, async runtime support.

## License

MIT - See [LICENSE](LICENSE) file.

## Contributing

See specification document for implementation roadmap and quality standards.
