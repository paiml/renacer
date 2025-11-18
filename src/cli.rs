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
    /// HTML format for visual reports (Sprint 22)
    Html,
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

    /// Enable HPU-accelerated analysis (GPU if available) (Sprint 21)
    #[arg(long = "hpu-analysis")]
    pub hpu_analysis: bool,

    /// Force CPU backend (disable GPU acceleration)
    #[arg(long = "hpu-cpu-only")]
    pub hpu_cpu_only: bool,

    /// Enable ML-based anomaly detection using Aprender (Sprint 23)
    #[arg(long = "ml-anomaly")]
    pub ml_anomaly: bool,

    /// Number of clusters for ML anomaly detection (default: 3, min: 2)
    #[arg(long = "ml-clusters", value_name = "N", default_value = "3")]
    pub ml_clusters: usize,

    /// Compare ML results with z-score anomaly detection
    #[arg(long = "ml-compare")]
    pub ml_compare: bool,

    /// Path to transpiler source map JSON file (Sprint 24)
    #[arg(long = "transpiler-map", value_name = "FILE")]
    pub transpiler_map: Option<String>,

    /// Show verbose transpiler context (Python/Rust correlation) (Sprint 25)
    #[arg(long = "show-transpiler-context")]
    pub show_transpiler_context: bool,

    /// Enable debug tracing output to stderr
    #[arg(long = "debug")]
    pub debug: bool,

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

    #[test]
    fn test_cli_hpu_analysis_flag() {
        let cli = Cli::parse_from(["renacer", "--hpu-analysis", "--", "echo", "test"]);
        assert!(cli.hpu_analysis);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_hpu_analysis_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.hpu_analysis);
    }

    #[test]
    fn test_cli_hpu_cpu_only_flag() {
        let cli = Cli::parse_from(["renacer", "--hpu-cpu-only", "--", "echo", "test"]);
        assert!(cli.hpu_cpu_only);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_hpu_cpu_only_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.hpu_cpu_only);
    }

    #[test]
    fn test_cli_hpu_with_statistics() {
        let cli = Cli::parse_from(["renacer", "-c", "--hpu-analysis", "--", "echo", "test"]);
        assert!(cli.statistics);
        assert!(cli.hpu_analysis);
    }

    #[test]
    fn test_cli_hpu_analysis_with_cpu_only() {
        let cli = Cli::parse_from([
            "renacer",
            "--hpu-analysis",
            "--hpu-cpu-only",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.hpu_analysis);
        assert!(cli.hpu_cpu_only);
    }

    #[test]
    fn test_cli_ml_anomaly_flag() {
        let cli = Cli::parse_from(["renacer", "--ml-anomaly", "--", "echo", "test"]);
        assert!(cli.ml_anomaly);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_ml_anomaly_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.ml_anomaly);
    }

    #[test]
    fn test_cli_ml_clusters_default() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert_eq!(cli.ml_clusters, 3);
    }

    #[test]
    fn test_cli_ml_clusters_custom() {
        let cli = Cli::parse_from(["renacer", "--ml-clusters", "5", "--", "echo", "test"]);
        assert_eq!(cli.ml_clusters, 5);
    }

    #[test]
    fn test_cli_ml_compare_flag() {
        let cli = Cli::parse_from(["renacer", "--ml-compare", "--", "echo", "test"]);
        assert!(cli.ml_compare);
    }

    #[test]
    fn test_cli_ml_compare_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.ml_compare);
    }

    #[test]
    fn test_cli_ml_anomaly_with_statistics() {
        let cli = Cli::parse_from(["renacer", "-c", "--ml-anomaly", "--", "echo", "test"]);
        assert!(cli.statistics);
        assert!(cli.ml_anomaly);
    }

    #[test]
    fn test_cli_transpiler_map_flag() {
        let cli = Cli::parse_from([
            "renacer",
            "--transpiler-map",
            "test.sourcemap.json",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.transpiler_map.as_deref(), Some("test.sourcemap.json"));
    }

    #[test]
    fn test_cli_transpiler_map_default_none() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(cli.transpiler_map.is_none());
    }

    #[test]
    fn test_cli_show_transpiler_context_flag() {
        let cli = Cli::parse_from(["renacer", "--show-transpiler-context", "--", "echo", "test"]);
        assert!(cli.show_transpiler_context);
    }

    #[test]
    fn test_cli_show_transpiler_context_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.show_transpiler_context);
    }

    #[test]
    fn test_cli_transpiler_map_with_function_time() {
        let cli = Cli::parse_from([
            "renacer",
            "--transpiler-map",
            "map.json",
            "--function-time",
            "--show-transpiler-context",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.transpiler_map.is_some());
        assert!(cli.function_time);
        assert!(cli.show_transpiler_context);
    }
}
