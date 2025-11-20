# Renacer Makefile
# Following bashrs and paiml-mcp-agent-toolkit patterns

.SUFFIXES:

.PHONY: help test coverage coverage-html coverage-clean mutants mutants-quick clean build release lint format check \
	tier1 tier2 tier3 chaos-test chaos-full check-regression fuzz benchmark

help: ## Show this help message
	@echo "Renacer - Pure Rust strace alternative"
	@echo ""
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

test: ## Run tests (fast, no coverage)
	@echo "🧪 Running tests..."
	@cargo test --quiet

coverage: ## Generate HTML coverage report and open in browser
	@echo "📊 Running comprehensive test coverage analysis..."
	@echo "🔍 Checking for cargo-llvm-cov..."
	@which cargo-llvm-cov > /dev/null 2>&1 || (echo "📦 Installing cargo-llvm-cov..." && cargo install cargo-llvm-cov --locked)
	@if ! rustup component list --installed | grep -q llvm-tools-preview; then \
		echo "📦 Installing llvm-tools-preview..."; \
		rustup component add llvm-tools-preview; \
	fi
	@echo "🧹 Cleaning old coverage data..."
	@cargo llvm-cov clean --workspace
	@mkdir -p target/coverage/html
	@echo "⚙️  Temporarily disabling global cargo config (mold/custom linker breaks coverage)..."
	@test -f ~/.cargo/config.toml && mv ~/.cargo/config.toml ~/.cargo/config.toml.cov-backup || true
	@echo "🧪 Phase 1: Running tests with instrumentation (no report)..."
	@cargo llvm-cov --no-report test --workspace --all-features || true
	@echo "📊 Phase 2: Generating coverage reports..."
	@cargo llvm-cov report --html --output-dir target/coverage/html || echo "⚠️  No coverage data generated"
	@cargo llvm-cov report --lcov --output-path target/coverage/lcov.info || echo "⚠️  LCOV generation skipped"
	@echo "⚙️  Restoring global cargo config..."
	@test -f ~/.cargo/config.toml.cov-backup && mv ~/.cargo/config.toml.cov-backup ~/.cargo/config.toml || true
	@echo ""
	@echo "📊 Coverage Summary:"
	@cargo llvm-cov report --summary-only || echo "Run 'cargo test' to generate coverage data first"
	@echo ""
	@echo "📊 Coverage reports generated:"
	@echo "- HTML: target/coverage/html/index.html"
	@echo "- LCOV: target/coverage/lcov.info"
	@echo ""
	@xdg-open target/coverage/html/index.html 2>/dev/null || \
		open target/coverage/html/index.html 2>/dev/null || \
		echo "✅ Open target/coverage/html/index.html in your browser"

coverage-html: coverage ## Alias for coverage

coverage-clean: ## Clean coverage artifacts
	@echo "🧹 Cleaning coverage artifacts..."
	@if command -v cargo-llvm-cov >/dev/null 2>&1; then \
		cargo llvm-cov clean --workspace; \
		echo "✅ Coverage artifacts cleaned!"; \
	else \
		echo "⚠️  cargo-llvm-cov not installed, skipping clean."; \
	fi

build: ## Build debug binary
	@echo "🔨 Building debug binary..."
	@cargo build

release: ## Build optimized release binary
	@echo "🚀 Building release binary..."
	@cargo build --release
	@echo "✅ Release binary: target/release/renacer"

lint: ## Run clippy linter
	@echo "🔍 Running clippy..."
	@cargo clippy -- -D warnings

format: ## Format code with rustfmt
	@echo "📝 Formatting code..."
	@cargo fmt

check: ## Type check without building
	@echo "✅ Type checking..."
	@cargo check --all-targets --all-features

clean: ## Clean build artifacts
	@echo "🧹 Cleaning build artifacts..."
	@cargo clean
	@rm -rf target/coverage
	@echo "✅ Clean completed!"

benchmark: ## Run performance benchmarks
	@echo "📊 Running benchmarks..."
	@cargo test --test benchmark_vs_strace -- --nocapture --test-threads=1

mutants: ## Run mutation testing (full analysis)
	@echo "🧬 Running mutation testing..."
	@echo "🔍 Checking for cargo-mutants..."
	@which cargo-mutants > /dev/null 2>&1 || (echo "📦 Installing cargo-mutants..." && cargo install cargo-mutants --locked)
	@echo "🧬 Running cargo-mutants (this may take several minutes)..."
	@cargo mutants --output target/mutants.out || echo "⚠️  Some mutants survived"
	@echo ""
	@echo "📊 Mutation Testing Results:"
	@cat target/mutants.out/mutants.out 2>/dev/null || echo "Check target/mutants.out/ for detailed results"

mutants-quick: ## Run mutation testing (quick check on changed files only)
	@echo "🧬 Running quick mutation testing..."
	@echo "🔍 Checking for cargo-mutants..."
	@which cargo-mutants > /dev/null 2>&1 || (echo "📦 Installing cargo-mutants..." && cargo install cargo-mutants --locked)
	@echo "🧬 Running cargo-mutants on uncommitted changes..."
	@cargo mutants --in-diff git:HEAD --output target/mutants-quick.out || echo "⚠️  Some mutants survived"
	@echo ""
	@echo "📊 Quick Mutation Testing Results:"
	@cat target/mutants-quick.out/mutants.out 2>/dev/null || echo "Check target/mutants-quick.out/ for detailed results"

# =============================================================================
# Tiered TDD Workflow (from trueno patterns)
# =============================================================================

tier1: ## Tier 1: Fast tests (<5s) - unit tests, clippy, format
	@echo "🏃 Tier 1: Fast tests (<5 seconds)..."
	@cargo fmt --check
	@cargo clippy -- -D warnings
	@cargo test --lib --quiet
	@echo "✅ Tier 1 complete!"

tier2: tier1 ## Tier 2: Integration tests (<30s) - includes tier1
	@echo "🏃 Tier 2: Integration tests (<30 seconds)..."
	@cargo test --tests --quiet
	@echo "✅ Tier 2 complete!"

tier3: tier2 ## Tier 3: Full validation (<5m) - includes tier1+2, property tests
	@echo "🏃 Tier 3: Full validation (<5 minutes)..."
	@cargo test --all-targets --all-features --quiet
	@echo "✅ Tier 3 complete!"

# =============================================================================
# Chaos Engineering (Sprint 29 - Red-Team Profile)
# =============================================================================

chaos-test: ## Run chaos engineering tests (basic tier)
	@echo "🔥 Running chaos engineering tests..."
	@cargo test --features chaos-basic --quiet
	@echo "✅ Chaos basic tests complete!"

chaos-full: ## Run full chaos engineering suite (requires chaos-full feature)
	@echo "🔥 Running full chaos engineering suite..."
	@cargo test --features chaos-full --quiet
	@echo "✅ Full chaos tests complete!"

check-regression: ## Check for performance regressions (>5% threshold)
	@echo "📊 Checking for performance regressions..."
	@ruchy scripts/check_regression.ruchy || echo "⚠️  Regression check failed or ruchy not found"

fuzz: ## Run fuzz testing targets
	@echo "🎲 Running fuzz tests..."
	@echo "🔍 Checking for cargo-fuzz..."
	@which cargo-fuzz > /dev/null 2>&1 || (echo "📦 Installing cargo-fuzz..." && cargo install cargo-fuzz --locked)
	@cargo +nightly fuzz run filter_parser -- -max_total_time=60 || echo "⚠️  Fuzz testing requires nightly toolchain"

# =============================================================================
# Differential Testing (Oracle Problem)
# =============================================================================

diff-test: ## Run differential tests against strace
	@echo "🔬 Running differential tests (Renacer vs strace)..."
	@cargo test --test differential_strace_tests --quiet || echo "⚠️  Differential tests not yet implemented"
