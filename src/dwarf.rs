//! DWARF debug info parsing for source correlation
//!
//! Sprint 5-6: Map instruction pointers to source file:line using DWARF .debug_line
//!
//! Uses addr2line crate for robust DWARF parsing

use anyhow::{Context, Result};
use object::{Object, ObjectSection};
use std::fs::File;
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
/// Sprint 5-6: Full implementation using addr2line crate
pub struct DwarfContext {
    /// addr2line context for DWARF lookups
    context: addr2line::Context<gimli::EndianRcSlice<gimli::RunTimeEndian>>,
}

impl DwarfContext {
    /// Load DWARF debug info from an ELF binary
    ///
    /// Sprint 5-6: Full implementation using addr2line + object crates
    pub fn load(binary_path: &Path) -> Result<Self> {
        // Verify binary exists
        if !binary_path.exists() {
            anyhow::bail!("Binary does not exist: {}", binary_path.display());
        }

        // Open and parse ELF binary
        let file = File::open(binary_path)
            .with_context(|| format!("Failed to open binary: {}", binary_path.display()))?;

        let mmap = unsafe { memmap2::Mmap::map(&file) }
            .context("Failed to memory-map binary")?;

        let object = object::File::parse(&*mmap)
            .context("Failed to parse ELF binary")?;

        // Load DWARF sections from object file
        let endian = if object.is_little_endian() {
            gimli::RunTimeEndian::Little
        } else {
            gimli::RunTimeEndian::Big
        };

        // Helper to load a DWARF section
        let load_section = |id: gimli::SectionId| -> Result<gimli::EndianRcSlice<gimli::RunTimeEndian>> {
            let data = object
                .section_by_name(id.name())
                .and_then(|section| section.uncompressed_data().ok())
                .unwrap_or(std::borrow::Cow::Borrowed(&[]));
            // Convert Cow<[u8]> to Rc<[u8]> by converting to owned Vec first
            let bytes: std::rc::Rc<[u8]> = std::rc::Rc::from(data.into_owned());
            Ok(gimli::EndianRcSlice::new(bytes, endian))
        };

        // Load all DWARF sections
        let dwarf = gimli::Dwarf::load(&load_section)
            .context("Failed to load DWARF sections - binary may not have debug symbols. Compile with -g flag.")?;

        // Create addr2line context from DWARF
        let context = addr2line::Context::from_dwarf(dwarf)
            .context("Failed to create DWARF context")?;

        Ok(Self { context })
    }

    /// Look up source location for an instruction pointer
    ///
    /// Sprint 5-6: Full implementation with DWARF .debug_line parsing
    /// Returns the first valid source location found in DWARF
    pub fn lookup(&self, ip: u64) -> Result<Option<SourceLocation>> {
        // Try multiple IP offsets to find user code
        // At syscall-entry-stop, IP might be in libc, so we try backing up
        for offset in [0, 1, 2, 4, 8, 16] {
            let adjusted_ip = ip.saturating_sub(offset);

            // Look up location in DWARF data
            let location = match self.context.find_location(adjusted_ip) {
                Ok(Some(loc)) => loc,
                Ok(None) => continue,
                Err(_) => continue,
            };

            // Extract file and line
            let file = match location.file {
                Some(f) => {
                    // Filter out libc/std paths - we want user code
                    if f.contains("/rustc/") || f.contains("library/") {
                        continue;
                    }
                    f
                }
                None => continue,
            };

            let line = location.line.unwrap_or(0);
            if line == 0 {
                continue; // Invalid line number
            }

            let column = location.column;

            // Try to find function name using find_frames
            let mut function_name = None;
            let frames_result = self.context.find_frames(adjusted_ip);

            // addr2line returns LookupResult which needs to be handled with load()
            if let Ok(mut frames_iter) = frames_result.skip_all_loads() {
                if let Ok(Some(frame)) = frames_iter.next() {
                    if let Some(func) = frame.function {
                        // Get raw name as Cow<str>
                        let raw_name = func.raw_name().ok();
                        if let Some(name) = raw_name {
                            function_name = Some(name.to_string());
                        }
                    }
                }
            }

            return Ok(Some(SourceLocation {
                file: file.to_string(),
                line,
                column,
                function: function_name,
            }));
        }

        // No valid location found
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
        assert!(result.is_ok(), "Should load DWARF context: {:?}", result.err());
    }

    #[test]
    fn test_dwarf_lookup_returns_option() {
        let (_temp_dir, bin_file) = compile_test_binary();
        let ctx = DwarfContext::load(&bin_file).unwrap();
        let result = ctx.lookup(0x1000);
        assert!(result.is_ok(), "Lookup should not crash: {:?}", result.err());
    }
}
