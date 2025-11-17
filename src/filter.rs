//! Syscall filtering for -e trace= expressions
//!
//! Sprint 9-10: Implement strace-compatible filtering
//! Supports:
//! - Individual syscalls: -e trace=open,read,write
//! - Syscall classes: -e trace=file, -e trace=network, -e trace=process
//!
//! Sprint 15: Advanced filtering with negation
//! Supports:
//! - Negation: -e trace=!close, -e trace=!file
//! - Mixed: -e trace=file,!close

use anyhow::{bail, Result};
use std::collections::HashSet;

/// Syscall filter that determines which syscalls to trace
#[derive(Debug, Clone)]
pub struct SyscallFilter {
    /// Set of syscall names to include (None = all syscalls)
    include: Option<HashSet<String>>,
    /// Set of syscall names to exclude (always applied)
    exclude: HashSet<String>,
}

impl SyscallFilter {
    /// Create a filter that includes all syscalls
    pub fn all() -> Self {
        Self {
            include: None,
            exclude: HashSet::new(),
        }
    }

    /// Parse a filter expression like "trace=open,read,write" or "trace=file"
    pub fn from_expr(expr: &str) -> Result<Self> {
        // Parse trace=XXX format
        if let Some(trace_spec) = expr.strip_prefix("trace=") {
            Self::from_trace_spec(trace_spec)
        } else {
            bail!(
                "Invalid filter expression: {}. Expected format: trace=SPEC",
                expr
            );
        }
    }

    /// Parse a trace specification (the part after "trace=")
    fn from_trace_spec(spec: &str) -> Result<Self> {
        // Sprint 15: Validate spec
        validate_trace_spec(spec)?;

        // Sprint 15: Parse include and exclude sets
        let (include_syscalls, exclude_syscalls, has_includes) = parse_syscall_sets(spec);

        // Sprint 15: If only negations, include all syscalls except excluded
        let include = if has_includes {
            Some(include_syscalls)
        } else {
            None // Trace all except excluded
        };

        Ok(Self {
            include,
            exclude: exclude_syscalls,
        })
    }

    /// Check if a syscall should be traced
    pub fn should_trace(&self, syscall_name: &str) -> bool {
        // Sprint 15: First check exclusions (highest priority)
        if self.exclude.contains(syscall_name) {
            return false;
        }

        // Then check inclusions
        match &self.include {
            None => true, // No filter = trace all (except excluded)
            Some(set) => set.contains(syscall_name),
        }
    }
}

/// Validate trace specification syntax
/// Sprint 15: Extracted to reduce complexity
fn validate_trace_spec(spec: &str) -> Result<()> {
    if spec.is_empty() {
        return Ok(());
    }

    if spec.trim() == "!" {
        bail!("Invalid negation syntax: '!' must be followed by syscall name or class");
    }

    Ok(())
}

/// Parse syscall sets from trace specification
/// Sprint 15: Extracted to reduce complexity
/// Returns (include_set, exclude_set, has_includes)
fn parse_syscall_sets(spec: &str) -> (HashSet<String>, HashSet<String>, bool) {
    let mut include_syscalls = HashSet::new();
    let mut exclude_syscalls = HashSet::new();
    let mut has_includes = false;

    if spec.is_empty() {
        return (include_syscalls, exclude_syscalls, true); // Empty means no includes
    }

    for part in spec.split(',') {
        let part = part.trim();

        // Check for negation prefix
        let (is_negation, syscall_part) = if let Some(s) = part.strip_prefix('!') {
            (true, s)
        } else {
            has_includes = true;
            (false, part)
        };

        // Expand syscall classes or add individual syscall
        let syscalls_to_add = expand_syscall_class(syscall_part);

        if is_negation {
            exclude_syscalls.extend(syscalls_to_add);
        } else {
            include_syscalls.extend(syscalls_to_add);
        }
    }

    (include_syscalls, exclude_syscalls, has_includes)
}

/// Expand a syscall class or return a single syscall name
/// Sprint 15: Extracted to reduce complexity
fn expand_syscall_class(name: &str) -> Vec<String> {
    match name {
        "file" => vec![
            "open",
            "openat",
            "close",
            "read",
            "write",
            "lseek",
            "stat",
            "fstat",
            "newfstatat",
            "access",
            "mkdir",
            "rmdir",
            "unlink",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        "network" => [
            "socket",
            "connect",
            "accept",
            "bind",
            "listen",
            "send",
            "recv",
            "sendto",
            "recvfrom",
            "setsockopt",
            "getsockopt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        "process" => [
            "fork",
            "vfork",
            "clone",
            "execve",
            "exit",
            "exit_group",
            "wait4",
            "waitid",
            "kill",
            "tkill",
            "tgkill",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        "memory" => ["mmap", "munmap", "mprotect", "mremap", "brk", "sbrk"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        _ => vec![name.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_all_traces_everything() {
        let filter = SyscallFilter::all();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("write"));
        assert!(filter.should_trace("anything"));
    }

    #[test]
    fn test_filter_individual_syscalls() {
        let filter = SyscallFilter::from_expr("trace=open,read,write").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("read"));
        assert!(filter.should_trace("write"));
        assert!(!filter.should_trace("close"));
    }

    #[test]
    fn test_filter_file_class() {
        let filter = SyscallFilter::from_expr("trace=file").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("openat"));
        assert!(filter.should_trace("read"));
        assert!(filter.should_trace("write"));
        assert!(!filter.should_trace("socket"));
    }

    #[test]
    fn test_filter_network_class() {
        let filter = SyscallFilter::from_expr("trace=network").unwrap();
        assert!(filter.should_trace("socket"));
        assert!(filter.should_trace("connect"));
        assert!(!filter.should_trace("open"));
    }

    #[test]
    fn test_filter_mixed() {
        let filter = SyscallFilter::from_expr("trace=file,socket").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("socket"));
        assert!(!filter.should_trace("clone"));
    }

    #[test]
    fn test_invalid_expression() {
        let result = SyscallFilter::from_expr("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_process_class() {
        let filter = SyscallFilter::from_expr("trace=process").unwrap();
        assert!(filter.should_trace("fork"));
        assert!(filter.should_trace("clone"));
        assert!(filter.should_trace("execve"));
        assert!(filter.should_trace("exit"));
        assert!(!filter.should_trace("open"));
        assert!(!filter.should_trace("socket"));
    }

    #[test]
    fn test_filter_memory_class() {
        let filter = SyscallFilter::from_expr("trace=memory").unwrap();
        assert!(filter.should_trace("mmap"));
        assert!(filter.should_trace("munmap"));
        assert!(filter.should_trace("mprotect"));
        assert!(filter.should_trace("brk"));
        assert!(!filter.should_trace("open"));
        assert!(!filter.should_trace("fork"));
    }

    #[test]
    fn test_filter_multiple_classes() {
        let filter = SyscallFilter::from_expr("trace=file,network,process").unwrap();
        // File class
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("read"));
        // Network class
        assert!(filter.should_trace("socket"));
        assert!(filter.should_trace("connect"));
        // Process class
        assert!(filter.should_trace("fork"));
        assert!(filter.should_trace("execve"));
        // Not included
        assert!(!filter.should_trace("mmap"));
    }

    #[test]
    fn test_filter_clone() {
        let filter1 = SyscallFilter::from_expr("trace=open,read").unwrap();
        let filter2 = filter1.clone();
        assert!(filter2.should_trace("open"));
        assert!(filter2.should_trace("read"));
        assert!(!filter2.should_trace("write"));
    }

    #[test]
    fn test_filter_debug() {
        let filter = SyscallFilter::all();
        let debug_str = format!("{:?}", filter);
        assert!(debug_str.contains("SyscallFilter"));
    }

    #[test]
    fn test_filter_empty_trace_spec() {
        // Empty spec should create filter with no syscalls
        let filter = SyscallFilter::from_expr("trace=").unwrap();
        // Empty filter should not trace anything (empty HashSet)
        assert!(!filter.should_trace("open"));
        assert!(!filter.should_trace("read"));
    }

    #[test]
    fn test_filter_whitespace_handling() {
        let filter = SyscallFilter::from_expr("trace=open, read , write").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("read"));
        assert!(filter.should_trace("write"));
        assert!(!filter.should_trace("close"));
    }

    // Sprint 15: Negation operator tests
    #[test]
    fn test_negation_single_syscall() {
        let filter = SyscallFilter::from_expr("trace=!close").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("read"));
        assert!(!filter.should_trace("close"));
    }

    #[test]
    fn test_negation_multiple_syscalls() {
        let filter = SyscallFilter::from_expr("trace=!open,!close").unwrap();
        assert!(!filter.should_trace("open"));
        assert!(!filter.should_trace("close"));
        assert!(filter.should_trace("read"));
        assert!(filter.should_trace("write"));
    }

    #[test]
    fn test_negation_syscall_class() {
        let filter = SyscallFilter::from_expr("trace=!file").unwrap();
        assert!(!filter.should_trace("open"));
        assert!(!filter.should_trace("read"));
        assert!(!filter.should_trace("write"));
        assert!(filter.should_trace("socket"));
        assert!(filter.should_trace("fork"));
    }

    #[test]
    fn test_mixed_positive_negative() {
        let filter = SyscallFilter::from_expr("trace=file,!close").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("read"));
        assert!(!filter.should_trace("close")); // Explicitly excluded
        assert!(!filter.should_trace("socket")); // Not in file class
    }

    #[test]
    fn test_negation_invalid_syntax() {
        let result = SyscallFilter::from_expr("trace=!");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid negation syntax"));
    }

    #[test]
    fn test_negation_preserves_original_behavior() {
        // Ensure positive-only filters still work
        let filter = SyscallFilter::from_expr("trace=open,read").unwrap();
        assert!(filter.should_trace("open"));
        assert!(filter.should_trace("read"));
        assert!(!filter.should_trace("write"));
    }

    #[test]
    fn test_expand_syscall_class_file() {
        let syscalls = expand_syscall_class("file");
        assert!(syscalls.contains(&"open".to_string()));
        assert!(syscalls.contains(&"close".to_string()));
        assert!(syscalls.contains(&"read".to_string()));
    }

    #[test]
    fn test_expand_syscall_class_network() {
        let syscalls = expand_syscall_class("network");
        assert!(syscalls.contains(&"socket".to_string()));
        assert!(syscalls.contains(&"connect".to_string()));
    }

    #[test]
    fn test_expand_syscall_class_individual() {
        let syscalls = expand_syscall_class("custom_syscall");
        assert_eq!(syscalls, vec!["custom_syscall".to_string()]);
    }
}
