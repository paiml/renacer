//! DWARF debug info parsing for source correlation
//!
//! Sprint 5-6: Map instruction pointers to source file:line using DWARF .debug_line
//!
//! Simplified implementation for MVP - full gimli integration deferred to future sprints

#![allow(dead_code)]

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Source location information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Source file path
    pub file: String,
    /// Line number
    pub line: u32,
    /// Column number (if available)
    pub column: Option<u32>,
    /// Function name (if available)
    pub function: Option<String>,
}

/// DWARF debug info context for a binary
///
/// Sprint 5-6 MVP: Simplified implementation
/// Full gimli-based parsing will be added in future sprints
pub struct DwarfContext {
    /// Mapping from instruction pointer to source location
    #[allow(dead_code)]
    cache: HashMap<u64, Option<SourceLocation>>,
    /// Binary path
    _binary_path: std::path::PathBuf,
}

impl DwarfContext {
    /// Load DWARF debug info from an ELF binary
    ///
    /// Sprint 5-6 MVP: Validates binary exists, full DWARF parsing deferred
    pub fn load(binary_path: &Path) -> Result<Self> {
        // Verify binary exists
        if !binary_path.exists() {
            anyhow::bail!("Binary does not exist: {}", binary_path.display());
        }

        // Sprint 5-6 MVP: Basic validation only
        // Full implementation will use gimli + addr2line crate for robust parsing
        // This is documented as technical debt for Sprint 7-8

        Ok(Self {
            cache: HashMap::new(),
            _binary_path: binary_path.to_path_buf(),
        })
    }

    /// Look up source location for an instruction pointer
    ///
    /// Sprint 5-6 MVP: Returns None (DWARF parsing deferred)
    /// Full implementation planned for Sprint 7-8
    pub fn lookup(&mut self, _ip: u64) -> Result<Option<SourceLocation>> {
        // MVP: Return None
        // TODO(Sprint 7-8): Implement full DWARF .debug_line parsing
        // Recommended approach: Use addr2line crate which wraps gimli
        // See: https://docs.rs/addr2line/latest/addr2line/
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn compile_test_binary() -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let src_file = temp_dir.path().join("test.rs");
        let bin_file = temp_dir.path().join("test_bin");

        fs::write(&src_file, "fn main() { println!(\"test\"); }").unwrap();

        Command::new("rustc")
            .arg(&src_file)
            .arg("-o")
            .arg(&bin_file)
            .arg("-g")
            .status()
            .unwrap();

        (temp_dir, bin_file)
    }

    #[test]
    fn test_dwarf_context_loads() {
        let (_temp_dir, bin_file) = compile_test_binary();
        let result = DwarfContext::load(&bin_file);
        assert!(result.is_ok(), "Should load DWARF context");
    }

    #[test]
    fn test_dwarf_lookup_returns_option() {
        let (_temp_dir, bin_file) = compile_test_binary();
        let mut ctx = DwarfContext::load(&bin_file).unwrap();
        let result = ctx.lookup(0x1000);
        assert!(result.is_ok(), "Lookup should not crash");
    }
}
