//! Validation module for golden trace comparison
//!
//! Implements `renacer validate` subcommand per specification
//! `docs/specifications/apr-runtime-model-tracing-support.md`
//!
//! # Overview
//!
//! The validate module enables regression detection by comparing
//! syscall traces against recorded baselines. It supports:
//!
//! - Golden trace generation (`--generate`)
//! - Baseline comparison (`--baseline`)
//! - Configurable timing tolerance
//! - Multiple output formats (text, JSON, `JUnit`)
//!
//! # Exit Codes
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0 | Validation passed |
//! | 1 | Regression detected |
//! | 2 | Baseline not found |
//! | 3 | Invalid baseline format |
//! | 4 | Command execution error |
//! | 5 | Configuration error |

pub mod comparison;
pub mod config;
pub mod error;
pub mod golden_trace;
pub mod output;

pub use comparison::{
    compare_syscalls, compare_timing, is_timing_regression, ComparisonResult, ComparisonSummary,
    MismatchType, SyscallMismatch, TimingRegression,
};
pub use config::{ValidateConfig, ValidationOutputFormat};
pub use error::{Result, ValidateError};
pub use golden_trace::{
    generate_baseline, load_baseline, GoldenBaseline, PlatformInfo, SyscallTimingStats,
    TimingStats, ToleranceConfig, TraceFlags, TraceHeader, TraceManifest, TraceStatistics,
    TraceSyscallEntry, TRACE_MAGIC, TRACE_MAGIC_END, TRACE_VERSION,
};
pub use output::{format_json_report, format_junit_report, format_result, format_text_report};

/// Exit codes per specification Section 3.4
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ValidateExitCode {
    /// Validation passed
    Passed = 0,
    /// Regression detected
    Failed = 1,
    /// Baseline not found
    BaselineNotFound = 2,
    /// Invalid baseline format
    InvalidBaseline = 3,
    /// Command execution error
    CommandError = 4,
    /// Configuration error
    ConfigError = 5,
}

impl ValidateExitCode {
    /// Convert to process exit code
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// Run validation with the given configuration
///
/// # Arguments
///
/// * `command` - The command to trace and validate
/// * `config` - Validation configuration
///
/// # Returns
///
/// Exit code indicating validation result
pub fn run_validate(command: &[String], config: &ValidateConfig) -> ValidateExitCode {
    // Generate mode
    if let Some(ref generate_dir) = config.generate_dir {
        let command_refs: Vec<&str> = command.iter().map(String::as_str).collect();
        match generate_baseline(&command_refs, generate_dir, config) {
            Ok(()) => {
                eprintln!("Generated baseline at: {}", generate_dir.display());
                return ValidateExitCode::Passed;
            }
            Err(e) => {
                eprintln!("Failed to generate baseline: {e}");
                return ValidateExitCode::CommandError;
            }
        }
    }

    // Validate mode
    let baseline_dir = match &config.baseline_dir {
        Some(dir) => dir,
        None => {
            eprintln!("Error: Either --baseline or --generate must be specified");
            return ValidateExitCode::ConfigError;
        }
    };

    // Load baseline
    let baseline = match load_baseline(baseline_dir) {
        Ok(b) => b,
        Err(ValidateError::BaselineNotFound { path }) => {
            eprintln!("Baseline not found: {}", path.display());
            return ValidateExitCode::BaselineNotFound;
        }
        Err(ValidateError::InvalidManifest { reason }) => {
            eprintln!("Invalid baseline: {reason}");
            return ValidateExitCode::InvalidBaseline;
        }
        Err(e) => {
            eprintln!("Error loading baseline: {e}");
            return ValidateExitCode::InvalidBaseline;
        }
    };

    // Trace the command
    use crate::filter::SyscallFilter;
    use crate::tracer::{trace_command, TracerConfig};

    let tracer_config = TracerConfig {
        timing_mode: true,
        statistics_mode: true,
        filter: SyscallFilter::all(),
        ..TracerConfig::default()
    };

    if let Err(e) = trace_command(command, tracer_config) {
        eprintln!("Command execution failed: {e}");
        return ValidateExitCode::CommandError;
    }

    // For now, use empty actual data (full implementation would extract from trace)
    let actual_syscalls: Vec<TraceSyscallEntry> = Vec::new();
    let actual_timing = std::collections::HashMap::new();

    // Perform comparison
    let comparison_result = match comparison::validate_against_baseline(
        &baseline,
        &actual_syscalls,
        &actual_timing,
        config.tolerance_percent,
        config.strict_mode,
        config.ignore_timing,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Comparison error: {e}");
            return ValidateExitCode::Failed;
        }
    };

    // Format and print output
    let output = output::format_result(&comparison_result, config.output_format);
    print!("{output}");

    if comparison_result.passed {
        ValidateExitCode::Passed
    } else {
        ValidateExitCode::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_exit_code_values() {
        assert_eq!(ValidateExitCode::Passed.code(), 0);
        assert_eq!(ValidateExitCode::Failed.code(), 1);
        assert_eq!(ValidateExitCode::BaselineNotFound.code(), 2);
        assert_eq!(ValidateExitCode::InvalidBaseline.code(), 3);
        assert_eq!(ValidateExitCode::CommandError.code(), 4);
        assert_eq!(ValidateExitCode::ConfigError.code(), 5);
    }

    #[test]
    fn test_config_requires_baseline_or_generate() {
        let config = ValidateConfig::default();
        let result = run_validate(&["echo".to_string()], &config);
        assert_eq!(result, ValidateExitCode::ConfigError);
    }

    #[test]
    fn test_run_validate_baseline_not_found() {
        let config = ValidateConfig::default()
            .with_baseline(std::path::PathBuf::from("/nonexistent/baseline"));
        let result = run_validate(&["echo".to_string()], &config);
        assert_eq!(result, ValidateExitCode::BaselineNotFound);
    }

    #[test]
    fn test_run_validate_invalid_baseline() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        // Create invalid manifest
        std::fs::write(temp_dir.path().join("manifest.json"), "invalid json")
            .expect("failed to write manifest");

        let config = ValidateConfig::default().with_baseline(temp_dir.path().to_path_buf());
        let result = run_validate(&["echo".to_string()], &config);
        assert_eq!(result, ValidateExitCode::InvalidBaseline);
    }

    #[test]
    fn test_exit_code_clone() {
        let code = ValidateExitCode::Passed;
        let cloned = code;
        assert_eq!(cloned.code(), 0);
    }

    #[test]
    fn test_exit_code_debug() {
        let code = ValidateExitCode::Failed;
        let debug = format!("{:?}", code);
        assert!(debug.contains("Failed"));
    }

    #[test]
    fn test_exit_code_copy() {
        let code = ValidateExitCode::BaselineNotFound;
        let copied: ValidateExitCode = code;
        assert_eq!(copied, code);
    }

    #[test]
    fn test_run_validate_other_errors() {
        // Test the generic error path in load_baseline
        // This tests ValidateError variants like Io or other errors
        let config = ValidateConfig::default()
            .with_baseline(std::path::PathBuf::from("/nonexistent/path/baseline"));
        let result = run_validate(&["echo".to_string()], &config);
        // BaselineNotFound is expected
        assert_eq!(result, ValidateExitCode::BaselineNotFound);
    }

    #[test]
    fn test_run_validate_all_exit_codes() {
        // Passed
        assert_eq!(ValidateExitCode::Passed.code(), 0);
        // Failed
        assert_eq!(ValidateExitCode::Failed.code(), 1);
        // BaselineNotFound
        assert_eq!(ValidateExitCode::BaselineNotFound.code(), 2);
        // InvalidBaseline
        assert_eq!(ValidateExitCode::InvalidBaseline.code(), 3);
        // CommandError
        assert_eq!(ValidateExitCode::CommandError.code(), 4);
        // ConfigError
        assert_eq!(ValidateExitCode::ConfigError.code(), 5);
    }

    #[test]
    fn test_exit_code_eq() {
        assert_eq!(ValidateExitCode::Passed, ValidateExitCode::Passed);
        assert_ne!(ValidateExitCode::Passed, ValidateExitCode::Failed);
    }

    #[test]
    fn test_run_validate_empty_command() {
        let config =
            ValidateConfig::default().with_baseline(std::path::PathBuf::from("/nonexistent"));
        // Even with empty command, should reach baseline check first
        let result = run_validate(&[], &config);
        assert_eq!(result, ValidateExitCode::BaselineNotFound);
    }
}
