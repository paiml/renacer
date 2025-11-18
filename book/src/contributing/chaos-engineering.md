# Chaos Engineering

Renacer includes **chaos engineering** capabilities for testing system resilience under adverse conditions. This feature was added in Sprint 29, following patterns from the Aprender ML library.

## Overview

Chaos engineering intentionally introduces controlled failures to verify system behavior under stress. For a syscall tracer like Renacer, this means testing how the tracer behaves when:
- Memory is limited
- CPU resources are constrained
- Processes exit unexpectedly
- Signals interrupt execution
- Network delays occur (for future network syscall analysis)

## ChaosConfig Builder (Sprint 29)

### Basic Usage

```rust
use renacer::chaos::ChaosConfig;
use std::time::Duration;

// Gentle chaos for regular testing
let config = ChaosConfig::gentle();

// Aggressive chaos for stress testing
let config = ChaosConfig::aggressive();

// Custom configuration
let config = ChaosConfig::new()
    .with_memory_limit(100 * 1024 * 1024)  // 100MB
    .with_cpu_limit(0.5)  // 50% CPU
    .with_timeout(Duration::from_secs(30))
    .with_signal_injection(true)
    .build();
```

### Configuration Options

| Parameter | Description | Gentle | Aggressive |
|-----------|-------------|--------|------------|
| `memory_limit` | Max memory in bytes | 500MB | 50MB |
| `cpu_limit` | CPU fraction (0.0-1.0) | 0.8 | 0.2 |
| `timeout` | Max execution time | 60s | 10s |
| `signal_injection` | Random signal delivery | false | true |
| `network_latency` | Simulated network delay | 0ms | 100ms |
| `packet_loss_rate` | Packet drop probability | 0% | 10% |

## Tiered Chaos Features

Renacer's chaos capabilities are organized into progressive tiers, enabled via Cargo features:

### Tier 1: Basic Chaos (`chaos-basic`)

**Fast chaos** - Resource limits and signal injection:

```toml
[dependencies]
renacer = { version = "0.4", features = ["chaos-basic"] }
```

**Capabilities:**
- Memory limit enforcement via cgroups
- CPU throttling
- Execution timeouts
- Signal injection (SIGINT, SIGTERM, SIGUSR1)

**Use Cases:**
- Testing error handling
- Validating graceful shutdowns
- Resource exhaustion scenarios

### Tier 2: Network Chaos (`chaos-network`)

**Network/IO chaos** - Latency and packet loss simulation:

```toml
[dependencies]
renacer = { version = "0.4", features = ["chaos-network"] }
```

**Capabilities:**
- Simulated network latency (ms-level delays)
- Packet loss simulation
- Bandwidth throttling
- Connection drop simulation

**Use Cases:**
- Testing network syscall tracing
- Simulating slow I/O
- Network reliability testing

### Tier 3: Byzantine Chaos (`chaos-byzantine`)

**Byzantine fault injection** - Syscall return modification:

```toml
[dependencies]
renacer = { version = "0.4", features = ["chaos-byzantine"] }
```

**Capabilities:**
- Modify syscall return values randomly
- Inject spurious errors (EINTR, EAGAIN)
- Simulate kernel bugs
- Corrupt data buffers

**Use Cases:**
- Testing error path robustness
- Validating retry logic
- Kernel bug simulation

### Full Suite (`chaos-full`)

**Complete chaos engineering** - All features plus loom and arbitrary:

```toml
[dependencies]
renacer = { version = "0.4", features = ["chaos-full"] }
```

**Additional Capabilities:**
- Concurrency testing with loom
- Fuzzing integration with arbitrary
- Composite chaos scenarios

## Builder Pattern (Aprender-Style)

### Chainable API

```rust
let config = ChaosConfig::new()
    .with_memory_limit(64 * 1024 * 1024)
    .with_cpu_limit(0.3)
    .with_timeout(Duration::from_secs(15))
    .with_signal_injection(true)
    .with_network_latency(Duration::from_millis(50))
    .with_packet_loss_rate(0.05)  // 5% packet loss
    .build();
```

### Preset Methods

```rust
// Gentle: Suitable for CI/CD
let gentle = ChaosConfig::gentle();
assert_eq!(gentle.memory_limit(), Some(500 * 1024 * 1024));
assert_eq!(gentle.cpu_limit(), Some(0.8));

// Aggressive: For stress testing
let aggressive = ChaosConfig::aggressive();
assert_eq!(aggressive.memory_limit(), Some(50 * 1024 * 1024));
assert_eq!(aggressive.signal_injection(), true);
```

## Testing with Chaos

### Property-Based Chaos Tests

Renacer includes 7 property-based tests for chaos configuration:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_memory_limit_valid_range(limit in 1u64..10_000_000_000) {
            let config = ChaosConfig::new()
                .with_memory_limit(limit)
                .build();

            assert_eq!(config.memory_limit(), Some(limit));
        }

        #[test]
        fn test_cpu_limit_clamped(limit in any::<f64>()) {
            let config = ChaosConfig::new()
                .with_cpu_limit(limit)
                .build();

            // CPU limit should be clamped to [0.0, 1.0]
            if let Some(cpu) = config.cpu_limit() {
                assert!(cpu >= 0.0 && cpu <= 1.0);
            }
        }
    }
}
```

### Integration Testing with Chaos

```rust
#[test]
fn test_tracer_under_memory_pressure() {
    let config = ChaosConfig::new()
        .with_memory_limit(10 * 1024 * 1024)  // 10MB limit
        .build();

    // Test that tracer handles OOM gracefully
    let result = run_tracer_with_chaos("ls", config);

    // Should either succeed or fail gracefully
    assert!(result.is_ok() || result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("memory"));
    }
}
```

## Future CLI Integration

Planned for Sprint 30:

```bash
# Use gentle preset
renacer --chaos gentle -- ./app

# Use aggressive preset
renacer --chaos aggressive -- ./flaky-test

# Custom chaos from JSON
renacer --chaos custom:chaos.json -- ./stress-test

# Example chaos.json
{
  "memory_limit": 104857600,
  "cpu_limit": 0.5,
  "timeout_secs": 30,
  "signal_injection": true,
  "network_latency_ms": 100,
  "packet_loss_rate": 0.1
}
```

## Use Cases

### 1. Testing Error Handling

```rust
// Verify tracer handles memory exhaustion
let config = ChaosConfig::new()
    .with_memory_limit(5 * 1024 * 1024)  // Very low limit
    .build();

// Tracer should fail gracefully, not crash
```

### 2. Stress Testing

```rust
// Aggressive limits to find breaking points
let config = ChaosConfig::aggressive();

// Run tracer on large program
// Identify resource bottlenecks
```

### 3. CI/CD Validation

```bash
# In CI pipeline
cargo test --features chaos-basic

# Run gentle chaos tests
CHAOS_MODE=gentle cargo test
```

### 4. Flaky Test Investigation

```rust
// Reproduce timing-sensitive bugs
let config = ChaosConfig::new()
    .with_cpu_limit(0.1)  // Slow down execution
    .with_signal_injection(true)  // Interrupt at random times
    .build();
```

## Chaos vs. Fuzz Testing

| Aspect | Chaos Engineering | Fuzz Testing |
|--------|-------------------|--------------|
| **Focus** | System behavior under stress | Input edge cases |
| **Target** | Resource limits, failures | Parser, validation |
| **Duration** | Moderate (seconds-minutes) | Long (hours-days) |
| **Determinism** | Controlled randomness | Full randomness |
| **Use Case** | Integration testing | Unit testing |

**Best Practice:** Use both together!

```bash
# Tier 3 testing
make fuzz      # Input fuzzing
make chaos     # System chaos testing
```

## Implementation Status (v0.4.1)

- ✅ ChaosConfig builder pattern
- ✅ Gentle/aggressive presets
- ✅ Property-based tests (7 tests)
- ✅ Cargo feature gates
- ⏳ CLI integration (planned Sprint 30)
- ⏳ Runtime chaos injection (planned Sprint 30)
- ⏳ Network chaos (planned Sprint 31)
- ⏳ Byzantine faults (planned Sprint 32)

## Resources

- [Chaos Engineering Book](https://principlesofchaos.org/)
- [Netflix Chaos Engineering](https://netflix.github.io/chaosmonkey/)
- [Aprender Chaos Patterns](https://github.com/paiml/aprender)
- [Loom Concurrency Testing](https://github.com/tokio-rs/loom)

## Example: Complete Chaos Test

```rust
use renacer::chaos::ChaosConfig;
use std::time::Duration;

#[test]
fn test_complete_chaos_scenario() {
    // Create aggressive chaos configuration
    let config = ChaosConfig::new()
        .with_memory_limit(20 * 1024 * 1024)  // 20MB
        .with_cpu_limit(0.25)                  // 25% CPU
        .with_timeout(Duration::from_secs(10))
        .with_signal_injection(true)
        .with_network_latency(Duration::from_millis(200))
        .with_packet_loss_rate(0.15)          // 15% loss
        .build();

    // Run tracer under chaos conditions
    let result = stress_test_tracer(config);

    // Verify graceful degradation
    match result {
        Ok(trace) => {
            // Success despite chaos!
            assert!(trace.syscalls.len() > 0);
        }
        Err(e) => {
            // Failed gracefully with meaningful error
            assert!(
                e.to_string().contains("timeout") ||
                e.to_string().contains("memory") ||
                e.to_string().contains("signal")
            );
        }
    }
}
```

## Key Takeaways

1. **Chaos reveals resilience** - Finds bugs that normal testing misses
2. **Progressive complexity** - Start with basic, move to byzantine
3. **Automated testing** - Integrate into CI/CD pipeline
4. **Graceful degradation** - Systems should fail safely
5. **Complementary to fuzzing** - Use both for comprehensive testing
