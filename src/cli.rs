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

    /// Enable Isolation Forest-based outlier detection (Sprint 22)
    #[arg(long = "ml-outliers")]
    pub ml_outliers: bool,

    /// Contamination threshold for Isolation Forest (default: 0.1, range: 0.0-0.5)
    #[arg(
        long = "ml-outlier-threshold",
        value_name = "THRESHOLD",
        default_value = "0.1"
    )]
    pub ml_outlier_threshold: f32,

    /// Number of trees in Isolation Forest (default: 100, min: 10)
    #[arg(long = "ml-outlier-trees", value_name = "N", default_value = "100")]
    pub ml_outlier_trees: usize,

    /// Enable explainability for ML outlier detection (Sprint 22)
    #[arg(long = "explain")]
    pub explain: bool,

    /// Enable deep learning (Autoencoder) anomaly detection (Sprint 23)
    #[arg(long = "dl-anomaly")]
    pub dl_anomaly: bool,

    /// Reconstruction error threshold for Autoencoder (default: 2.0)
    #[arg(long = "dl-threshold", value_name = "THRESHOLD", default_value = "2.0")]
    pub dl_threshold: f32,

    /// Hidden layer size for Autoencoder (default: 3)
    #[arg(long = "dl-hidden-size", value_name = "SIZE", default_value = "3")]
    pub dl_hidden_size: usize,

    /// Number of training epochs for Autoencoder (default: 100)
    #[arg(long = "dl-epochs", value_name = "N", default_value = "100")]
    pub dl_epochs: usize,

    /// Path to transpiler source map JSON file (Sprint 24)
    #[arg(long = "transpiler-map", value_name = "FILE")]
    pub transpiler_map: Option<String>,

    /// Show verbose transpiler context (Python/Rust correlation) (Sprint 25)
    #[arg(long = "show-transpiler-context")]
    pub show_transpiler_context: bool,

    /// Rewrite stack traces to show original source locations (Sprint 26)
    #[arg(long = "rewrite-stacktrace")]
    pub rewrite_stacktrace: bool,

    /// Rewrite compilation errors to show original source locations (Sprint 27)
    #[arg(long = "rewrite-errors")]
    pub rewrite_errors: bool,

    /// Trace transpiler compile-time decisions for debugging (Sprint 26)
    #[arg(long = "trace-transpiler-decisions")]
    pub trace_transpiler_decisions: bool,

    /// OpenTelemetry OTLP endpoint for trace export (Sprint 30)
    #[arg(long = "otlp-endpoint", value_name = "URL")]
    pub otlp_endpoint: Option<String>,

    /// Service name for OpenTelemetry traces (Sprint 30)
    #[arg(
        long = "otlp-service-name",
        value_name = "NAME",
        default_value = "renacer"
    )]
    pub otlp_service_name: String,

    /// W3C Trace Context for distributed tracing (Sprint 33)
    ///
    /// Inject trace context to create Renacer spans as children of application spans.
    /// Format: version-trace_id-parent_id-trace_flags
    /// Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
    ///
    /// If not provided, checks TRACEPARENT or OTEL_TRACEPARENT environment variables.
    /// If no context found, creates new root trace (existing behavior).
    #[arg(long = "trace-parent", value_name = "TRACEPARENT")]
    pub trace_parent: Option<String>,

    /// Enable compute block tracing (Trueno SIMD operations) - Sprint 32
    ///
    /// Exports statistical computation blocks (e.g., calculate_statistics) as
    /// OpenTelemetry spans. Uses adaptive sampling: only traces blocks with
    /// duration >= threshold (default: 100μs). Toyota Way compliant: safe by
    /// default, cannot DoS tracing backend.
    #[arg(long = "trace-compute")]
    pub trace_compute: bool,

    /// Trace ALL compute blocks (bypass adaptive sampling threshold) - Sprint 32
    ///
    /// Debug mode: traces even fast compute blocks (<100μs). Use for development
    /// and debugging only. Can generate high span volume (~500 spans/sec).
    /// Requires --trace-compute flag.
    #[arg(long = "trace-compute-all", requires = "trace_compute")]
    pub trace_compute_all: bool,

    /// Custom threshold for compute block tracing (microseconds) - Sprint 32
    ///
    /// Only trace compute blocks with duration >= threshold. Default: 100μs.
    /// Lower values increase span volume. Requires --trace-compute flag.
    #[arg(
        long = "trace-compute-threshold",
        value_name = "MICROS",
        default_value = "100",
        requires = "trace_compute"
    )]
    pub trace_compute_threshold: u64,

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

    #[test]
    fn test_cli_rewrite_stacktrace_flag() {
        let cli = Cli::parse_from(["renacer", "--rewrite-stacktrace", "--", "echo", "test"]);
        assert!(cli.rewrite_stacktrace);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_rewrite_stacktrace_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.rewrite_stacktrace);
    }

    #[test]
    fn test_cli_rewrite_stacktrace_with_transpiler_map() {
        let cli = Cli::parse_from([
            "renacer",
            "--transpiler-map",
            "map.json",
            "--rewrite-stacktrace",
            "--show-transpiler-context",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.transpiler_map.is_some());
        assert!(cli.rewrite_stacktrace);
        assert!(cli.show_transpiler_context);
    }

    #[test]
    fn test_cli_rewrite_errors_flag() {
        let cli = Cli::parse_from(["renacer", "--rewrite-errors", "--", "echo", "test"]);
        assert!(cli.rewrite_errors);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_rewrite_errors_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.rewrite_errors);
    }

    #[test]
    fn test_cli_rewrite_errors_with_transpiler_map() {
        let cli = Cli::parse_from([
            "renacer",
            "--transpiler-map",
            "map.json",
            "--rewrite-errors",
            "--show-transpiler-context",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.transpiler_map.is_some());
        assert!(cli.rewrite_errors);
        assert!(cli.show_transpiler_context);
    }

    #[test]
    fn test_cli_ml_outliers_flag() {
        let cli = Cli::parse_from(["renacer", "--ml-outliers", "--", "echo", "test"]);
        assert!(cli.ml_outliers);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_trace_transpiler_decisions_flag() {
        let cli = Cli::parse_from([
            "renacer",
            "--trace-transpiler-decisions",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.trace_transpiler_decisions);
        assert!(cli.command.is_some());
    }

    #[test]
    fn test_cli_ml_outliers_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.ml_outliers);
    }

    #[test]
    fn test_cli_trace_transpiler_decisions_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.trace_transpiler_decisions);
    }

    #[test]
    fn test_cli_ml_outlier_threshold_default() {
        let cli = Cli::parse_from(["renacer", "--ml-outliers", "--", "echo", "test"]);
        assert_eq!(cli.ml_outlier_threshold, 0.1);
    }

    #[test]
    fn test_cli_trace_transpiler_decisions_with_transpiler_map() {
        let cli = Cli::parse_from([
            "renacer",
            "--transpiler-map",
            "map.json",
            "--trace-transpiler-decisions",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.transpiler_map.is_some());
        assert!(cli.trace_transpiler_decisions);
    }

    #[test]
    fn test_cli_ml_outlier_threshold_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--ml-outliers",
            "--ml-outlier-threshold",
            "0.15",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.ml_outlier_threshold, 0.15);
    }

    #[test]
    fn test_cli_ml_outlier_trees_default() {
        let cli = Cli::parse_from(["renacer", "--ml-outliers", "--", "echo", "test"]);
        assert_eq!(cli.ml_outlier_trees, 100);
    }

    #[test]
    fn test_cli_ml_outlier_trees_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--ml-outliers",
            "--ml-outlier-trees",
            "150",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.ml_outlier_trees, 150);
    }

    #[test]
    fn test_cli_explain_flag() {
        let cli = Cli::parse_from([
            "renacer",
            "--ml-outliers",
            "--explain",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.ml_outliers);
        assert!(cli.explain);
    }

    #[test]
    fn test_cli_explain_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.explain);
    }

    #[test]
    fn test_cli_ml_outliers_with_statistics() {
        let cli = Cli::parse_from(["renacer", "-c", "--ml-outliers", "--", "echo", "test"]);
        assert!(cli.statistics);
        assert!(cli.ml_outliers);
    }

    #[test]
    fn test_cli_ml_outliers_with_kmeans() {
        let cli = Cli::parse_from([
            "renacer",
            "--ml-outliers",
            "--ml-anomaly",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.ml_outliers);
        assert!(cli.ml_anomaly);
    }

    // Sprint 23: Deep Learning / Autoencoder tests
    #[test]
    fn test_cli_dl_anomaly_flag() {
        let cli = Cli::parse_from(["renacer", "--dl-anomaly", "--", "echo", "test"]);
        assert!(cli.dl_anomaly);
    }

    #[test]
    fn test_cli_dl_anomaly_default_false() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(!cli.dl_anomaly);
    }

    #[test]
    fn test_cli_dl_threshold_default() {
        let cli = Cli::parse_from(["renacer", "--dl-anomaly", "--", "echo", "test"]);
        assert_eq!(cli.dl_threshold, 2.0);
    }

    #[test]
    fn test_cli_dl_threshold_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--dl-anomaly",
            "--dl-threshold",
            "3.0",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.dl_threshold, 3.0);
    }

    #[test]
    fn test_cli_dl_hidden_size_default() {
        let cli = Cli::parse_from(["renacer", "--dl-anomaly", "--", "echo", "test"]);
        assert_eq!(cli.dl_hidden_size, 3);
    }

    #[test]
    fn test_cli_dl_hidden_size_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--dl-anomaly",
            "--dl-hidden-size",
            "5",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.dl_hidden_size, 5);
    }

    #[test]
    fn test_cli_dl_epochs_default() {
        let cli = Cli::parse_from(["renacer", "--dl-anomaly", "--", "echo", "test"]);
        assert_eq!(cli.dl_epochs, 100);
    }

    #[test]
    fn test_cli_dl_epochs_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--dl-anomaly",
            "--dl-epochs",
            "200",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.dl_epochs, 200);
    }

    #[test]
    fn test_cli_dl_anomaly_with_statistics() {
        let cli = Cli::parse_from(["renacer", "-c", "--dl-anomaly", "--", "echo", "test"]);
        assert!(cli.statistics);
        assert!(cli.dl_anomaly);
    }

    #[test]
    fn test_cli_dl_anomaly_with_other_ml() {
        let cli = Cli::parse_from([
            "renacer",
            "--dl-anomaly",
            "--ml-anomaly",
            "--ml-outliers",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.dl_anomaly);
        assert!(cli.ml_anomaly);
        assert!(cli.ml_outliers);
    }

    // Sprint 30: OpenTelemetry OTLP Export tests
    #[test]
    fn test_cli_otlp_endpoint_flag() {
        let cli = Cli::parse_from([
            "renacer",
            "--otlp-endpoint",
            "http://localhost:4317",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.otlp_endpoint.is_some());
        assert_eq!(cli.otlp_endpoint.unwrap(), "http://localhost:4317");
    }

    #[test]
    fn test_cli_otlp_endpoint_default_none() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert!(cli.otlp_endpoint.is_none());
    }

    #[test]
    fn test_cli_otlp_service_name_default() {
        let cli = Cli::parse_from(["renacer", "--", "echo", "test"]);
        assert_eq!(cli.otlp_service_name, "renacer");
    }

    #[test]
    fn test_cli_otlp_service_name_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--otlp-service-name",
            "my-app",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.otlp_service_name, "my-app");
    }

    #[test]
    fn test_cli_otlp_with_endpoint_and_service() {
        let cli = Cli::parse_from([
            "renacer",
            "--otlp-endpoint",
            "http://jaeger:4317",
            "--otlp-service-name",
            "traced-app",
            "--",
            "echo",
            "test",
        ]);
        assert_eq!(cli.otlp_endpoint.unwrap(), "http://jaeger:4317");
        assert_eq!(cli.otlp_service_name, "traced-app");
    }

    #[test]
    fn test_cli_otlp_with_statistics() {
        let cli = Cli::parse_from([
            "renacer",
            "-c",
            "--otlp-endpoint",
            "http://localhost:4317",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.statistics);
        assert!(cli.otlp_endpoint.is_some());
    }

    #[test]
    fn test_cli_otlp_with_timing() {
        let cli = Cli::parse_from([
            "renacer",
            "-T",
            "--otlp-endpoint",
            "http://localhost:4317",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.timing);
        assert!(cli.otlp_endpoint.is_some());
    }

    // Sprint 32: Compute Block Tracing tests
    #[test]
    fn test_cli_trace_compute_flag() {
        let cli = Cli::parse_from(["renacer", "--trace-compute", "--", "echo", "test"]);
        assert!(cli.trace_compute);
        assert!(!cli.trace_compute_all);
        assert_eq!(cli.trace_compute_threshold, 100); // default
    }

    #[test]
    fn test_cli_trace_compute_all_flag() {
        let cli = Cli::parse_from([
            "renacer",
            "--trace-compute",
            "--trace-compute-all",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.trace_compute);
        assert!(cli.trace_compute_all);
    }

    #[test]
    fn test_cli_trace_compute_threshold_custom() {
        let cli = Cli::parse_from([
            "renacer",
            "--trace-compute",
            "--trace-compute-threshold",
            "50",
            "--",
            "echo",
            "test",
        ]);
        assert!(cli.trace_compute);
        assert_eq!(cli.trace_compute_threshold, 50);
    }

    #[test]
    fn test_cli_trace_compute_with_otlp() {
        let cli = Cli::parse_from([
            "renacer",
            "--otlp-endpoint",
            "http://localhost:4317",
            "--trace-compute",
            "-c",
            "--stats-extended",
            "--",
            "cargo",
            "build",
        ]);
        assert!(cli.otlp_endpoint.is_some());
        assert!(cli.trace_compute);
        assert!(cli.statistics);
        assert!(cli.stats_extended);
    }

    #[test]
    fn test_cli_trace_compute_all_requires_trace_compute() {
        // Should fail because --trace-compute-all requires --trace-compute
        let result = Cli::try_parse_from(["renacer", "--trace-compute-all", "--", "echo", "test"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_trace_compute_threshold_requires_trace_compute() {
        // Should fail because --trace-compute-threshold requires --trace-compute
        let result = Cli::try_parse_from([
            "renacer",
            "--trace-compute-threshold",
            "50",
            "--",
            "echo",
            "test",
        ]);
        assert!(result.is_err());
    }
}
