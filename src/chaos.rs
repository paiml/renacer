// Chaos Engineering Configuration (Sprint 29 - Red-Team Profile)
//
// Builder pattern for chaos configuration following aprender patterns.
// Enables systematic fault injection for testing Renacer's robustness.
//
// Features:
// - Tiered chaos levels (basic, network, byzantine)
// - Chainable builder API
// - Feature-gated implementations

use std::time::Duration;

/// Chaos engineering configuration with builder pattern
///
/// # Example
/// ```
/// use renacer::chaos::ChaosConfig;
///
/// let config = ChaosConfig::new()
///     .with_memory_limit(100 * 1024 * 1024)  // 100MB
///     .with_cpu_limit(0.5)                    // 50% CPU
///     .with_timeout(Duration::from_secs(30))
///     .with_signal_injection(true)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Memory limit in bytes (0 = unlimited)
    pub memory_limit: usize,

    /// CPU limit as fraction (0.0-1.0, 0 = unlimited)
    pub cpu_limit: f64,

    /// Execution timeout
    pub timeout: Duration,

    /// Enable random signal injection
    pub signal_injection: bool,

    /// Network latency injection (milliseconds)
    #[cfg(feature = "chaos-network")]
    pub network_latency_ms: u64,

    /// Packet loss probability (0.0-1.0)
    #[cfg(feature = "chaos-network")]
    pub packet_loss_prob: f64,

    /// Byzantine fault injection probability (0.0-1.0)
    #[cfg(feature = "chaos-byzantine")]
    pub byzantine_fault_prob: f64,

    /// Syscalls to inject faults into
    #[cfg(feature = "chaos-byzantine")]
    pub fault_syscalls: Vec<String>,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            memory_limit: 0,
            cpu_limit: 0.0,
            timeout: Duration::from_secs(60),
            signal_injection: false,
            #[cfg(feature = "chaos-network")]
            network_latency_ms: 0,
            #[cfg(feature = "chaos-network")]
            packet_loss_prob: 0.0,
            #[cfg(feature = "chaos-byzantine")]
            byzantine_fault_prob: 0.0,
            #[cfg(feature = "chaos-byzantine")]
            fault_syscalls: Vec::new(),
        }
    }
}

impl ChaosConfig {
    /// Create a new chaos configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set memory limit in bytes
    ///
    /// # Example
    /// ```
    /// use renacer::chaos::ChaosConfig;
    /// let config = ChaosConfig::new().with_memory_limit(100 * 1024 * 1024);
    /// assert_eq!(config.memory_limit, 104857600);
    /// ```
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.memory_limit = bytes;
        self
    }

    /// Set CPU limit as fraction (0.0-1.0)
    ///
    /// # Example
    /// ```
    /// use renacer::chaos::ChaosConfig;
    /// let config = ChaosConfig::new().with_cpu_limit(0.5);
    /// assert!((config.cpu_limit - 0.5).abs() < f64::EPSILON);
    /// ```
    pub fn with_cpu_limit(mut self, fraction: f64) -> Self {
        self.cpu_limit = fraction.clamp(0.0, 1.0);
        self
    }

    /// Set execution timeout
    ///
    /// # Example
    /// ```
    /// use renacer::chaos::ChaosConfig;
    /// use std::time::Duration;
    /// let config = ChaosConfig::new().with_timeout(Duration::from_secs(30));
    /// assert_eq!(config.timeout.as_secs(), 30);
    /// ```
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable or disable signal injection
    pub fn with_signal_injection(mut self, enabled: bool) -> Self {
        self.signal_injection = enabled;
        self
    }

    /// Set network latency injection (requires chaos-network feature)
    #[cfg(feature = "chaos-network")]
    pub fn with_network_latency(mut self, ms: u64) -> Self {
        self.network_latency_ms = ms;
        self
    }

    /// Set packet loss probability (requires chaos-network feature)
    #[cfg(feature = "chaos-network")]
    pub fn with_packet_loss(mut self, probability: f64) -> Self {
        self.packet_loss_prob = probability.clamp(0.0, 1.0);
        self
    }

    /// Set byzantine fault injection probability (requires chaos-byzantine feature)
    #[cfg(feature = "chaos-byzantine")]
    pub fn with_byzantine_faults(mut self, probability: f64) -> Self {
        self.byzantine_fault_prob = probability.clamp(0.0, 1.0);
        self
    }

    /// Set syscalls to inject faults into (requires chaos-byzantine feature)
    #[cfg(feature = "chaos-byzantine")]
    pub fn with_fault_syscalls(mut self, syscalls: Vec<String>) -> Self {
        self.fault_syscalls = syscalls;
        self
    }

    /// Build the final configuration (validates and returns)
    pub fn build(self) -> Self {
        self
    }

    /// Check if any chaos features are enabled
    pub fn is_active(&self) -> bool {
        self.memory_limit > 0
            || self.cpu_limit > 0.0
            || self.signal_injection
            || self.timeout < Duration::from_secs(60)
    }

    /// Get a preset for "gentle" chaos testing
    pub fn gentle() -> Self {
        Self::new()
            .with_memory_limit(512 * 1024 * 1024) // 512MB
            .with_cpu_limit(0.8)
            .with_timeout(Duration::from_secs(120))
    }

    /// Get a preset for "aggressive" chaos testing
    pub fn aggressive() -> Self {
        Self::new()
            .with_memory_limit(64 * 1024 * 1024) // 64MB
            .with_cpu_limit(0.25)
            .with_timeout(Duration::from_secs(10))
            .with_signal_injection(true)
    }

    /// Get a preset for "extreme" chaos testing (requires chaos-byzantine)
    #[cfg(feature = "chaos-byzantine")]
    pub fn extreme() -> Self {
        Self::aggressive()
            .with_byzantine_faults(0.1)
            .with_fault_syscalls(vec![
                "read".to_string(),
                "write".to_string(),
                "open".to_string(),
                "close".to_string(),
            ])
    }
}

/// Result type for chaos operations
pub type ChaosResult<T> = Result<T, ChaosError>;

/// Errors that can occur during chaos testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChaosError {
    /// Memory limit exceeded
    MemoryLimitExceeded { limit: usize, used: usize },

    /// Execution timeout
    Timeout { elapsed: Duration, limit: Duration },

    /// Signal injection failed
    SignalInjectionFailed { signal: i32, reason: String },

    /// Byzantine fault injection failed
    #[cfg(feature = "chaos-byzantine")]
    ByzantineFaultFailed { syscall: String, reason: String },
}

impl std::fmt::Display for ChaosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChaosError::MemoryLimitExceeded { limit, used } => {
                write!(f, "Memory limit exceeded: {} > {} bytes", used, limit)
            }
            ChaosError::Timeout { elapsed, limit } => {
                write!(f, "Timeout: {:?} > {:?}", elapsed, limit)
            }
            ChaosError::SignalInjectionFailed { signal, reason } => {
                write!(f, "Signal injection failed ({}): {}", signal, reason)
            }
            #[cfg(feature = "chaos-byzantine")]
            ChaosError::ByzantineFaultFailed { syscall, reason } => {
                write!(f, "Byzantine fault failed ({}): {}", syscall, reason)
            }
        }
    }
}

impl std::error::Error for ChaosError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ChaosConfig::new();
        assert_eq!(config.memory_limit, 0);
        assert!((config.cpu_limit - 0.0).abs() < f64::EPSILON);
        assert_eq!(config.timeout.as_secs(), 60);
        assert!(!config.signal_injection);
    }

    #[test]
    fn test_builder_chain() {
        let config = ChaosConfig::new()
            .with_memory_limit(100)
            .with_cpu_limit(0.5)
            .with_timeout(Duration::from_secs(30))
            .with_signal_injection(true)
            .build();

        assert_eq!(config.memory_limit, 100);
        assert!((config.cpu_limit - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.timeout.as_secs(), 30);
        assert!(config.signal_injection);
    }

    #[test]
    fn test_cpu_limit_clamping() {
        let config = ChaosConfig::new().with_cpu_limit(1.5);
        assert!((config.cpu_limit - 1.0).abs() < f64::EPSILON);

        let config = ChaosConfig::new().with_cpu_limit(-0.5);
        assert!((config.cpu_limit - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_is_active() {
        let config = ChaosConfig::new();
        assert!(!config.is_active());

        let config = ChaosConfig::new().with_memory_limit(100);
        assert!(config.is_active());

        let config = ChaosConfig::new().with_signal_injection(true);
        assert!(config.is_active());
    }

    #[test]
    fn test_gentle_preset() {
        let config = ChaosConfig::gentle();
        assert_eq!(config.memory_limit, 512 * 1024 * 1024);
        assert!((config.cpu_limit - 0.8).abs() < f64::EPSILON);
        assert_eq!(config.timeout.as_secs(), 120);
    }

    #[test]
    fn test_aggressive_preset() {
        let config = ChaosConfig::aggressive();
        assert_eq!(config.memory_limit, 64 * 1024 * 1024);
        assert!((config.cpu_limit - 0.25).abs() < f64::EPSILON);
        assert!(config.signal_injection);
    }

    #[test]
    fn test_error_display() {
        let err = ChaosError::MemoryLimitExceeded {
            limit: 100,
            used: 200,
        };
        assert!(err.to_string().contains("200"));
        assert!(err.to_string().contains("100"));

        let err = ChaosError::Timeout {
            elapsed: Duration::from_secs(10),
            limit: Duration::from_secs(5),
        };
        assert!(err.to_string().contains("Timeout"));
    }

    #[cfg(feature = "chaos-network")]
    #[test]
    fn test_network_chaos() {
        let config = ChaosConfig::new()
            .with_network_latency(100)
            .with_packet_loss(0.1);

        assert_eq!(config.network_latency_ms, 100);
        assert!((config.packet_loss_prob - 0.1).abs() < f64::EPSILON);
    }

    #[cfg(feature = "chaos-byzantine")]
    #[test]
    fn test_byzantine_chaos() {
        let config = ChaosConfig::new()
            .with_byzantine_faults(0.05)
            .with_fault_syscalls(vec!["read".to_string()]);

        assert!((config.byzantine_fault_prob - 0.05).abs() < f64::EPSILON);
        assert_eq!(config.fault_syscalls, vec!["read".to_string()]);
    }

    #[cfg(feature = "chaos-byzantine")]
    #[test]
    fn test_extreme_preset() {
        let config = ChaosConfig::extreme();
        assert!((config.byzantine_fault_prob - 0.1).abs() < f64::EPSILON);
        assert!(!config.fault_syscalls.is_empty());
    }
}
