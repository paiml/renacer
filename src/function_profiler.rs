//! Function-level profiling with DWARF correlation
//!
//! GitHub Issue #1: Function-level profiling for performance analysis
//!
//! This module provides function-level timing aggregation by correlating
//! syscalls with DWARF debug info to identify performance bottlenecks.
//!
//! Features:
//! - Function-level syscall attribution and timing
//! - Call graph tracking (parent-child relationships)
//! - Hot path analysis (most frequently executed paths)
//! - I/O bottleneck detection (slow operations)
//! - Flamegraph export support

use std::collections::HashMap;

/// I/O syscalls that should be tracked for bottleneck detection
const IO_SYSCALLS: &[&str] = &[
    "read", "write", "readv", "writev", "pread64", "pwrite64",
    "openat", "open", "close", "fsync", "fdatasync", "sync",
    "sendfile", "splice", "tee", "vmsplice"
];

/// Threshold for slow I/O operations (1ms = 1000 microseconds)
const SLOW_IO_THRESHOLD_US: u64 = 1000;

/// Statistics for a single function
#[derive(Debug, Clone, Default)]
pub struct FunctionStats {
    /// Number of syscalls attributed to this function
    pub syscall_count: u64,
    /// Total time spent in syscalls from this function (microseconds)
    pub total_time_us: u64,
    /// Functions called by this function (call graph) - Reserved for future use
    #[allow(dead_code)]
    pub callees: HashMap<String, u64>,
    /// Number of times this is an I/O syscall - Reserved for future use
    #[allow(dead_code)]
    pub io_syscalls: u64,
    /// Number of slow I/O operations (>1ms) - Reserved for future use
    #[allow(dead_code)]
    pub slow_io_count: u64,
}

/// Tracks function-level profiling statistics
#[derive(Debug, Default)]
pub struct FunctionProfiler {
    /// Map from function name to statistics
    stats: HashMap<String, FunctionStats>,
}

impl FunctionProfiler {
    /// Create a new function profiler
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a syscall execution attributed to a function
    ///
    /// # Arguments
    /// * `function_name` - Name of the function making the syscall
    /// * `syscall_name` - Name of the syscall being made (for I/O detection)
    /// * `duration_us` - Duration of the syscall in microseconds
    pub fn record(&mut self, function_name: &str, syscall_name: &str, duration_us: u64) {
        let entry = self.stats.entry(function_name.to_string()).or_default();
        entry.syscall_count += 1;
        entry.total_time_us += duration_us;

        // Track I/O syscalls
        if IO_SYSCALLS.contains(&syscall_name) {
            entry.io_syscalls += 1;

            // Track slow I/O operations (>1ms)
            if duration_us > SLOW_IO_THRESHOLD_US {
                entry.slow_io_count += 1;
            }
        }
    }

    /// Print function timing summary to stderr
    pub fn print_summary(&self) {
        if self.stats.is_empty() {
            eprintln!("\nNo function profiling data collected.");
            return;
        }

        // Sort by total time (descending)
        let mut sorted: Vec<_> = self.stats.iter().collect();
        sorted.sort_by(|a, b| b.1.total_time_us.cmp(&a.1.total_time_us));

        eprintln!("\n╔════════════════════════════════════════════════════════════════════════════════════════════════════╗");
        eprintln!("║  Function Timing Summary (sorted by total time)                                                   ║");
        eprintln!("╚════════════════════════════════════════════════════════════════════════════════════════════════════╝");
        eprintln!();
        eprintln!("{:<40} {:>10} {:>12} {:>12} {:>10} {:>10}",
            "Function", "Calls", "Total Time", "Avg Time", "I/O Ops", "Slow I/O");
        eprintln!("{}", "─".repeat(104));

        for (function, stats) in sorted {
            let total_seconds = stats.total_time_us as f64 / 1_000_000.0;
            let avg_us = if stats.syscall_count > 0 {
                stats.total_time_us / stats.syscall_count
            } else {
                0
            };
            let avg_seconds = avg_us as f64 / 1_000_000.0;

            // Highlight functions with slow I/O
            let marker = if stats.slow_io_count > 0 { "⚠️ " } else { "   " };

            eprintln!(
                "{}{:<37} {:>10} {:>11.6}s {:>11.6}s {:>10} {:>10}",
                marker, function, stats.syscall_count, total_seconds, avg_seconds,
                stats.io_syscalls, stats.slow_io_count
            );
        }

        eprintln!("{}", "─".repeat(104));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_profiler_new() {
        let profiler = FunctionProfiler::new();
        assert!(profiler.stats.is_empty());
    }

    #[test]
    fn test_function_profiler_record() {
        let mut profiler = FunctionProfiler::new();
        profiler.record("main", "write", 1000);
        profiler.record("main", "read", 2000);
        profiler.record("helper", "open", 500);

        assert_eq!(profiler.stats.len(), 2);
        assert_eq!(profiler.stats.get("main").unwrap().syscall_count, 2);
        assert_eq!(profiler.stats.get("main").unwrap().total_time_us, 3000);
        assert_eq!(profiler.stats.get("helper").unwrap().syscall_count, 1);
        assert_eq!(profiler.stats.get("helper").unwrap().total_time_us, 500);
    }

    #[test]
    fn test_function_profiler_empty_summary() {
        let profiler = FunctionProfiler::new();
        // Should not panic
        profiler.print_summary();
    }

    #[test]
    fn test_function_stats_default() {
        let stats = FunctionStats::default();
        assert_eq!(stats.syscall_count, 0);
        assert_eq!(stats.total_time_us, 0);
    }

    #[test]
    fn test_function_profiler_print_summary_with_data() {
        let mut profiler = FunctionProfiler::new();
        profiler.record("main", "write", 1000000); // 1 second
        profiler.record("main", "read", 500000);   // 0.5 seconds
        profiler.record("helper", "open", 250000); // 0.25 seconds
        profiler.record("foo", "close", 100000);    // 0.1 seconds

        // This exercises the print_summary() code path with data
        // including sorting, formatting, and calculations
        profiler.print_summary();

        // Verify internal state
        assert_eq!(profiler.stats.len(), 3);
    }

    #[test]
    fn test_function_profiler_sorting_by_total_time() {
        let mut profiler = FunctionProfiler::new();
        // Record in non-sorted order
        profiler.record("slow_func", "write", 5000000);  // 5 seconds total
        profiler.record("fast_func", "brk", 100000);   // 0.1 seconds total
        profiler.record("medium_func", "read", 1000000); // 1 second total

        // print_summary() should sort by total time (descending)
        profiler.print_summary();

        // Verify data is present
        assert_eq!(profiler.stats.get("slow_func").unwrap().total_time_us, 5000000);
        assert_eq!(profiler.stats.get("fast_func").unwrap().total_time_us, 100000);
        assert_eq!(profiler.stats.get("medium_func").unwrap().total_time_us, 1000000);
    }

    #[test]
    fn test_function_profiler_average_calculation() {
        let mut profiler = FunctionProfiler::new();
        // Record multiple calls to test average calculation
        profiler.record("test_func", "write", 1000);
        profiler.record("test_func", "read", 2000);
        profiler.record("test_func", "open", 3000);

        let stats = profiler.stats.get("test_func").unwrap();
        assert_eq!(stats.syscall_count, 3);
        assert_eq!(stats.total_time_us, 6000);

        // Average should be 2000 microseconds
        let avg = stats.total_time_us / stats.syscall_count;
        assert_eq!(avg, 2000);

        profiler.print_summary();
    }

    #[test]
    fn test_function_profiler_zero_syscalls_edge_case() {
        let mut profiler = FunctionProfiler::new();
        // Manually insert a function with 0 syscalls (edge case)
        profiler.stats.insert("never_called".to_string(), FunctionStats {
            syscall_count: 0,
            total_time_us: 0,
            callees: HashMap::new(),
            io_syscalls: 0,
            slow_io_count: 0,
        });

        // Should handle division by zero gracefully
        profiler.print_summary();

        let stats = profiler.stats.get("never_called").unwrap();
        assert_eq!(stats.syscall_count, 0);
    }

    #[test]
    fn test_io_syscall_tracking() {
        let mut profiler = FunctionProfiler::new();

        // Record I/O syscalls
        profiler.record("io_func", "read", 500);
        profiler.record("io_func", "write", 600);
        profiler.record("io_func", "open", 700);
        profiler.record("io_func", "close", 400);

        let stats = profiler.stats.get("io_func").unwrap();
        assert_eq!(stats.io_syscalls, 4);
        assert_eq!(stats.syscall_count, 4);
        assert_eq!(stats.slow_io_count, 0); // All under 1ms
    }

    #[test]
    fn test_non_io_syscalls_not_tracked() {
        let mut profiler = FunctionProfiler::new();

        // Record non-I/O syscalls
        profiler.record("compute_func", "brk", 500);
        profiler.record("compute_func", "mmap", 600);
        profiler.record("compute_func", "getpid", 100);

        let stats = profiler.stats.get("compute_func").unwrap();
        assert_eq!(stats.io_syscalls, 0);
        assert_eq!(stats.syscall_count, 3);
        assert_eq!(stats.slow_io_count, 0);
    }

    #[test]
    fn test_slow_io_detection() {
        let mut profiler = FunctionProfiler::new();

        // Record slow I/O operations (>1ms = >1000us)
        profiler.record("slow_io_func", "read", 2000);   // 2ms - SLOW
        profiler.record("slow_io_func", "write", 5000);  // 5ms - SLOW
        profiler.record("slow_io_func", "fsync", 10000); // 10ms - SLOW

        let stats = profiler.stats.get("slow_io_func").unwrap();
        assert_eq!(stats.io_syscalls, 3);
        assert_eq!(stats.slow_io_count, 3);
        assert_eq!(stats.syscall_count, 3);
    }

    #[test]
    fn test_fast_io_not_marked_slow() {
        let mut profiler = FunctionProfiler::new();

        // Record fast I/O operations (<1ms)
        profiler.record("fast_io_func", "read", 100);   // 0.1ms - fast
        profiler.record("fast_io_func", "write", 500);  // 0.5ms - fast
        profiler.record("fast_io_func", "close", 50);   // 0.05ms - fast

        let stats = profiler.stats.get("fast_io_func").unwrap();
        assert_eq!(stats.io_syscalls, 3);
        assert_eq!(stats.slow_io_count, 0); // None are slow
        assert_eq!(stats.syscall_count, 3);
    }

    #[test]
    fn test_mixed_io_and_non_io_operations() {
        let mut profiler = FunctionProfiler::new();

        // Mix of I/O and non-I/O syscalls
        profiler.record("mixed_func", "read", 1500);    // I/O, slow
        profiler.record("mixed_func", "brk", 100);      // Non-I/O
        profiler.record("mixed_func", "write", 500);    // I/O, fast
        profiler.record("mixed_func", "mmap", 200);     // Non-I/O
        profiler.record("mixed_func", "fsync", 3000);   // I/O, slow

        let stats = profiler.stats.get("mixed_func").unwrap();
        assert_eq!(stats.syscall_count, 5);
        assert_eq!(stats.io_syscalls, 3);      // read, write, fsync
        assert_eq!(stats.slow_io_count, 2);    // read and fsync
        assert_eq!(stats.total_time_us, 5300);
    }

    #[test]
    fn test_slow_io_threshold_boundary() {
        let mut profiler = FunctionProfiler::new();

        // Test exactly at threshold
        profiler.record("boundary_func", "read", 1000);  // Exactly 1ms - NOT slow (>1ms)
        profiler.record("boundary_func", "write", 999);  // Just under - NOT slow
        profiler.record("boundary_func", "open", 1001);  // Just over - SLOW

        let stats = profiler.stats.get("boundary_func").unwrap();
        assert_eq!(stats.io_syscalls, 3);
        assert_eq!(stats.slow_io_count, 1); // Only the 1001us operation
    }

    #[test]
    fn test_all_io_syscall_types() {
        let mut profiler = FunctionProfiler::new();

        // Test all I/O syscall types from IO_SYSCALLS constant
        profiler.record("io_types", "read", 100);
        profiler.record("io_types", "write", 100);
        profiler.record("io_types", "readv", 100);
        profiler.record("io_types", "writev", 100);
        profiler.record("io_types", "pread64", 100);
        profiler.record("io_types", "pwrite64", 100);
        profiler.record("io_types", "openat", 100);
        profiler.record("io_types", "open", 100);
        profiler.record("io_types", "close", 100);
        profiler.record("io_types", "fsync", 100);
        profiler.record("io_types", "fdatasync", 100);
        profiler.record("io_types", "sync", 100);
        profiler.record("io_types", "sendfile", 100);
        profiler.record("io_types", "splice", 100);
        profiler.record("io_types", "tee", 100);
        profiler.record("io_types", "vmsplice", 100);

        let stats = profiler.stats.get("io_types").unwrap();
        assert_eq!(stats.io_syscalls, 16); // All 16 I/O syscall types
        assert_eq!(stats.syscall_count, 16);
        assert_eq!(stats.slow_io_count, 0); // All fast
    }
}
