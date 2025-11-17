//! CLI argument parsing for Renacer

use clap::{Parser, ValueEnum};

/// Output format for syscall traces
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text format (default)
    Text,
    /// JSON format for machine parsing
    Json,
    /// CSV format for spreadsheet analysis
    Csv,
}

#[derive(Parser, Debug)]
#[command(name = "renacer")]
#[command(version)]
#[command(about = "Pure Rust system call tracer with source correlation", long_about = None)]
pub struct Cli {
    /// Enable source code correlation using DWARF debug info
    #[arg(short, long)]
    pub source: bool,

    /// Filter syscalls to trace (e.g., -e trace=open,read,write or -e trace=file)
    #[arg(short = 'e', long = "expr", value_name = "EXPR")]
    pub filter: Option<String>,

    /// Show statistics summary (syscall counts and timing) instead of individual calls
    #[arg(short = 'c', long = "summary")]
    pub statistics: bool,

    /// Show time spent in each syscall
    #[arg(short = 'T', long = "timing")]
    pub timing: bool,

    /// Output format (text or json)
    #[arg(long = "format", value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Attach to running process by PID (mutually exclusive with command)
    #[arg(short = 'p', long = "pid", value_name = "PID")]
    pub pid: Option<i32>,

    /// Follow forks (trace child processes)
    #[arg(short = 'f', long = "follow-forks")]
    pub follow_forks: bool,

    /// Enable self-profiling to measure Renacer's own overhead
    #[arg(long = "profile-self")]
    pub profile_self: bool,

    /// Enable function-level timing with DWARF correlation
    #[arg(long = "function-time")]
    pub function_time: bool,

    /// Enable extended statistics with percentiles and anomaly detection (requires -c)
    #[arg(long = "stats-extended")]
    pub stats_extended: bool,

    /// Anomaly detection threshold in standard deviations (default: 3.0)
    #[arg(
        long = "anomaly-threshold",
        value_name = "SIGMA",
        default_value = "3.0"
    )]
    pub anomaly_threshold: f32,

    /// Enable real-time anomaly detection (Sprint 20)
    #[arg(long = "anomaly-realtime")]
    pub anomaly_realtime: bool,

    /// Sliding window size for real-time anomaly detection (default: 100)
    #[arg(
        long = "anomaly-window-size",
        value_name = "SIZE",
        default_value = "100"
    )]
    pub anomaly_window_size: usize,

    /// Command to trace (everything after --)
    #[arg(last = true)]
    pub command: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parses_command() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "hello"]);
        assert!(cli.command.is_some());
        let cmd = cli.command.unwrap();
        assert_eq!(cmd[0], "echo");
        assert_eq!(cmd[1], "hello");
    }

    #[test]
    fn test_cli_empty_without_command() {
        let cli = Cli::parse_from(["renacer"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_profile_self_flag() {
        let cli = Cli::parse_from(["renacer", "--profile-self", "--", "echo", "test"]);
        assert!(cli.profile_self);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_profile_self_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.profile_self);
    }

    #[test]
    fn test_cli_function_time_flag() {
        let cli = Cli::parse_from(["renacer", "--function-time", "--", "echo", "test"]);
        assert!(cli.function_time);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_function_time_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.function_time);
    }

    #[test]
    fn test_cli_stats_extended_flag() {
        let cli = Cli::parse_from(["renacer", "--stats-extended", "--", "echo", "test"]);
        assert!(cli.stats_extended);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_stats_extended_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.stats_extended);
    }

    #[test]
    fn test_cli_anomaly_threshold_default() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert_eq!(cli.anomaly_threshold, 3.0);
    }

    #[test]
    fn test_cli_anomaly_threshold_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--anomaly-threshold",
            "2.5",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.anomaly_threshold, 2.5);
    }

    #[test]
    fn test_cli_stats_extended_with_statistics() {
        let cli = Cli::parse_from(["renacer", "-c", "--stats-extended", "--", "echo", "test"]);
        assert!(cli.statistics);
        assert!(cli.stats_extended);
    }
}
