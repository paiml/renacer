// Sprint 33: W3C Trace Context Propagation
//
// Implements W3C Trace Context standard for distributed tracing.
// Reference: https://www.w3.org/TR/trace-context/
//
// Format: version-trace_id-parent_id-trace_flags
// Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01

use std::fmt;

/// W3C Trace Context (traceparent header)
///
/// Represents distributed trace context passed across service boundaries.
/// Enables Renacer to create syscall spans as children of application spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub version: u8,
    pub trace_id: [u8; 16], // 128-bit trace ID
    pub parent_id: [u8; 8], // 64-bit parent span ID
    pub trace_flags: u8,    // 8-bit flags (01 = sampled)
}

impl TraceContext {
    /// Parse W3C traceparent string
    ///
    /// Format: "00-{trace_id}-{parent_id}-{flags}"
    /// Example: "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
    pub fn parse(traceparent: &str) -> Result<Self, TraceContextError> {
        // Split into 4 parts: version-trace_id-parent_id-flags
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return Err(TraceContextError::InvalidFormat);
        }

        // Parse version (must be "00")
        let version =
            u8::from_str_radix(parts[0], 16).map_err(|_| TraceContextError::InvalidVersion)?;
        if version != 0 {
            return Err(TraceContextError::InvalidVersion);
        }

        // Parse trace_id (32 hex chars = 16 bytes)
        if parts[1].len() != 32 {
            return Err(TraceContextError::InvalidTraceId);
        }
        let trace_id = hex_to_bytes_16(parts[1]).ok_or(TraceContextError::InvalidTraceId)?;

        // Validate trace_id is not all zeros
        if trace_id.iter().all(|&b| b == 0) {
            return Err(TraceContextError::AllZeroTraceId);
        }

        // Parse parent_id (16 hex chars = 8 bytes)
        if parts[2].len() != 16 {
            return Err(TraceContextError::InvalidParentId);
        }
        let parent_id = hex_to_bytes_8(parts[2]).ok_or(TraceContextError::InvalidParentId)?;

        // Validate parent_id is not all zeros
        if parent_id.iter().all(|&b| b == 0) {
            return Err(TraceContextError::AllZeroParentId);
        }

        // Parse trace_flags (2 hex chars = 1 byte)
        if parts[3].len() != 2 {
            return Err(TraceContextError::InvalidTraceFlags);
        }
        let trace_flags =
            u8::from_str_radix(parts[3], 16).map_err(|_| TraceContextError::InvalidTraceFlags)?;

        Ok(TraceContext {
            version,
            trace_id,
            parent_id,
            trace_flags,
        })
    }

    /// Extract trace context from environment variables
    ///
    /// Checks TRACEPARENT and OTEL_TRACEPARENT (in that order)
    pub fn from_env() -> Option<Self> {
        std::env::var("TRACEPARENT")
            .or_else(|_| std::env::var("OTEL_TRACEPARENT"))
            .ok()
            .and_then(|s| Self::parse(&s).ok())
    }

    /// Check if trace is sampled (trace_flags & 0x01)
    pub fn is_sampled(&self) -> bool {
        self.trace_flags & 0x01 != 0
    }

    /// Convert trace_id to OpenTelemetry TraceId
    #[cfg(feature = "otlp")]
    pub fn otel_trace_id(&self) -> opentelemetry::trace::TraceId {
        opentelemetry::trace::TraceId::from_bytes(self.trace_id)
    }

    /// Convert parent_id to OpenTelemetry SpanId
    #[cfg(feature = "otlp")]
    pub fn otel_parent_id(&self) -> opentelemetry::trace::SpanId {
        opentelemetry::trace::SpanId::from_bytes(self.parent_id)
    }
}

impl fmt::Display for TraceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}-{}-{}-{:02x}",
            self.version,
            bytes_to_hex_16(&self.trace_id),
            bytes_to_hex_8(&self.parent_id),
            self.trace_flags
        )
    }
}

/// Trace Context parsing errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceContextError {
    /// Invalid format (must be version-trace_id-parent_id-flags)
    InvalidFormat,
    /// Invalid version (must be 00)
    InvalidVersion,
    /// Invalid trace_id (must be 32 hex chars)
    InvalidTraceId,
    /// Invalid parent_id (must be 16 hex chars)
    InvalidParentId,
    /// Invalid trace_flags (must be 2 hex chars)
    InvalidTraceFlags,
    /// Trace ID is all zeros (forbidden by W3C spec)
    AllZeroTraceId,
    /// Parent ID is all zeros (forbidden by W3C spec)
    AllZeroParentId,
}

impl fmt::Display for TraceContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => write!(
                f,
                "Invalid traceparent format (expected: version-trace_id-parent_id-flags)"
            ),
            Self::InvalidVersion => write!(f, "Invalid version (must be 00)"),
            Self::InvalidTraceId => write!(f, "Invalid trace_id (must be 32 hex characters)"),
            Self::InvalidParentId => write!(f, "Invalid parent_id (must be 16 hex characters)"),
            Self::InvalidTraceFlags => write!(f, "Invalid trace_flags (must be 2 hex characters)"),
            Self::AllZeroTraceId => write!(f, "Trace ID cannot be all zeros"),
            Self::AllZeroParentId => write!(f, "Parent ID cannot be all zeros"),
        }
    }
}

impl std::error::Error for TraceContextError {}

// Helper functions for hex conversion

fn hex_to_bytes_16(hex: &str) -> Option<[u8; 16]> {
    let mut bytes = [0u8; 16];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 16 {
            return None;
        }
        let hex_str = std::str::from_utf8(chunk).ok()?;
        bytes[i] = u8::from_str_radix(hex_str, 16).ok()?;
    }
    Some(bytes)
}

fn hex_to_bytes_8(hex: &str) -> Option<[u8; 8]> {
    let mut bytes = [0u8; 8];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 8 {
            return None;
        }
        let hex_str = std::str::from_utf8(chunk).ok()?;
        bytes[i] = u8::from_str_radix(hex_str, 16).ok()?;
    }
    Some(bytes)
}

fn bytes_to_hex_16(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn bytes_to_hex_8(bytes: &[u8; 8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ============================================================================
// UNIT TESTS (EXTREME TDD - RED Phase)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Test 1: Parse valid traceparent
    #[test]
    fn test_parse_valid_traceparent() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::parse(traceparent).unwrap();

        assert_eq!(ctx.version, 0);
        assert_eq!(ctx.trace_flags, 1);
        assert!(ctx.is_sampled());

        // Verify trace_id
        let expected_trace_id = [
            0x0a, 0xf7, 0x65, 0x19, 0x16, 0xcd, 0x43, 0xdd, 0x84, 0x48, 0xeb, 0x21, 0x1c, 0x80,
            0x31, 0x9c,
        ];
        assert_eq!(ctx.trace_id, expected_trace_id);

        // Verify parent_id
        let expected_parent_id = [0xb7, 0xad, 0x6b, 0x71, 0x69, 0x20, 0x33, 0x31];
        assert_eq!(ctx.parent_id, expected_parent_id);
    }

    // Test 2: Parse another valid traceparent (not sampled)
    #[test]
    fn test_parse_valid_traceparent_not_sampled() {
        let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00";
        let ctx = TraceContext::parse(traceparent).unwrap();

        assert_eq!(ctx.version, 0);
        assert_eq!(ctx.trace_flags, 0);
        assert!(!ctx.is_sampled());
    }

    // Test 3: Invalid format (missing parts)
    #[test]
    fn test_parse_invalid_format_missing_parts() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidFormat));
    }

    // Test 4: Invalid format (too many parts)
    #[test]
    fn test_parse_invalid_format_too_many_parts() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-extra";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidFormat));
    }

    // Test 5: Invalid format (empty string)
    #[test]
    fn test_parse_invalid_format_empty() {
        let result = TraceContext::parse("");
        assert_eq!(result, Err(TraceContextError::InvalidFormat));
    }

    // Test 6: All-zero trace_id
    #[test]
    fn test_parse_all_zero_trace_id() {
        let traceparent = "00-00000000000000000000000000000000-b7ad6b7169203331-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::AllZeroTraceId));
    }

    // Test 7: All-zero parent_id
    #[test]
    fn test_parse_all_zero_parent_id() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::AllZeroParentId));
    }

    // Test 8: Invalid version
    #[test]
    fn test_parse_invalid_version() {
        let traceparent = "99-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidVersion));
    }

    // Test 9: Invalid version (non-hex)
    #[test]
    fn test_parse_invalid_version_non_hex() {
        let traceparent = "XX-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidVersion));
    }

    // Test 10: Invalid trace_id (wrong length)
    #[test]
    fn test_parse_invalid_trace_id_wrong_length() {
        let traceparent = "00-0af7651916cd43dd-b7ad6b7169203331-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidTraceId));
    }

    // Test 11: Invalid trace_id (non-hex)
    #[test]
    fn test_parse_invalid_trace_id_non_hex() {
        let traceparent = "00-ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ-b7ad6b7169203331-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidTraceId));
    }

    // Test 12: Invalid parent_id (wrong length)
    #[test]
    fn test_parse_invalid_parent_id_wrong_length() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidParentId));
    }

    // Test 13: Invalid parent_id (non-hex)
    #[test]
    fn test_parse_invalid_parent_id_non_hex() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-XXXXXXXXXXXXXXXX-01";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidParentId));
    }

    // Test 14: Invalid trace_flags (wrong length)
    #[test]
    fn test_parse_invalid_trace_flags_wrong_length() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-1";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidTraceFlags));
    }

    // Test 15: Invalid trace_flags (non-hex)
    #[test]
    fn test_parse_invalid_trace_flags_non_hex() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-XX";
        let result = TraceContext::parse(traceparent);

        assert_eq!(result, Err(TraceContextError::InvalidTraceFlags));
    }

    // Test 16: is_sampled() with flag = 01
    #[test]
    fn test_is_sampled_flag_set() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::parse(traceparent).unwrap();

        assert!(ctx.is_sampled());
    }

    // Test 17: is_sampled() with flag = 00
    #[test]
    fn test_is_sampled_flag_unset() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-00";
        let ctx = TraceContext::parse(traceparent).unwrap();

        assert!(!ctx.is_sampled());
    }

    // Test 18: Display formatting
    #[test]
    fn test_display_formatting() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::parse(traceparent).unwrap();

        assert_eq!(ctx.to_string(), traceparent);
    }

    // Test 19: from_env() with TRACEPARENT set
    #[test]
    fn test_from_env_traceparent() {
        std::env::set_var(
            "TRACEPARENT",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        );

        let ctx = TraceContext::from_env();
        assert!(ctx.is_some());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.version, 0);
        assert!(ctx.is_sampled());

        std::env::remove_var("TRACEPARENT");
    }

    // Test 20: from_env() with OTEL_TRACEPARENT set
    #[test]
    fn test_from_env_otel_traceparent() {
        std::env::remove_var("TRACEPARENT");
        std::env::set_var(
            "OTEL_TRACEPARENT",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00",
        );

        let ctx = TraceContext::from_env();
        assert!(ctx.is_some());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.version, 0);
        assert!(!ctx.is_sampled());

        std::env::remove_var("OTEL_TRACEPARENT");
    }

    // Test 21: from_env() with no env var set
    #[test]
    fn test_from_env_missing() {
        std::env::remove_var("TRACEPARENT");
        std::env::remove_var("OTEL_TRACEPARENT");

        let ctx = TraceContext::from_env();
        assert!(ctx.is_none());
    }

    // Test 22: from_env() with invalid format
    #[test]
    fn test_from_env_invalid_format() {
        std::env::set_var("TRACEPARENT", "INVALID");

        let ctx = TraceContext::from_env();
        assert!(ctx.is_none());

        std::env::remove_var("TRACEPARENT");
    }

    // Test 23: Clone trait
    #[test]
    fn test_trace_context_clone() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx1 = TraceContext::parse(traceparent).unwrap();
        let ctx2 = ctx1.clone();

        assert_eq!(ctx1, ctx2);
    }

    // Test 24: Debug trait
    #[test]
    fn test_trace_context_debug() {
        let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::parse(traceparent).unwrap();

        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("TraceContext"));
    }

    // Test 25: Error Display trait
    #[test]
    fn test_error_display() {
        let err = TraceContextError::InvalidFormat;
        assert!(err.to_string().contains("Invalid traceparent format"));

        let err = TraceContextError::AllZeroTraceId;
        assert!(err.to_string().contains("all zeros"));
    }
}
