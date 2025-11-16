//! Syscall filtering for -e trace= expressions
//!
//! Sprint 9-10: Implement strace-compatible filtering
//! Supports:
//! - Individual syscalls: -e trace=open,read,write
//! - Syscall classes: -e trace=file, -e trace=network, -e trace=process

use anyhow::{bail, Result};
use std::collections::HashSet;

/// Syscall filter that determines which syscalls to trace
#[derive(Debug, Clone)]
pub struct SyscallFilter {
    /// Set of syscall names to include (None = all syscalls)
    include: Option<HashSet<String>>,
}

impl SyscallFilter {
    /// Create a filter that includes all syscalls
    pub fn all() -> Self {
        Self { include: None }
    }

    /// Parse a filter expression like "trace=open,read,write" or "trace=file"
    pub fn from_expr(expr: &str) -> Result<Self> {
        // Parse trace=XXX format
        if let Some(trace_spec) = expr.strip_prefix("trace=") {
            Self::from_trace_spec(trace_spec)
        } else {
            bail!("Invalid filter expression: {}. Expected format: trace=SPEC", expr);
        }
    }

    /// Parse a trace specification (the part after "trace=")
    fn from_trace_spec(spec: &str) -> Result<Self> {
        let mut syscalls = HashSet::new();

        for part in spec.split(',') {
            let part = part.trim();

            // Check for syscall classes
            match part {
                "file" => {
                    // File operations
                    syscalls.extend([
                        "open", "openat", "close", "read", "write",
                        "lseek", "stat", "fstat", "newfstatat",
                        "access", "mkdir", "rmdir", "unlink",
                    ].iter().map(|s| s.to_string()));
                }
                "network" => {
                    // Network operations
                    syscalls.extend([
                        "socket", "connect", "accept", "bind", "listen",
                        "send", "recv", "sendto", "recvfrom",
                        "setsockopt", "getsockopt",
                    ].iter().map(|s| s.to_string()));
                }
                "process" => {
                    // Process operations
                    syscalls.extend([
                        "fork", "vfork", "clone", "execve",
                        "exit", "exit_group", "wait4", "waitid",
                        "kill", "tkill", "tgkill",
                    ].iter().map(|s| s.to_string()));
                }
                "memory" => {
                    // Memory operations
                    syscalls.extend([
                        "mmap", "munmap", "mprotect", "mremap",
                        "brk", "sbrk",
                    ].iter().map(|s| s.to_string()));
                }
                _ => {
                    // Individual syscall name
                    syscalls.insert(part.to_string());
                }
            }
        }

        Ok(Self {
            include: Some(syscalls),
        })
    }

    /// Check if a syscall should be traced
    pub fn should_trace(&self, syscall_name: &str) -> bool {
        match &self.include {
            None => true, // No filter = trace all
            Some(set) => set.contains(syscall_name),
        }
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
}
