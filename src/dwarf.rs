//! DWARF debug info parsing for source correlation
//!
//! Sprint 5-6: Map instruction pointers to source file:line using DWARF .debug_line
//!
//! MVP Implementation: Stub that allows --source flag to work without crashing

use anyhow::{Context, Result};
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
pub struct DwarfContext {
    /// Mapping from instruction pointer to source location
    cache: HashMap<u64, Option<SourceLocation>>,
    /// Binary path for future DWARF parsing
    _binary_path: std::path::PathBuf,
}

impl DwarfContext {
    /// Load DWARF debug info from an ELF binary
    ///
    /// Sprint 5-6 MVP: This is a stub implementation that doesn't crash
    /// Full implementation will use gimli to parse .debug_line sections
    pub fn load(binary_path: &Path) -> Result<Self> {
        // Verify binary exists
        if !binary_path.exists() {
            anyhow::bail!("Binary does not exist: {}", binary_path.display());
        }

        Ok(Self {
            cache: HashMap::new(),
            _binary_path: binary_path.to_path_buf(),
        })
    }

    /// Look up source location for an instruction pointer
    ///
    /// Sprint 5-6 MVP: Returns None (DWARF parsing not yet implemented)
    /// This allows --source flag to work without crashing
    pub fn lookup(&mut self, _ip: u64) -> Result<Option<SourceLocation>> {
        // MVP: Always return None
        // Full implementation will:
        // 1. Parse .debug_line section with gimli
        // 2. Binary search line number program
        // 3. Return file:line mapping
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
