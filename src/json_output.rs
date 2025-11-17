//! JSON output format for syscall traces
//!
//! Sprint 9-10: --format json implementation

use serde::{Deserialize, Serialize};

/// Source location information for a syscall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSourceLocation {
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

/// A single syscall event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSyscall {
    /// Syscall name (e.g., "openat", "read")
    pub name: String,
    /// Arguments as formatted strings
    pub args: Vec<String>,
    /// Return value (may be negative for errors)
    pub result: i64,
    /// Duration in microseconds (0 if timing not enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_us: Option<u64>,
    /// Source location (if --source enabled and available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<JsonSourceLocation>,
}

/// Summary statistics for the trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSummary {
    /// Total number of syscalls traced
    pub total_syscalls: u64,
    /// Total time in microseconds (if timing enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_time_us: Option<u64>,
    /// Exit code of traced process
    pub exit_code: i32,
}

/// Root JSON output structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonOutput {
    /// Format version identifier
    pub version: String,
    /// Format name
    pub format: String,
    /// List of syscall events
    pub syscalls: Vec<JsonSyscall>,
    /// Summary statistics
    pub summary: JsonSummary,
}

impl JsonOutput {
    /// Create a new JSON output structure
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            format: "renacer-json-v1".to_string(),
            syscalls: Vec::new(),
            summary: JsonSummary {
                total_syscalls: 0,
                total_time_us: None,
                exit_code: 0,
            },
        }
    }

    /// Add a syscall to the output
    pub fn add_syscall(&mut self, syscall: JsonSyscall) {
        self.summary.total_syscalls += 1;
        if let Some(duration) = syscall.duration_us {
            *self.summary.total_time_us.get_or_insert(0) += duration;
        }
        self.syscalls.push(syscall);
    }

    /// Set the exit code
    pub fn set_exit_code(&mut self, code: i32) {
        self.summary.exit_code = code;
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

impl Default for JsonOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_output_creation() {
        let output = JsonOutput::new();
        assert_eq!(output.format, "renacer-json-v1");
        assert_eq!(output.syscalls.len(), 0);
        assert_eq!(output.summary.total_syscalls, 0);
    }

    #[test]
    fn test_add_syscall() {
        let mut output = JsonOutput::new();
        let syscall = JsonSyscall {
            name: "write".to_string(),
            args: vec!["1".to_string(), "\"hello\"".to_string(), "5".to_string()],
            result: 5,
            duration_us: Some(100),
            source: None,
        };

        output.add_syscall(syscall);
        assert_eq!(output.summary.total_syscalls, 1);
        assert_eq!(output.summary.total_time_us, Some(100));
    }

    #[test]
    fn test_json_serialization() {
        let mut output = JsonOutput::new();
        output.add_syscall(JsonSyscall {
            name: "openat".to_string(),
            args: vec![
                "0xffffff9c".to_string(),
                "\"/tmp/test\"".to_string(),
                "0x2".to_string(),
            ],
            result: 3,
            duration_us: None,
            source: Some(JsonSourceLocation {
                file: "main.rs".to_string(),
                line: 42,
                function: Some("main".to_string()),
            }),
        });
        output.set_exit_code(0);

        let json = output.to_json().unwrap();
        assert!(json.contains("\"name\": \"openat\""));
        assert!(json.contains("\"format\": \"renacer-json-v1\""));
        assert!(json.contains("\"file\": \"main.rs\""));
        assert!(json.contains("\"line\": 42"));
    }

    #[test]
    fn test_optional_fields_omitted() {
        let syscall = JsonSyscall {
            name: "read".to_string(),
            args: vec!["3".to_string()],
            result: 10,
            duration_us: None,
            source: None,
        };

        let json = serde_json::to_string(&syscall).unwrap();
        // Optional None fields should be omitted
        assert!(!json.contains("duration_us"));
        assert!(!json.contains("source"));
    }
}
