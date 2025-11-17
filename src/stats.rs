//! Syscall statistics tracking for -c mode
//!
//! Sprint 9-10: Statistics mode implementation

use std::collections::HashMap;

/// Statistics for a single syscall type
#[derive(Debug, Clone, Default)]
pub struct SyscallStats {
    /// Number of times this syscall was called
    pub count: u64,
    /// Number of errors (negative return values)
    pub errors: u64,
}

/// Tracks statistics for all syscalls
#[derive(Debug, Default)]
pub struct StatsTracker {
    /// Map from syscall name to statistics
    stats: HashMap<String, SyscallStats>,
}

impl StatsTracker {
    /// Create a new statistics tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a syscall execution
    pub fn record(&mut self, syscall_name: &str, result: i64) {
        let entry = self.stats.entry(syscall_name.to_string()).or_default();
        entry.count += 1;
        if result < 0 {
            entry.errors += 1;
        }
    }

    /// Print statistics summary to stdout
    pub fn print_summary(&self) {
        if self.stats.is_empty() {
            println!("No syscalls traced.");
            return;
        }

        // Calculate totals
        let total_calls: u64 = self.stats.values().map(|s| s.count).sum();
        let total_errors: u64 = self.stats.values().map(|s| s.errors).sum();

        // Sort by call count (descending)
        let mut sorted: Vec<_> = self.stats.iter().collect();
        sorted.sort_by(|a, b| b.1.count.cmp(&a.1.count));

        // Print header
        println!("% time     seconds  usecs/call     calls    errors syscall");
        println!("------ ----------- ----------- --------- --------- ----------------");

        // Print each syscall
        for (name, stats) in sorted {
            let percent = (stats.count as f64 / total_calls as f64) * 100.0;
            println!(
                "{:6.2} {:>11} {:>11} {:>9} {:>9} {}",
                percent,
                "0.000000", // TODO: actual timing in future
                0,          // TODO: usecs/call in future
                stats.count,
                if stats.errors > 0 {
                    stats.errors.to_string()
                } else {
                    String::new()
                },
                name
            );
        }

        // Print summary line
        println!("------ ----------- ----------- --------- --------- ----------------");
        println!(
            "100.00 {:>11} {:>11} {:>9} {:>9} total",
            "0.000000",
            0,
            total_calls,
            if total_errors > 0 {
                total_errors.to_string()
            } else {
                String::new()
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_tracker_records_calls() {
        let mut tracker = StatsTracker::new();
        tracker.record("open", 3);
        tracker.record("read", 10);
        tracker.record("read", 10);

        assert_eq!(tracker.stats.get("open").unwrap().count, 1);
        assert_eq!(tracker.stats.get("read").unwrap().count, 2);
    }

    #[test]
    fn test_stats_tracker_records_errors() {
        let mut tracker = StatsTracker::new();
        tracker.record("open", 3); // success
        tracker.record("open", -2); // error (ENOENT)
        tracker.record("open", -13); // error (EACCES)

        let stats = tracker.stats.get("open").unwrap();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.errors, 2);
    }

    #[test]
    fn test_empty_tracker() {
        let tracker = StatsTracker::new();
        // Should not panic
        tracker.print_summary();
    }
}
